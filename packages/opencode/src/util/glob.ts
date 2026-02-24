import { glob, globSync, type GlobOptions } from "glob"
import { minimatch } from "minimatch"

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
    if (options.useRust && core?.glob) {
      return core.glob(pattern, options.cwd ?? ".", 10, options.dot ?? false)
    }
    return glob(pattern, toGlobOptions(options)) as Promise<string[]>
  }

  export function scanSync(pattern: string, options: Options = {}): string[] {
    if (options.useRust && core?.globParallel) {
      return core.globParallel(pattern, options.cwd ?? ".", 10, options.dot ?? false)
    }
    return globSync(pattern, toGlobOptions(options)) as string[]
  }

  export function match(pattern: string, filepath: string): boolean {
    return minimatch(filepath, pattern, { dot: true })
  }
}
