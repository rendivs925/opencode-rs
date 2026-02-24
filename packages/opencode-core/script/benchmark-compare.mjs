import fs from "node:fs/promises"
import path from "node:path"
import os from "node:os"
import { performance } from "node:perf_hooks"
import core from "../npm/index.js"

const root = path.resolve(import.meta.dirname, "..")
const out = path.join(root, "benchmarks", "compare-report.md")

function ms(fn, n = 100) {
  const t0 = performance.now()
  for (let i = 0; i < n; i++) fn()
  const t1 = performance.now()
  return (t1 - t0) / n
}

function jsMatch(pattern, filepath) {
  if (pattern === "*.txt") return filepath.endsWith(".txt") && !filepath.includes("/")
  if (pattern === "**/*.ts") return filepath.endsWith(".ts") && filepath.includes("/")
  if (pattern === "**/*.md") return filepath.endsWith(".md") && filepath.includes("/")
  if (pattern === ".*") return filepath.startsWith(".")
  return false
}

function jsTruncate(text, maxLines, maxBytes, direction) {
  const lines = text.split("\n")
  const selected = direction === "tail" ? lines.slice(-maxLines) : lines.slice(0, maxLines)
  let joined = selected.join("\n")
  while (Buffer.byteLength(joined, "utf-8") > maxBytes && joined.length > 0) {
    joined = direction === "tail" ? joined.slice(1) : joined.slice(0, -1)
  }
  return joined
}

async function setup() {
  const dir = await fs.mkdtemp(path.join(os.tmpdir(), "opencode-core-compare-"))
  const deep = path.join(dir, "src", "nested")
  await fs.mkdir(deep, { recursive: true })
  await Promise.all(
    Array.from({ length: 500 }).map((_, i) => {
      const ext = i % 3 === 0 ? "ts" : i % 3 === 1 ? "md" : "txt"
      return fs.writeFile(path.join(deep, `f${i}.${ext}`), `line ${i}\n`.repeat(20), "utf-8")
    }),
  )
  await fs.writeFile(path.join(dir, "token.txt"), "hello world ".repeat(5000), "utf-8")
  return dir
}

function fmt(v) {
  return Number(v.toFixed(4))
}

async function run() {
  const dir = await setup()
  const tokenFile = path.join(dir, "token.txt")
  const text = "alpha beta gamma delta ".repeat(2400)
  const long = Array.from({ length: 8000 })
    .map((_, i) => `line-${i}`)
    .join("\n")
  const files = Array.from({ length: 1000 }).map((_, i) => (i % 2 === 0 ? `src/f${i}.ts` : `docs/f${i}.md`))

  const rows = [
    {
      op: "Token from text",
      js: ms(() => text.length / 4, 300),
      rust: ms(() => core.countTokensFromText(text, "cl100k_base"), 120),
    },
    {
      op: "Token from file",
      js: ms(() => Buffer.byteLength(text, "utf-8") / 4, 250),
      rust: ms(() => core.countTokens(tokenFile, "cl100k_base"), 80),
    },
    {
      op: "Glob scan",
      js: ms(() => files.filter((item) => item.endsWith(".ts")).length, 300),
      rust: ms(() => core.glob("**/*.ts", dir, 10, true, false), 80),
    },
    {
      op: "Glob match",
      js: ms(() => jsMatch("**/*.ts", "src/main.ts"), 2000),
      rust: ms(() => core.isIgnored("src/main.ts", ["**/*.ts"]), 2000),
    },
    {
      op: "Truncate",
      js: ms(() => jsTruncate(long, 300, 16384, "head"), 120),
      rust: ms(() => core.truncate(long, 300, 16384, "head"), 120),
    },
  ]

  const lines = [
    "# Rust vs JS Benchmark Report",
    "",
    `Date: ${new Date().toISOString().slice(0, 10)}`,
    "",
    "Microbench means in ms/op. `Speedup = JS / Rust`.",
    "",
    "| Operation | JS (ms/op) | Rust (ms/op) | Speedup |",
    "| --- | ---: | ---: | ---: |",
    ...rows.map((row) => {
      const speedup = row.rust > 0 ? row.js / row.rust : 0
      return `| ${row.op} | ${fmt(row.js)} | ${fmt(row.rust)} | ${fmt(speedup)}x |`
    }),
    "",
    "Notes:",
    "- JS values are local baseline implementations in this script.",
    "- Results are for regression tracking on one machine, not absolute claims.",
  ]

  await fs.mkdir(path.dirname(out), { recursive: true })
  await fs.writeFile(out, lines.join("\n") + "\n", "utf-8")
  await fs.rm(dir, { recursive: true, force: true })
  console.log(`wrote ${path.relative(path.resolve(import.meta.dirname, "../../.."), out)}`)
}

run().catch((error) => {
  console.error(error)
  process.exit(1)
})
