import z from "zod"
import { Tool } from "./tool"
import { searchContentContext, searchContentRendered } from "@/core/native"

import DESCRIPTION from "./grep.txt"
import { Instance } from "../project/instance"
import path from "path"
import { assertExternalDirectory } from "./external-directory"

const MAX_LINE_LENGTH = 2000

export const GrepTool = Tool.define("grep", {
  description: DESCRIPTION,
  parameters: z.object({
    pattern: z.string().describe("The regex pattern to search for in file contents"),
    path: z.string().optional().describe("The directory to search in. Defaults to the current working directory."),
    include: z.string().optional().describe('File pattern to include in the search (e.g. "*.js", "*.{ts,tsx}")'),
  }),
  async execute(params, ctx) {
    if (!params.pattern) {
      throw new Error("pattern is required")
    }

    await ctx.ask({
      permission: "grep",
      patterns: [params.pattern],
      always: ["*"],
      metadata: {
        pattern: params.pattern,
        path: params.path,
        include: params.include,
      },
    })

    let searchPath = params.path ?? Instance.directory
    searchPath = path.isAbsolute(searchPath) ? searchPath : path.resolve(Instance.directory, searchPath)
    await assertExternalDirectory(ctx, searchPath, { kind: "directory" })

    const limit = 100
    const context = searchContentContext(params.pattern, searchPath, params.include ? [params.include] : undefined, true, false, undefined, limit, MAX_LINE_LENGTH, 1, 1)
    if (context.totalMatches > 0) {
      const output = context.matches
        .slice(0, limit)
        .map((item) => {
          const before = item.before.map((line, idx) => `  ${item.lineNum - item.before.length + idx}: ${line}`).join("\n")
          const match = `  ${item.lineNum}: ${item.lineText}`
          const after = item.after.map((line, idx) => `  ${item.lineNum + idx + 1}: ${line}`).join("\n")
          return [`${item.path}:`, before, match, after].filter(Boolean).join("\n")
        })
        .join("\n\n")
      return {
        title: params.pattern,
        metadata: { matches: context.totalMatches, truncated: context.totalMatches > context.matches.length },
        output: `Found ${context.totalMatches} matches\n\n${output}`,
      }
    }

    const result = searchContentRendered(params.pattern, searchPath, params.include, limit, MAX_LINE_LENGTH)
    if (result.displayedMatches === 0) {
      return {
        title: params.pattern,
        metadata: { matches: 0, truncated: false },
        output: "No files found",
      }
    }

    return {
      title: params.pattern,
      metadata: {
        matches: result.totalMatches,
        truncated: result.truncated,
      },
      output: result.output,
    }
  },
})
