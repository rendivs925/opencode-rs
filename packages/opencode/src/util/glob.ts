import { glob, globParallel, isIgnored, readGlobCache, writeGlobCache } from "@/core/native"
import fs from "fs"
import path from "path"
import os from "os"
import { xdgCache } from "xdg-basedir"

export namespace Glob {
  export interface Options {
    cwd?: string
    absolute?: boolean
    include?: "file" | "all"
    dot?: boolean
    symlink?: boolean
    cache?: boolean
  }

  const DIR = path.join(xdgCache || process.env.XDG_CACHE_HOME || path.join(os.homedir(), ".cache"), "opencode", "rust-glob")

  function key(pattern: string, options: Options) {
    const cwd = path.resolve(options.cwd ?? ".")
    const stat = fs.statSync(cwd)
    return [
      cwd,
      pattern,
      options.absolute ? "abs" : "rel",
      options.include ?? "file",
      options.dot ? "dot" : "nodot",
      options.symlink ? "follow" : "nofollow",
      stat.size,
      stat.mtimeMs,
    ].join("|")
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
    if (options.cache !== false) {
      const k = key(pattern, options)
      const cached = readGlobCache(DIR, k)
      if (cached != null) return cached
      const results = glob(pattern, options.cwd ?? ".", 10, options.dot ?? false, options.symlink ?? false)
      const normalized = normalizeRust(results, options)
      writeGlobCache(DIR, k, normalized)
      return normalized
    }
    const results = glob(pattern, options.cwd ?? ".", 10, options.dot ?? false, options.symlink ?? false)
    return normalizeRust(results, options)
  }

  export function scanSync(pattern: string, options: Options = {}): string[] {
    if (options.cache !== false) {
      const k = key(pattern, options)
      const cached = readGlobCache(DIR, k)
      if (cached != null) return cached
      const results = globParallel(pattern, options.cwd ?? ".", 10, options.dot ?? false, options.symlink ?? false)
      const normalized = normalizeRust(results, options)
      writeGlobCache(DIR, k, normalized)
      return normalized
    }
    const results = globParallel(pattern, options.cwd ?? ".", 10, options.dot ?? false, options.symlink ?? false)
    return normalizeRust(results, options)
  }

  export function match(pattern: string, filepath: string): boolean {
    return isIgnored(filepath, [pattern])
  }
}
