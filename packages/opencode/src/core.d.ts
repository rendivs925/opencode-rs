declare module "@opencode-ai/core" {
  export function glob(
    pattern: string,
    cwd: string,
    maxDepth?: number,
    includeHidden?: boolean,
    followLinks?: boolean,
  ): string[]
  export function globParallel(
    pattern: string,
    cwd: string,
    maxDepth?: number,
    includeHidden?: boolean,
    followLinks?: boolean,
  ): string[]
  export function globScan(
    pattern: string,
    cwd: string,
    maxDepth?: number,
    includeHidden?: boolean,
    followLinks?: boolean,
    includeAll?: boolean,
    absolute?: boolean,
  ): string[]
  export function globScanParallel(
    pattern: string,
    cwd: string,
    maxDepth?: number,
    includeHidden?: boolean,
    followLinks?: boolean,
    includeAll?: boolean,
    absolute?: boolean,
  ): string[]
  export function globMatch(pattern: string, filepath: string): boolean

  export function countTokens(path: string, encoding: string): number
  export function countTokensFromText(text: string, encoding: string): number
  export function countTokensStreaming(path: string, encoding: string, chunkSize: number): number

  export function isIgnored(path: string, patterns: string[]): boolean
  export function filterPaths(paths: string[], patterns: string[]): string[]
  export function compilePatterns(patterns: string[]): CompiledIgnore

  export type TruncationResult = {
    text: string
    lines: number
    bytes: number
    truncated: boolean
  }

  export function truncate(
    text: string,
    maxLines?: number,
    maxBytes?: number,
    direction?: "head" | "tail",
  ): TruncationResult
  export function truncateLines(text: string, maxLines: number): TruncationResult
  export function truncateBytes(text: string, maxBytes: number): TruncationResult
  export function truncateTail(text: string, maxLines: number): TruncationResult

  export type ArchiveEntry = {
    name: string
    size: string
    isDir: boolean
  }

  export function extractArchive(archivePath: string, outputDir: string): ArchiveEntry[]
  export function listArchiveContents(archivePath: string): ArchiveEntry[]
  export function readArchiveEntry(archivePath: string, entryName: string): Uint8Array
  export function extractTarGz(archivePath: string, outputDir: string): ArchiveEntry[]
  export function extractTarBz2(archivePath: string, outputDir: string): ArchiveEntry[]

  export function writeGlobCache(cacheDir: string, key: string, value: string[]): void
  export function readGlobCache(cacheDir: string, key: string): string[] | undefined
  export function writeTokenCache(cacheDir: string, key: string, value: number): void
  export function readTokenCache(cacheDir: string, key: string): number | undefined
  export function clearCache(cacheDir: string): void
  export function deleteCache(cacheDir: string, key: string): void

  export function listDirectoryProject(
    dir: string,
    base: string,
    worktree: string,
    exclude?: string[],
  ): Array<{
    name: string
    path: string
    absolute: string
    entryType: "file" | "directory"
    ignored: boolean
  }>

  export function readFull(
    path: string,
    hintPath?: string,
  ): {
    kind: "text" | "binary"
    exists: boolean
    content: string
    mimeType?: string
    encoding?: "base64"
  }

  export function countUntrackedLines(root: string, files: string[]): Array<{ path: string; lines: number }>
  export function gitStatus(
    root: string,
  ): Array<{ path: string; added: number; removed: number; status: "added" | "deleted" | "modified" }>
  export function readDiffSnapshot(root: string, file: string): { hasDiff: boolean; original: string }
  export function searchIndexedPaths(
    indexed: { files: string[]; dirs: string[] },
    query: string,
    kind: "file" | "directory" | "all",
    limit?: number,
  ): string[]
  export function indexGlobalHomeDirs(searchPath: string, platform?: string): { files: string[]; dirs: string[] }
  export function buildDiffPatch(
    file: string,
    original: string,
    content: string,
  ): {
    diff: string
    patch: {
      oldFileName: string
      newFileName: string
      oldHeader?: string
      newHeader?: string
      hunks: Array<{
        oldStart: number
        oldLines: number
        newStart: number
        newLines: number
        lines: string[]
      }>
      index?: string
    }
  }
  export function resolveGitDir(root: string): string | undefined | null

  export type NativeFileEvent = {
    path: string
    kind: string
  }

  export class FileWatcher {
    watch(path: string): void
    unwatch(): void
    nextEvents(limit?: number, ignorePatterns?: string[]): NativeFileEvent[]
  }

  export class CompiledIgnore {
    isIgnored(path: string): boolean
    filter(paths: string[]): string[]
    isIgnoredWith(path: string, extraPatterns?: string[], whitelist?: string[]): boolean
    filterWith(paths: string[], extraPatterns?: string[], whitelist?: string[]): string[]
  }
}
