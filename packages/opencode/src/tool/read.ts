import z from "zod"
import * as fs from "fs/promises"
import * as path from "path"
import { Tool } from "./tool"
import { LSP } from "../lsp"
import { FileTime } from "../file/time"
import DESCRIPTION from "./read.txt"
import { Instance } from "../project/instance"
import { assertExternalDirectory } from "./external-directory"
import { InstructionPrompt } from "../session/instruction"
import { Filesystem } from "../util/filesystem"
import { readAttachment, readDirWindow, readFileWindow } from "@/core/native"

const DEFAULT_READ_LIMIT = 2000
const MAX_LINE_LENGTH = 2000
const MAX_BYTES = 50 * 1024
const MAX_BYTES_LABEL = `${MAX_BYTES / 1024} KB`

export const ReadTool = Tool.define("read", {
  description: DESCRIPTION,
  parameters: z.object({
    filePath: z.string().describe("The absolute path to the file or directory to read"),
    offset: z.coerce.number().describe("The line number to start reading from (1-indexed)").optional(),
    limit: z.coerce.number().describe("The maximum number of lines to read (defaults to 2000)").optional(),
  }),
  async execute(params, ctx) {
    if (params.offset !== undefined && params.offset < 1) {
      throw new Error("offset must be greater than or equal to 1")
    }
    let filepath = params.filePath
    if (!path.isAbsolute(filepath)) {
      filepath = path.resolve(Instance.directory, filepath)
    }
    const title = path.relative(Instance.worktree, filepath)

    const stat = Filesystem.stat(filepath)

    await assertExternalDirectory(ctx, filepath, {
      bypass: Boolean(ctx.extra?.["bypassCwdCheck"]),
      kind: stat?.isDirectory() ? "directory" : "file",
    })

    await ctx.ask({
      permission: "read",
      patterns: [filepath],
      always: ["*"],
      metadata: {},
    })

    if (!stat) {
      const dir = path.dirname(filepath)
      const base = path.basename(filepath)

      const suggestions = await fs
        .readdir(dir)
        .then((entries) =>
          entries
            .filter(
              (entry) =>
                entry.toLowerCase().includes(base.toLowerCase()) || base.toLowerCase().includes(entry.toLowerCase()),
            )
            .map((entry) => path.join(dir, entry))
            .slice(0, 3),
        )
        .catch(() => [])

      if (suggestions.length > 0) {
        throw new Error(`File not found: ${filepath}\n\nDid you mean one of these?\n${suggestions.join("\n")}`)
      }

      throw new Error(`File not found: ${filepath}`)
    }

    if (stat.isDirectory()) {
      const limit = params.limit ?? DEFAULT_READ_LIMIT
      const offset = params.offset ?? 1
      const window = readDirWindow(filepath, offset, limit)
      const sliced = window.entries
      const truncated = window.truncated

      const output = [
        `<path>${filepath}</path>`,
        `<type>directory</type>`,
        `<entries>`,
        sliced.join("\n"),
        truncated
          ? `\n(Showing ${sliced.length} of ${window.totalEntries} entries. Use 'offset' parameter to read beyond entry ${window.nextOffset})`
          : `\n(${window.totalEntries} entries)`,
        `</entries>`,
      ].join("\n")

      return {
        title,
        output,
        metadata: {
          preview: sliced.slice(0, 20).join("\n"),
          truncated,
          loaded: [] as string[],
        },
      }
    }

    const instructions = await InstructionPrompt.resolve(ctx.messages, filepath, ctx.messageID)

    const attachment = readAttachment(filepath)
    if (attachment.isAttachment && attachment.base64 && attachment.mimeType) {
      const msg = `${attachment.mimeType === "application/pdf" ? "PDF" : "Image"} read successfully`
      return {
        title,
        output: msg,
        metadata: {
          preview: msg,
          truncated: false,
          loaded: instructions.map((i) => i.filepath),
        },
        attachments: [
          {
            type: "file",
            mime: attachment.mimeType,
            url: `data:${attachment.mimeType};base64,${attachment.base64}`,
          },
        ],
      }
    }

    const limit = params.limit ?? DEFAULT_READ_LIMIT
    const offset = params.offset ?? 1
    const window = readFileWindow(filepath, offset, limit, MAX_BYTES, MAX_LINE_LENGTH)

    const content = window.lines.map((line, index) => {
      return `${index + offset}: ${line}`
    })
    const preview = window.lines.slice(0, 20).join("\n")

    let output = [`<path>${filepath}</path>`, `<type>file</type>`, "<content>"].join("\n")
    output += content.join("\n")

    const totalLines = window.totalLines
    const lastReadLine = offset + window.lines.length - 1
    const nextOffset = window.nextOffset
    const truncated = window.truncated

    if (window.truncatedByBytes) {
      output += `\n\n(Output capped at ${MAX_BYTES_LABEL}. Showing lines ${offset}-${lastReadLine}. Use offset=${nextOffset} to continue.)`
    } else if (window.truncated) {
      output += `\n\n(Showing lines ${offset}-${lastReadLine} of ${totalLines}. Use offset=${nextOffset} to continue.)`
    } else {
      output += `\n\n(End of file - total ${totalLines} lines)`
    }
    output += "\n</content>"

    // just warms the lsp client
    LSP.touchFile(filepath, false)
    FileTime.read(ctx.sessionID, filepath)

    if (instructions.length > 0) {
      output += `\n\n<system-reminder>\n${instructions.map((i) => i.content).join("\n\n")}\n</system-reminder>`
    }

    return {
      title,
      output,
      metadata: {
        preview,
        truncated,
        loaded: instructions.map((i) => i.filepath),
      },
    }
  },
})
