import {
  clearCache,
  deleteCache,
  fastHash,
  globCompiledCacheClear,
  globCompiledCacheSize,
  globMatchCompiled,
  globParallel,
  globScanParallel,
  globScanCompiled,
  readGlobCache,
  writeGlobCache,
} from "@/core/native"
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
    return fastHash(
      [
      cwd,
      pattern,
      options.absolute ? "abs" : "rel",
      options.include ?? "file",
      options.dot ? "dot" : "nodot",
      options.symlink ? "follow" : "nofollow",
      stat.size,
      stat.mtimeMs,
      ].join("|"),
    )
  }

  function maybeTrimCoreCache() {
    if (globCompiledCacheSize() <= 512) return
    globCompiledCacheClear()
  }

  export async function scan(pattern: string, options: Options = {}): Promise<string[]> {
    const includeAll = options.include === "all"
    if (options.cache !== false) {
      const k = key(pattern, options)
      const cached = readGlobCache(DIR, k)
      if (cached != null) return cached
      const results = globScanCompiled(
        pattern,
        options.cwd ?? ".",
        10,
        options.dot ?? false,
        options.symlink ?? false,
        includeAll,
        options.absolute ?? false,
        false,
      )
      deleteCache(DIR, k)
      writeGlobCache(DIR, k, results)
      maybeTrimCoreCache()
      return results
    }
    return globScanCompiled(
      pattern,
      options.cwd ?? ".",
      10,
      options.dot ?? false,
      options.symlink ?? false,
      includeAll,
      options.absolute ?? false,
      false,
    )
  }

  export function scanSync(pattern: string, options: Options = {}): string[] {
    const includeAll = options.include === "all"
    if (options.cache !== false) {
      const k = key(pattern, options)
      const cached = readGlobCache(DIR, k)
      if (cached != null) return cached
      const results = globScanCompiled(
        pattern,
        options.cwd ?? ".",
        10,
        options.dot ?? false,
        options.symlink ?? false,
        includeAll,
        options.absolute ?? false,
        true,
      )
      deleteCache(DIR, k)
      writeGlobCache(DIR, k, results)
      maybeTrimCoreCache()
      return results
    }
    if (!includeAll && !(options.absolute ?? false)) {
      return globParallel(
        pattern,
        options.cwd ?? ".",
        10,
        options.dot ?? false,
        options.symlink ?? false,
      )
    }
    if (!includeAll) {
      return globScanParallel(
        pattern,
        options.cwd ?? ".",
        10,
        options.dot ?? false,
        options.symlink ?? false,
        false,
        options.absolute ?? false,
      )
    }
    return globScanCompiled(
      pattern,
      options.cwd ?? ".",
      10,
      options.dot ?? false,
      options.symlink ?? false,
      includeAll,
      options.absolute ?? false,
      true,
    )
  }

  export function match(pattern: string, filepath: string): boolean {
    return globMatchCompiled(pattern, filepath)
  }

  export function clear() {
    clearCache(DIR)
    globCompiledCacheClear()
  }
}
