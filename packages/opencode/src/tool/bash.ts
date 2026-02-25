import z from "zod"
import { Tool } from "./tool"
import path from "path"
import DESCRIPTION from "./bash.txt"
import { Log } from "../util/log"
import { Instance } from "../project/instance"

import { $ } from "bun"
import { Filesystem } from "@/util/filesystem"
import { Flag } from "@/flag/flag.ts"
import { Shell } from "@/shell/shell"

import { BashArity } from "@/permission/arity"
import { Truncate } from "./truncation"
import { Plugin } from "@/plugin"
import { parseBashCommand, streamCommand, streamKill, streamRead, streamStart, streamWrite } from "../core/native"

const MAX_METADATA_LENGTH = 30_000
const DEFAULT_TIMEOUT = Flag.OPENCODE_EXPERIMENTAL_BASH_DEFAULT_TIMEOUT_MS || 2 * 60 * 1000

export const log = Log.create({ service: "bash-tool" })

function shellArgs(shell: string, command: string) {
  if (process.platform !== "win32") return { command: shell, args: ["-lc", command] }
  const name = path.basename(shell).toLowerCase()
  if (name.includes("powershell") || name.includes("pwsh")) {
    return { command: shell, args: ["-NoProfile", "-Command", command] }
  }
  return { command: shell, args: ["/d", "/s", "/c", command] }
}

