import fs from "node:fs/promises"
import path from "node:path"

const root = path.resolve(import.meta.dirname, "..")
const profile = process.argv.includes("--release") ? "release" : "debug"
const ext = process.platform === "darwin" ? "dylib" : process.platform === "win32" ? "dll" : "so"
const built = path.join(root, "target", profile, `libopencode_core.${ext}`)

const targetDir = path.join(root, "prebuilds", `${process.platform}-${process.arch}`)
const target = path.join(targetDir, "opencode-core.node")

await fs.mkdir(targetDir, { recursive: true })
await fs.copyFile(built, target)
console.log(`synced ${path.relative(root, built)} -> ${path.relative(root, target)}`)
