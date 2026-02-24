import { describe, expect, test } from "bun:test"
import fs from "fs/promises"
import path from "path"
import { tmpdir } from "../fixture/fixture"
import { Token } from "../../src/util/token"

describe("Token", () => {
  test("count() is stable for repeated text", () => {
    const text = "hello world ".repeat(20)
    const a = Token.count(text)
    const b = Token.count(text)
    expect(b).toBe(a)
  })

  test("countFile() updates when file changes", async () => {
    await using tmp = await tmpdir()
    const file = path.join(tmp.path, "a.txt")
    await fs.writeFile(file, "alpha beta gamma", "utf-8")
    const a = Token.countFile(file)
    await Bun.sleep(5)
    await fs.writeFile(file, "alpha beta gamma delta epsilon zeta eta theta", "utf-8")
    const b = Token.countFile(file)
    expect(b).toBeGreaterThan(a)
  })

  test("estimate() returns zero for empty input", () => {
    expect(Token.estimate("")).toBe(0)
  })
})