// TODO: we may wanna rename this tool so it works better on other shells
export const BashTool = Tool.define("bash", async () => {
  const shell = Shell.acceptable()
  log.info("bash tool using shell", { shell })

  return {
    description: DESCRIPTION.replaceAll("${directory}", Instance.directory)
      .replaceAll("${maxLines}", String(Truncate.MAX_LINES))
      .replaceAll("${maxBytes}", String(Truncate.MAX_BYTES)),
    parameters: z.object({
      command: z.string().describe("The command to execute"),
      timeout: z.number().describe("Optional timeout in milliseconds").optional(),
      stdin: z.string().describe("Optional stdin payload to send before reading output").optional(),
      mode: z.enum(["incremental", "oneshot"]).describe("Streaming mode").optional(),
      workdir: z
        .string()
        .describe(
          `The working directory to run the command in. Defaults to ${Instance.directory}. Use this instead of 'cd' commands.`,
        )
        .optional(),
      description: z
        .string()
        .describe(
          "Clear, concise description of what this command does in 5-10 words. Examples:\nInput: ls\nOutput: Lists files in current directory\n\nInput: git status\nOutput: Shows working tree status\n\nInput: npm install\nOutput: Installs package dependencies\n\nInput: mkdir foo\nOutput: Creates directory 'foo'",
        ),
    }),
    async execute(params, ctx) {
      const cwd = params.workdir || Instance.directory
      if (params.timeout !== undefined && params.timeout < 0) {
        throw new Error(`Invalid timeout value: ${params.timeout}. Timeout must be a positive number.`)
      }
      const timeout = params.timeout ?? DEFAULT_TIMEOUT
      const mode = params.mode ?? "incremental"
      const directories = new Set<string>()
      if (!Instance.containsPath(cwd)) directories.add(cwd)
      const patterns = new Set<string>()
      const always = new Set<string>()

      for (const cmd of parseBashCommand(params.command).commands) {
        const commandText = cmd.text
        const command = cmd.command

        // not an exhaustive list, but covers most common cases
        if (command.length && ["cd", "rm", "cp", "mv", "mkdir", "touch", "chmod", "chown", "cat"].includes(command[0])) {
          for (const arg of command.slice(1)) {
            if (
              arg.startsWith("-") ||
              arg === ">" ||
              arg === ">>" ||
              arg === "<" ||
              arg === "2>" ||
              arg === "1>" ||
              arg === "2>>" ||
              (command[0] === "chmod" && arg.startsWith("+"))
            ) {
              continue
            }
            const resolved = await $`realpath ${arg}`
              .cwd(cwd)
              .quiet()
              .nothrow()
              .text()
              .then((x) => x.trim())
            log.info("resolved path", { arg, resolved })
            if (resolved) {
              const normalized =
                process.platform === "win32" ? Filesystem.windowsPath(resolved).replace(/\//g, "\\") : resolved
              if (!Instance.containsPath(normalized)) {
                const dir = (await Filesystem.isDir(normalized)) ? normalized : path.dirname(normalized)
                directories.add(dir)
              }
            }
          }
        }

        // cd covered by above check
        if (command.length && command[0] !== "cd") {
          patterns.add(commandText)
          always.add(BashArity.prefix(command).join(" ") + " *")
        }
      }

      if (directories.size > 0) {
        const globs = Array.from(directories).map((dir) => {
          // Preserve POSIX-looking paths with /s, even on Windows
          if (dir.startsWith("/")) return `${dir.replace(/[\\/]+$/, "")}/*`
          return path.join(dir, "*")
        })
        await ctx.ask({
          permission: "external_directory",
          patterns: globs,
          always: globs,
          metadata: {},
        })
      }

      if (patterns.size > 0) {
        await ctx.ask({
          permission: "bash",
          patterns: Array.from(patterns),
          always: Array.from(always),
          metadata: {},
        })
      }

      const shellEnv = await Plugin.trigger(
        "shell.env",
        { cwd, sessionID: ctx.sessionID, callID: ctx.callID },
        { env: {} },
      )
      const env = Object.fromEntries(
        Object.entries({
          ...process.env,
          ...shellEnv.env,
        }).filter((item): item is [string, string] => typeof item[1] === "string"),
      )

      let output = ""
      let exit: number | undefined
      let reason: "exit" | "timeout" | "killed" | undefined

      // Initialize metadata with empty output
      ctx.metadata({
        metadata: {
          output: "",
          description: params.description,
        },
      })

      const append = (chunk: string) => {
        output += chunk
        ctx.metadata({
          metadata: {
            // truncate the metadata to avoid GIANT blobs of data (has nothing to do w/ what agent can access)
            output: output.length > MAX_METADATA_LENGTH ? output.slice(0, MAX_METADATA_LENGTH) + "\n\n..." : output,
            description: params.description,
          },
        })
      }

      const runtime = shellArgs(shell, params.command)
      if (mode === "oneshot") {
        const single = streamCommand({
          command: runtime.command,
          args: runtime.args,
          cwd,
          env,
          timeoutMs: timeout,
          stdinMode: params.stdin ? "piped" : "null",
        })
        output = single.data
        exit = single.exitCode
        reason = single.completeReason
      }

      let aborted = false
      if (mode !== "oneshot") {
        const session = streamStart({
          command: runtime.command,
          args: runtime.args,
          cwd,
          env,
          timeoutMs: timeout,
          chunkSize: 64 * 1024,
          stdinMode: params.stdin ? "piped" : "null",
        })
        if (params.stdin) {
          streamWrite(session.id, params.stdin)
          streamWrite(session.id, "", true)
        }
        while (true) {
          if (ctx.abort.aborted && !aborted) {
            aborted = true
            streamKill(session.id)
          }

          const read = streamRead(session.id, 128, 100)
          for (const chunk of read.chunks) {
            if (chunk.data) append(chunk.data)
            if (chunk.isComplete) {
              exit = chunk.exitCode
              reason = chunk.completeReason
            }
          }
          if (read.isComplete) break
        }
      }

      const resultMetadata: string[] = []

      if (reason === "timeout") {
        resultMetadata.push(`bash tool terminated command after exceeding timeout ${timeout} ms`)
      }

      if (aborted) {
        resultMetadata.push("User aborted the command")
      }

      if (resultMetadata.length > 0) {
        output += "\n\n<bash_metadata>\n" + resultMetadata.join("\n") + "\n</bash_metadata>"
      }

      return {
        title: params.description,
        metadata: {
          output: output.length > MAX_METADATA_LENGTH ? output.slice(0, MAX_METADATA_LENGTH) + "\n\n..." : output,
          exit,
          description: params.description,
        },
        output,
      }
    },
  }
})
