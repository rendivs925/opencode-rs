# @opencode-ai/core

High-performance core utilities for OpenCode written in Rust using NAPI-RS.

## Features

- **Glob** - Fast file discovery using walkdir + rayon
- **Token Counting** - Tiktoken-based tokenization with memory-mapped I/O
- **Ignore Patterns** - Gitignore-compatible pattern matching
- **Truncation** - Fast byte-level text truncation
- **Archive Extraction** - Zip and tar.gz extraction
- **File Watching** - Cross-platform file system watching
- **Caching** - Bincode + Gzip serialization for fast caching

## Installation

```bash
npm install @opencode-ai/core
```

## Usage

```typescript
import { glob, countTokens, truncate } from "@opencode-ai/core"

// Glob file discovery
const files = await glob("**/*.ts", { cwd: "./src" })

// Token counting
const tokens = await countTokens("./large-file.txt", "cl100k_base")

// Text truncation
const result = truncate("long text...", { maxLines: 10 })
```

## API

### Glob

```typescript
glob(pattern: string, cwd: string, maxDepth?: number, includeHidden?: boolean): string[]
globParallel(pattern: string, cwd: string, maxDepth?: number, includeHidden?: boolean): string[]
```

### Token Counting

```typescript
countTokens(path: string, encoding: string): number
countTokensFromText(text: string, encoding: string): number
countTokensStreaming(path: string, encoding: string, chunkSize: number): number
```

Supported encodings: `cl100k_base`, `p50k_base`, `p50k_edit`, `r50k_base`

### Ignore Patterns

```typescript
isIgnored(path: string, patterns: string[]): boolean
filterPaths(paths: string[], patterns: string[]): string[]
compilePatterns(patterns: string[]): CompiledIgnore
```

### Truncation

```typescript
truncate(text: string, maxLines?: number, maxBytes?: number, direction?: "head" | "tail"): TruncationResult
truncateLines(text: string, maxLines: number): TruncationResult
truncateBytes(text: string, maxBytes: number): TruncationResult
truncateTail(text: string, maxLines: number): TruncationResult
```

### Archive

```typescript
extractArchive(archivePath: string, outputDir: string): ArchiveEntry[]
listArchiveContents(archivePath: string): ArchiveEntry[]
readArchiveEntry(archivePath: string, entryName: string): Buffer
extractTarGz(archivePath: string, outputDir: string): ArchiveEntry[]
extractTarBz2(archivePath: string, outputDir: string): ArchiveEntry[]
```

### Cache

```typescript
writeGlobCache(cacheDir: string, key: string, value: string[]): void
readGlobCache(cacheDir: string, key: string): string[] | null
writeTokenCache(cacheDir: string, key: string, value: number): void
readTokenCache(cacheDir: string, key: string): number | null
clearCache(cacheDir: string): void
deleteCache(cacheDir: string, key: string): void
```

### File Watcher

```typescript
new FileWatcher(): FileWatcher
watcher.watch(path: string): void
watcher.unwatch(): void
```

## Building

```bash
# Install Rust
cargo build --release

# Or use napi
npx @napi-rs/cli build
```

## Performance

| Feature            | Speedup |
| ------------------ | ------- |
| Glob               | 5-20x   |
| Token Counting     | 10-50x  |
| Ignore Patterns    | 10-100x |
| Truncation         | 5-10x   |
| Archive Extraction | 3-10x   |

## License

MIT
