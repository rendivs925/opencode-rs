import { glob, globParallel } from "@/core/native"
import { minimatch } from "minimatch"
import fs from "fs"
import path from "path"

export namespace Glob {
  export interface Options {
    cwd?: string
    absolute?: boolean
    include?: "file" | "all"
    dot?: boolean
    symlink?: boolean
  }

  function normalizeRust(results: string[], options: Options) {
    const cwd = path.resolve(options.cwd ?? ".")
    const abs = results.map((item) => (path.isAbsolute(item) ? item : path.resolve(item)))
    const filtered =
      options.include === "all"
        ? abs
        : abs.filter((item) => {
            const stat = fs.statSync(item, { throwIfNoEntry: false })
            return !!stat?.isFile()
          })
    if (options.absolute) return filtered
    return filtered.map((item) => path.relative(cwd, item).split(path.sep).join("/"))
  }

  export async function scan(pattern: string, options: Options = {}): Promise<string[]> {
    const results = glob(pattern, options.cwd ?? ".", 10, options.dot ?? false, options.symlink ?? false)
    return normalizeRust(results, options)
  }

  export function scanSync(pattern: string, options: Options = {}): string[] {
    const results = globParallel(pattern, options.cwd ?? ".", 10, options.dot ?? false, options.symlink ?? false)
    return normalizeRust(results, options)
  }

  export function match(pattern: string, filepath: string): boolean {
    return minimatch(filepath, pattern, { dot: true })
  }
}
