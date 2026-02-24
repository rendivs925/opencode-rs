import { countTokens, countTokensFromText, readTokenCache, writeTokenCache } from "@/core/native"
import path from "path"
import { statSync } from "fs"
import os from "os"
import { xdgCache } from "xdg-basedir"

export namespace Token {
  const DIR = path.join(xdgCache || process.env.XDG_CACHE_HOME || path.join(os.homedir(), ".cache"), "opencode", "rust-token")

  function keyText(input: string, encoding: string) {
    return `text:${encoding}:${input.length}:${Bun.hash.xxHash32(input)}`
  }

  function keyFile(filepath: string, encoding: string) {
    const stat = statSync(filepath)
    return `file:${encoding}:${path.resolve(filepath)}:${stat.size}:${stat.mtimeMs}`
  }

  export function estimate(input: string) {
    const text = input || ""
    if (!text) return 0
    const key = keyText(text, "cl100k_base")
    const cached = readTokenCache(DIR, key)
    if (cached != null) return cached
    const result = countTokensFromText(text, "cl100k_base")
    writeTokenCache(DIR, key, result)
    return result
  }

  export function count(input: string, encoding: string = "cl100k_base") {
    const key = keyText(input, encoding)
    const cached = readTokenCache(DIR, key)
    if (cached != null) return cached
    const result = countTokensFromText(input, encoding)
    writeTokenCache(DIR, key, result)
    return result
  }

  export function countFile(filepath: string, encoding: string = "cl100k_base") {
    const key = keyFile(filepath, encoding)
    const cached = readTokenCache(DIR, key)
    if (cached != null) return cached
    const result = countTokens(filepath, encoding)
    writeTokenCache(DIR, key, result)
    return result
  }
}
