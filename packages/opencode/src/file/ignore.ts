import { compilePatterns } from "@/core/native"
import { sep } from "node:path"

export namespace FileIgnore {
  const FOLDERS = new Set([
    "node_modules",
    "bower_components",
    ".pnpm-store",
    "vendor",
    ".npm",
    "dist",
    "build",
    "out",
    ".next",
    "target",
    "bin",
    "obj",
    ".git",
    ".svn",
    ".hg",
    ".vscode",
    ".idea",
    ".turbo",
    ".output",
    "desktop",
    ".sst",
    ".cache",
    ".webkit-cache",
    "__pycache__",
    ".pytest_cache",
    "mypy_cache",
    ".history",
    ".gradle",
  ])

  const FILES = [
    "**/*.swp",
    "**/*.swo",

    "**/*.pyc",

    // OS
    "**/.DS_Store",
    "**/Thumbs.db",

    // Logs & temp
    "**/logs/**",
    "**/tmp/**",
    "**/temp/**",
    "**/*.log",

    // Coverage/test outputs
    "**/coverage/**",
    "**/.nyc_output/**",
  ]

  const FOLDER_PATTERNS = [...FOLDERS].flatMap((item) => [`${item}`, `${item}/**`, `**/${item}`, `**/${item}/**`])
  export const PATTERNS = [...FILES, ...FOLDER_PATTERNS]
  const COMPILED = compilePatterns(PATTERNS) as {
    isIgnored(path: string): boolean
    isIgnoredWith(path: string, extraPatterns?: string[], whitelist?: string[]): boolean
  }

  export function match(
    filepath: string,
    opts?: {
      extra?: string[]
      whitelist?: string[]
    },
  ) {
    const extra = opts?.extra || []
    const normalized = filepath.split(sep).join("/")
    const whitelist = opts?.whitelist || []
    if (!extra.length && !whitelist.length) return COMPILED.isIgnored(normalized)
    return COMPILED.isIgnoredWith(normalized, extra, whitelist)
  }
}
