import { describe, expect, test } from "bun:test"
import { $ } from "bun"
import fs from "fs/promises"
import path from "path"
import { tmpdir } from "../fixture/fixture"
import {
  buildDiffPatch,
  gitStatus,
  globScan,
  indexGlobalHomeDirs,
  listDirectoryProject,
  listTree,
  readDiffSnapshot,
  readFull,
  searchIndexedPaths,
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

  test("indexed search ranks and keeps hidden dirs behind visible entries", () => {
    const result = searchIndexedPaths(
      {
        files: ["src/main.ts", ".cache/x.ts", "docs/guide.md"],
        dirs: ["src/", ".cache/", "docs/"],
      },
      "",
      "directory",
      10,
    )
    expect(result[0]).toBe("docs/")
    expect(result[1]).toBe("src/")
    expect(result[result.length - 1]).toBe(".cache/")
  })

  test("git status and diff snapshot return structured data", async () => {
    await using tmp = await tmpdir({
      git: true,
      init: async (dir) => {
        await Bun.write(path.join(dir, "tracked.txt"), "a\n")
      },
    })
    await $`git add . && git commit -m init`.cwd(tmp.path).quiet()
    await Bun.write(path.join(tmp.path, "tracked.txt"), "b\n")
    await Bun.write(path.join(tmp.path, "new.txt"), "x\ny\n")

    const status = gitStatus(tmp.path)
    expect(status.some((item) => item.path === "tracked.txt" && item.status === "modified")).toBe(true)
    expect(status.some((item) => item.path === "new.txt" && item.status === "added")).toBe(true)

    const snapshot = readDiffSnapshot(tmp.path, "tracked.txt")
    expect(snapshot.hasDiff).toBe(true)
    expect(snapshot.original).toContain("a")

    const built = buildDiffPatch("tracked.txt", snapshot.original, "b\n")
    expect(built.diff).toContain("--- tracked.txt")
    expect(built.patch.hunks.length).toBe(1)
  })

  test("global home dir index returns shallow dirs", async () => {
    await using tmp = await tmpdir({
      init: async (dir) => {
        await fs.mkdir(path.join(dir, "src", "nested"), { recursive: true })
        await Bun.write(path.join(dir, "src", ".keep"), "")
        await Bun.write(path.join(dir, "src", "nested", "x.txt"), "x")
      },
    })
    const indexed = indexGlobalHomeDirs(tmp.path, process.platform)
    expect(indexed.dirs.some((item) => item === "src/")).toBe(true)
    expect(indexed.dirs.some((item) => item === "src/nested/")).toBe(true)
  })
})
