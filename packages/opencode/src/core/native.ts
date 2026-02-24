const core = require("../../../opencode-core/npm/index.js")

export const glob = core.glob as (
  pattern: string,
  cwd: string,
  maxDepth?: number,
  includeHidden?: boolean,
  followLinks?: boolean,
) => string[]

export const globParallel = core.globParallel as (
  pattern: string,
  cwd: string,
  maxDepth?: number,
  includeHidden?: boolean,
  followLinks?: boolean,
) => string[]

export const countTokens = core.countTokens as (path: string, encoding: string) => number
export const countTokensFromText = core.countTokensFromText as (text: string, encoding: string) => number
export const countTokensStreaming = core.countTokensStreaming as (
  path: string,
  encoding: string,
  chunkSize: number,
) => number

export const isIgnored = core.isIgnored as (path: string, patterns: string[]) => boolean
export const filterPaths = core.filterPaths as (paths: string[], patterns: string[]) => string[]
export const compilePatterns = core.compilePatterns as (patterns: string[]) => unknown

export const truncate = core.truncate as (
  text: string,
  maxLines?: number,
  maxBytes?: number,
  direction?: "head" | "tail",
) => { text: string; lines: number; bytes: number; truncated: boolean }

export const truncateLines = core.truncateLines as (text: string, maxLines: number) => unknown
export const truncateBytes = core.truncateBytes as (text: string, maxBytes: number) => unknown
export const truncateTail = core.truncateTail as (text: string, maxLines: number) => unknown

export const extractArchive = core.extractArchive as (archivePath: string, outputDir: string) => unknown
export const listArchiveContents = core.listArchiveContents as (
  archivePath: string,
) => { name: string; size: string; isDir: boolean }[]
export const readArchiveEntry = core.readArchiveEntry as (archivePath: string, entryName: string) => Uint8Array
export const extractTarGz = core.extractTarGz as (archivePath: string, outputDir: string) => unknown
export const extractTarBz2 = core.extractTarBz2 as (archivePath: string, outputDir: string) => unknown

export const writeGlobCache = core.writeGlobCache as (cacheDir: string, key: string, value: string[]) => void
export const readGlobCache = core.readGlobCache as (cacheDir: string, key: string) => string[] | undefined
export const writeTokenCache = core.writeTokenCache as (cacheDir: string, key: string, value: number) => void
export const readTokenCache = core.readTokenCache as (cacheDir: string, key: string) => number | undefined
export const clearCache = core.clearCache as (cacheDir: string) => void
export const deleteCache = core.deleteCache as (cacheDir: string, key: string) => void

export const FileWatcher = core.FileWatcher as new () => {
  watch(path: string): void
  unwatch(): void
  nextEvent(): { path: string; kind: string } | null | undefined
}

export const CompiledIgnore = core.CompiledIgnore as new () => unknown
