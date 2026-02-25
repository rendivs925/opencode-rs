import fs from "node:fs/promises"
import path from "node:path"

const root = path.resolve(import.meta.dirname, "..")
const file = path.join(root, "benchmarks", "report.md")

const threshold = {
  countTokensFromText: 500.0,
  "countTokens(file)": 500.0,
  glob: 10.0,
  globParallel: 20.0,
  isIgnored: 1.0,
  truncate: 5.0,
}

function parseRows(text) {
  return text
    .split("\n")
    .filter((item) => item.startsWith("| ") && item.endsWith(" |"))
    .slice(2)
    .map((item) => item.split("|").map((col) => col.trim()))
    .filter((cols) => cols.length >= 3)
    .map((cols) => {
      const op = cols[1]
      const mean = Number(cols[2])
      return { op, mean }
    })
    .filter((item) => Number.isFinite(item.mean))
}

async function run() {
  const text = await fs.readFile(file, "utf-8")
  const rows = parseRows(text)
  const fails = Object.entries(threshold)
    .map(([op, min]) => {
      const row = rows.find((item) => item.op === op)
      if (!row) return { op, max: min, mean: NaN, reason: "missing benchmark row" }
      if (row.mean <= min) return null
      return { op, max: min, mean: row.mean, reason: "above budget" }
    })
    .filter(Boolean)

  if (!fails.length) {
    console.log("benchmark guard passed")
    return
  }

  console.error("benchmark guard failed")
  for (const item of fails) {
    console.error(`- ${item.op}: ${item.reason} (got ${item.mean}ms, need <= ${item.max}ms)`)
  }
  process.exit(1)
}

run().catch((err) => {
  console.error(err)
  process.exit(1)
})
