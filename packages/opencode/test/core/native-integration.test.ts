import { describe, expect, test } from "bun:test"
import path from "path"
import { tmpdir } from "../fixture/fixture"
import {
  globScan,
  listDirectoryProject,
  listTree,
  readFull,
  searchContentRendered,
} from "../../src/core/native"

describe("core/native integration contracts", () => {
  test("readFull returns text and base64 contracts", async () => {
    await using tmp = await tmpdir({
      init: async (dir) => {
        await Bun.write(path.join(dir, "a.txt"), " hello \n")
        await Bun.write(path.join(dir, "a.png"), Buffer.from([0x89, 0x50, 0x4e, 0x47]))
      },
    })

    const text = readFull(path.join(tmp.path, "a.txt"), "a.txt")
    expect(text.kind).toBe("text")
    expect(text.content).toBe("hello")

    const image = readFull(path.join(tmp.path, "a.png"), "a.png")
    expect(image.kind).toBe("text")
    expect(image.encoding).toBe("base64")
    expect(image.mimeType).toBe("image/png")
  })

  test("glob/search/tree/list native outputs are usable", async () => {
    await using tmp = await tmpdir({
      git: true,
      init: async (dir) => {
        await Bun.write(path.join(dir, ".gitignore"), "dist\n")
        await Bun.write(path.join(dir, "src", "one.ts"), "export const one = 1\n")
        await Bun.write(path.join(dir, "dist", "out.js"), "console.log(1)\n")
      },
    })

    const files = globScan("**/*.ts", tmp.path, 10, false, false, false, false)
    expect(files).toContain("src/one.ts")

    const grep = searchContentRendered("export", tmp.path, "**/*.ts", 100, 2000)
    expect(grep.totalMatches).toBeGreaterThan(0)
    expect(grep.output).toContain("Found")

    const tree = listTree(tmp.path, undefined, 100)
    expect(tree.output).toContain("src/")

    const nodes = listDirectoryProject(tmp.path, tmp.path, tmp.path, [".git", ".DS_Store"])
    expect(nodes.find((item) => item.name === "dist")?.ignored).toBe(true)
  })
})
