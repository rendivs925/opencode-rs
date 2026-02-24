import { describe, expect, test } from "bun:test"
import fs from "fs/promises"
import path from "path"
import { tmpdir } from "../fixture/fixture"
import { Ripgrep } from "../../src/file/ripgrep"

describe("file.ripgrep", () => {
  test("defaults to include hidden", async () => {
    await using tmp = await tmpdir({
      init: async (dir) => {
        await Bun.write(path.join(dir, "visible.txt"), "hello")
        await fs.mkdir(path.join(dir, ".opencode"), { recursive: true })
        await Bun.write(path.join(dir, ".opencode", "thing.json"), "{}")
      },
    })

    const files = await Array.fromAsync(Ripgrep.files({ cwd: tmp.path }))
    const hasVisible = files.includes("visible.txt")
    const hasHidden = files.includes(path.join(".opencode", "thing.json"))
    expect(hasVisible).toBe(true)
    expect(hasHidden).toBe(true)
  })

  test("hidden false excludes hidden", async () => {
    await using tmp = await tmpdir({
      init: async (dir) => {
        await Bun.write(path.join(dir, "visible.txt"), "hello")
        await fs.mkdir(path.join(dir, ".opencode"), { recursive: true })
        await Bun.write(path.join(dir, ".opencode", "thing.json"), "{}")
      },
    })

    const files = await Array.fromAsync(Ripgrep.files({ cwd: tmp.path, hidden: false }))
    const hasVisible = files.includes("visible.txt")
    const hasHidden = files.includes(path.join(".opencode", "thing.json"))
    expect(hasVisible).toBe(true)
    expect(hasHidden).toBe(false)
  })

  test("search returns structured matches", async () => {
    await using tmp = await tmpdir({
      init: async (dir) => {
        await Bun.write(path.join(dir, "a.ts"), "export const one = 1\n")
        await Bun.write(path.join(dir, "b.js"), "export const two = 2\n")
      },
    })

    const result = await Ripgrep.search({
      cwd: tmp.path,
      pattern: "export\\s+const",
      glob: ["*.ts"],
      limit: 10,
    })

    expect(result.length).toBe(1)
    expect(result[0]?.path.text.endsWith(path.join(tmp.path, "a.ts"))).toBe(true)
    expect(result[0]?.line_number).toBe(1)
  })

  test("tree returns directories", async () => {
    await using tmp = await tmpdir({
      init: async (dir) => {
        await fs.mkdir(path.join(dir, "src", "lib"), { recursive: true })
        await Bun.write(path.join(dir, "src", "lib", "a.ts"), "const a = 1\n")
      },
    })

    const output = await Ripgrep.tree({ cwd: tmp.path, limit: 10 })
    expect(output).toContain("src")
    expect(output).toContain("src/lib")
  })
})
