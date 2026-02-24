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

  export type NativeFileEvent = {
    path: string
    kind: string
  }

  export class FileWatcher {
    watch(path: string): void
    unwatch(): void
    nextEvent(): NativeFileEvent | null | undefined
  }

  export class CompiledIgnore {
    isIgnored(path: string): boolean
    filter(paths: string[]): string[]
  }
}
