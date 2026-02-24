import { describe, expect, test } from "bun:test"
import path from "path"
import { tmpdir } from "../fixture/fixture"
import { FileWatcher } from "../../src/core/native"

describe("native watcher coalescing", () => {
  test("coalesces burst updates for the same path", async () => {
    await using tmp = await tmpdir()
    const watcher = new FileWatcher()
    watcher.watch(tmp.path)

    const target = path.join(tmp.path, "burst.txt")
    await Bun.write(target, "a")
    await Bun.write(target, "b")
    await Bun.write(target, "c")

    await new Promise((resolve) => setTimeout(resolve, 800))
    const events = watcher.nextEvents(128, [])
    const forTarget = events.filter((item) => item.path === target)
    expect(forTarget.length).toBeLessThanOrEqual(1)
    watcher.unwatch()
  })
})
