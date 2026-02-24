import fs from "node:fs/promises"
import path from "node:path"
import os from "node:os"
import { performance } from "node:perf_hooks"
import core from "../npm/index.js"

const root = path.resolve(import.meta.dirname, "..")
const out = path.join(root, "benchmarks", "report.md")

function ms(fn, n = 200) {
  const t0 = performance.now()
  for (let i = 0; i < n; i++) fn()
  const t1 = performance.now()
  return Number(((t1 - t0) / n).toFixed(4))
}

async function setup() {
  const dir = await fs.mkdtemp(path.join(os.tmpdir(), "opencode-core-bench-"))
  const deep = path.join(dir, "src", "nested")
  await fs.mkdir(deep, { recursive: true })
  await Promise.all(
    Array.from({ length: 300 }).map((_, i) => {
      const ext = i % 3 === 0 ? "ts" : i % 3 === 1 ? "md" : "json"
      return fs.writeFile(path.join(deep, `f${i}.${ext}`), `line ${i}\n`.repeat(20), "utf-8")
    }),
  )
  await fs.writeFile(path.join(dir, "token.txt"), "hello world ".repeat(4000), "utf-8")
  return dir
}

async function run() {
  const dir = await setup()
  const tokenFile = path.join(dir, "token.txt")
  const text = "alpha beta gamma delta ".repeat(2000)
  const long = Array.from({ length: 5000 })
    .map((_, i) => `line-${i}`)
    .join("\n")

  const rows = [
    ["countTokensFromText", ms(() => core.countTokensFromText(text, "cl100k_base"), 100)],
    ["countTokens(file)", ms(() => core.countTokens(tokenFile, "cl100k_base"), 50)],
    ["glob", ms(() => core.glob("**/*.ts", dir, 10, true, false), 50)],
    ["globParallel", ms(() => core.globParallel("**/*.ts", dir, 10, true, false), 50)],
    ["isIgnored", ms(() => core.isIgnored("src/main.ts", ["**/*.ts", "node_modules/**"]), 500)],
    ["truncate", ms(() => core.truncate(long, 300, 16384, "head"), 100)],
  ]

  const date = new Date().toISOString().slice(0, 10)
  const lines = [
    "# Rust Benchmark Report",
    "",
    `Date: ${date}`,
    "",
    "These are native `@opencode-ai/core` microbench means in ms/op.",
    "",
    "| Operation | Mean (ms/op) |",
    "| --- | ---: |",
    ...rows.map(([name, val]) => `| ${name} | ${val} |`),
    "",
    "Notes:",
    "- This is a local microbench run (single machine, warm process).",
    "- Use it for regression tracking, not absolute cross-machine comparisons.",
  ]
  await fs.mkdir(path.dirname(out), { recursive: true })
  await fs.writeFile(out, lines.join("\n") + "\n", "utf-8")
  await fs.rm(dir, { recursive: true, force: true })
  console.log(`wrote ${path.relative(path.resolve(import.meta.dirname, "../../.."), out)}`)
}

run().catch((e) => {
  console.error(e)
  process.exit(1)
})
