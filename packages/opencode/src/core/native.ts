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

export const globScan = core.globScan as (
  pattern: string,
  cwd: string,
  maxDepth?: number,
  includeHidden?: boolean,
  followLinks?: boolean,
  includeAll?: boolean,
  absolute?: boolean,
) => string[]

export const globScanParallel = core.globScanParallel as (
  pattern: string,
  cwd: string,
  maxDepth?: number,
  includeHidden?: boolean,
  followLinks?: boolean,
  includeAll?: boolean,
  absolute?: boolean,
) => string[]

export const globMatch = core.globMatch as (pattern: string, filepath: string) => boolean

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
  nextEvents(limit?: number, ignorePatterns?: string[]): { path: string; kind: "add" | "change" | "unlink" }[]
}

export const CompiledIgnore = core.CompiledIgnore as new () => {
  isIgnored(path: string): boolean
  filter(paths: string[]): string[]
  isIgnoredWith(path: string, extraPatterns?: string[], whitelist?: string[]): boolean
  filterWith(paths: string[], extraPatterns?: string[], whitelist?: string[]): string[]
}

export const containsPath = core.containsPath as (parent: string, child: string) => boolean
export const overlapsPath = core.overlapsPath as (a: string, b: string) => boolean
export const listDirectory = core.listDirectory as (
  dir: string,
  base: string,
  exclude?: string[],
  ignorePatterns?: string[],
) => {
  name: string
  path: string
  absolute: string
  entryType: "file" | "directory"
  ignored: boolean
}[]

export const listDirectoryProject = core.listDirectoryProject as (
  dir: string,
  base: string,
  worktree: string,
  exclude?: string[],
) => {
  name: string
  path: string
  absolute: string
  entryType: "file" | "directory"
  ignored: boolean
}[]

export const searchContent = core.searchContent as (
  pattern: string,
  searchPath: string,
  include?: string,
  maxResults?: number,
  maxLineLength?: number,
) => {
  matches: {
    path: string
    modTime: number
    lineNum: number
    lineText: string
    absoluteOffset: number
    submatches: { text: string; start: number; end: number }[]
  }[]
  hasErrors: boolean
  totalMatches: number
}

export const searchContentRendered = core.searchContentRendered as (
  pattern: string,
  searchPath: string,
  include?: string,
  maxResults?: number,
  maxLineLength?: number,
) => {
  output: string
  hasErrors: boolean
  totalMatches: number
  displayedMatches: number
  truncated: boolean
}

export const listFiles = core.listFiles as (
  searchPath: string,
  globs?: string[],
  includeHidden?: boolean,
  followLinks?: boolean,
  maxDepth?: number,
) => { files: string[]; hasErrors: boolean }

export const listFilesSorted = core.listFilesSorted as (
  searchPath: string,
  globs?: string[],
  includeHidden?: boolean,
  followLinks?: boolean,
  maxDepth?: number,
  limit?: number,
) => { files: { path: string; modTime: number }[]; total: number; hasErrors: boolean }

export const listTree = core.listTree as (
  searchPath: string,
  globs?: string[],
  limit?: number,
) => { output: string; count: number; truncated: boolean }

export const indexPaths = core.indexPaths as (
  searchPath: string,
  includeHidden?: boolean,
  followLinks?: boolean,
  maxDepth?: number,
) => { files: string[]; dirs: string[]; hasErrors: boolean }

export const indexPathsCached = core.indexPathsCached as (
  searchPath: string,
  includeHidden?: boolean,
  followLinks?: boolean,
  maxDepth?: number,
  refresh?: boolean,
) => { files: string[]; dirs: string[]; hasErrors: boolean }

export const renderTree = core.renderTree as (
  searchPath: string,
  limit?: number,
  includeHidden?: boolean,
  followLinks?: boolean,
  maxDepth?: number,
) => string

export const searchPaths = core.searchPaths as (
  searchPath: string,
  query: string,
  kind: "file" | "directory" | "all",
  limit?: number,
  includeHidden?: boolean,
  followLinks?: boolean,
  maxDepth?: number,
) => string[]

export const searchContentAdvanced = core.searchContentAdvanced as (
  pattern: string,
  searchPath: string,
  globs?: string[],
  includeHidden?: boolean,
  followLinks?: boolean,
  maxDepth?: number,
  maxResults?: number,
  maxLineLength?: number,
) => {
  matches: {
    path: string
    modTime: number
    lineNum: number
    lineText: string
    absoluteOffset: number
    submatches: { text: string; start: number; end: number }[]
  }[]
  hasErrors: boolean
  totalMatches: number
}

export const readFileWindow = core.readFileWindow as (
  path: string,
  offset?: number,
  limit?: number,
  maxBytes?: number,
  maxLineLength?: number,
) => {
  lines: string[]
  totalLines: number
  truncated: boolean
  truncatedByBytes: boolean
  nextOffset: number
}

export const classifyReadTarget = core.classifyReadTarget as (
  path: string,
  hintPath?: string,
) => {
  mode: "text" | "binary" | "base64"
  exists: boolean
  mimeType?: string
}

export const readFull = core.readFull as (
  path: string,
  hintPath?: string,
) => {
  kind: "text" | "binary"
  exists: boolean
  content: string
  mimeType?: string
  encoding?: "base64"
}

export const countUntrackedLines = core.countUntrackedLines as (
  root: string,
  files: string[],
) => { path: string; lines: number }[]

export const readAttachment = core.readAttachment as (path: string) => {
  isAttachment: boolean
  mimeType?: string
  base64?: string
}

export const readDirWindow = core.readDirWindow as (
  path: string,
  offset?: number,
  limit?: number,
) => {
  entries: string[]
  totalEntries: number
  truncated: boolean
  nextOffset: number
}
