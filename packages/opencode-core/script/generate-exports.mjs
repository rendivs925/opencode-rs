import fs from "node:fs/promises"
import path from "node:path"

const root = path.resolve(import.meta.dirname, "..")
const nativeTs = path.resolve(root, "../opencode/src/core/native.ts")
const outPath = path.join(root, "npm", "index.js")
const check = process.argv.includes("--check")

const source = await fs.readFile(nativeTs, "utf8")
const names = [...source.matchAll(/export const (\w+) = core\.(\w+)\s+as/g)]
  .map((m) => ({ local: m[1], native: m[2] }))
  .filter((item) => item.local === item.native)
  .map((item) => item.local)

const unique = [...new Set(names)]

const content = [
  "let native",
  "try {",
  "  native = require(\"../prebuilds/\" + process.platform + \"-\" + process.arch + \"/opencode-core.node\")",
  "} catch (e) {",
  "  const req = eval(\"require\")",
  "  native = req(\"@opencode-ai/core-\" + process.platform + \"-\" + process.arch)",
  "}",
  "",
  "module.exports = {",
  ...unique.map((name) => `  ${name}: native.${name},`),
  "}",
  "",
].join("\n")

if (check) {
  const current = await fs.readFile(outPath, "utf8")
  if (current !== content) {
    throw new Error("npm/index.js is out of sync. Run: bun run exports:sync")
  }
  console.log(`checked exports (${unique.length})`)
  process.exit(0)
}

await fs.writeFile(outPath, content, "utf8")
console.log(`wrote exports (${unique.length})`)
