import { glob, globSync, type GlobOptions } from "glob"
import { minimatch } from "minimatch"
import fs from "fs"
import path from "path"

let core: typeof import("@opencode-ai/core") | undefined
try {
  core = await import("@opencode-ai/core")
} catch {}

export namespace Glob {
  export interface Options {
    cwd?: string
    absolute?: boolean
    include?: "file" | "all"
    dot?: boolean
    symlink?: boolean
    useRust?: boolean
  }

  function supportsRust(pattern: string, options: Options) {
    if (options.symlink) return false
    if (/[{}[\]()!+@]/.test(pattern)) return false
    return true
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

  function toGlobOptions(options: Options): GlobOptions {
    return {
      cwd: options.cwd,
      absolute: options.absolute,
      dot: options.dot,
      follow: options.symlink ?? false,
      nodir: options.include !== "all",
    }
  }

  export async function scan(pattern: string, options: Options = {}): Promise<string[]> {
    const rust = options.useRust ?? true
    if (rust && core?.glob && supportsRust(pattern, options)) {
      const results = core.glob(pattern, options.cwd ?? ".", 10, options.dot ?? false)
      return normalizeRust(results, options)
    }
    return glob(pattern, toGlobOptions(options)) as Promise<string[]>
  }

  export function scanSync(pattern: string, options: Options = {}): string[] {
    const rust = options.useRust ?? true
    if (rust && core?.globParallel && supportsRust(pattern, options)) {
      const results = core.globParallel(pattern, options.cwd ?? ".", 10, options.dot ?? false)
      return normalizeRust(results, options)
    }
    return globSync(pattern, toGlobOptions(options)) as string[]
  }

  export function match(pattern: string, filepath: string): boolean {
    return minimatch(filepath, pattern, { dot: true })
  }
}
