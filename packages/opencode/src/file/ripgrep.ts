// Ripgrep-compatible utility functions backed by Rust core
import { listFiles, renderTree, searchContentAdvanced } from "@/core/native"
import path from "path"
import fs from "fs/promises"
import z from "zod"
import { Log } from "@/util/log"

export namespace Ripgrep {
  const log = Log.create({ service: "ripgrep" })
  const Stats = z.object({
    elapsed: z.object({
      secs: z.number(),
      nanos: z.number(),
      human: z.string(),
    }),
    searches: z.number(),
    searches_with_match: z.number(),
    bytes_searched: z.number(),
    bytes_printed: z.number(),
    matched_lines: z.number(),
    matches: z.number(),
  })

  const Begin = z.object({
    type: z.literal("begin"),
    data: z.object({
      path: z.object({
        text: z.string(),
      }),
    }),
  })

  export const Match = z.object({
    type: z.literal("match"),
    data: z.object({
      path: z.object({
        text: z.string(),
      }),
      lines: z.object({
        text: z.string(),
      }),
      line_number: z.number(),
      absolute_offset: z.number(),
      submatches: z.array(
        z.object({
          match: z.object({
            text: z.string(),
          }),
          start: z.number(),
          end: z.number(),
        }),
      ),
    }),
  })

  const End = z.object({
    type: z.literal("end"),
    data: z.object({
      path: z.object({
        text: z.string(),
      }),
      binary_offset: z.number().nullable(),
      stats: Stats,
    }),
  })

  const Summary = z.object({
    type: z.literal("summary"),
    data: z.object({
      elapsed_total: z.object({
        human: z.string(),
        nanos: z.number(),
        secs: z.number(),
      }),
      stats: Stats,
    }),
  })

  const Result = z.union([Begin, Match, End, Summary])

  export type Result = z.infer<typeof Result>
  export type Match = z.infer<typeof Match>
  export type Begin = z.infer<typeof Begin>
  export type End = z.infer<typeof End>
  export type Summary = z.infer<typeof Summary>

  export async function filepath() {
    return "native-rust"
  }

  export async function* files(input: {
    cwd: string
    glob?: string[]
    hidden?: boolean
    follow?: boolean
    maxDepth?: number
    signal?: AbortSignal
  }) {
    input.signal?.throwIfAborted()

    if (!(await fs.stat(input.cwd).catch(() => undefined))?.isDirectory()) {
      throw Object.assign(new Error(`No such file or directory: '${input.cwd}'`), {
        code: "ENOENT",
        errno: -2,
        path: input.cwd,
      })
    }

    const globs = ["!.git/*", ...(input.glob ?? [])]
    const result = listFiles(input.cwd, globs, input.hidden !== false, input.follow, input.maxDepth)

    for (const file of result.files) {
      input.signal?.throwIfAborted()
      yield file
    }

    input.signal?.throwIfAborted()
  }

  export async function tree(input: { cwd: string; limit?: number; signal?: AbortSignal }) {
    input.signal?.throwIfAborted()
    log.info("tree", input)
    const output = renderTree(input.cwd, input.limit, true, false)
    input.signal?.throwIfAborted()
    return output
  }

  export async function search(input: {
    cwd: string
    pattern: string
    glob?: string[]
    limit?: number
    follow?: boolean
  }): Promise<z.infer<typeof Match>["data"][]> {
    const globs = ["!.git/*", ...(input.glob ?? [])]
    const result = searchContentAdvanced(
      input.pattern,
      input.cwd,
      globs,
      true,
      input.follow,
      undefined,
      input.limit,
      undefined,
    )

    return result.matches.map((item) => {
      const parsed = {
        type: "match",
        data: {
          path: { text: path.join(input.cwd, item.path) },
          lines: { text: item.lineText },
          line_number: item.lineNum,
          absolute_offset: item.absoluteOffset,
          submatches: item.submatches.map((m) => ({
            match: { text: m.text },
            start: m.start,
            end: m.end,
          })),
        },
      }

      return Match.parse(parsed).data
    })
  }
}
