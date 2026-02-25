import { BusEvent } from "@/bus/bus-event"
import z from "zod"
import path from "path"
import { Log } from "../util/log"
import { Instance } from "../project/instance"
import { Global } from "../global"
import {
  buildDiffPatch,
  caseFoldAscii,
  countUntrackedLines,
  gitStatus,
  indexGlobalHomeDirs,
  indexPathsCached,
  listDirectory,
  listDirectoryProject,
  readDiffSnapshot,
  readFull,
  searchIndexedPaths,
  searchPaths,
} from "@/core/native"

export namespace File {
  const log = Log.create({ service: "file" })

  export const Info = z
    .object({
      path: z.string(),
      added: z.number().int(),
      removed: z.number().int(),
      status: z.enum(["added", "deleted", "modified"]),
    })
    .meta({
      ref: "File",
    })

  export type Info = z.infer<typeof Info>

  export const Node = z
    .object({
      name: z.string(),
      path: z.string(),
      absolute: z.string(),
      type: z.enum(["file", "directory"]),
      ignored: z.boolean(),
    })
    .meta({
      ref: "FileNode",
    })
  export type Node = z.infer<typeof Node>

  export const Content = z
    .object({
      type: z.enum(["text", "binary"]),
      content: z.string(),
      diff: z.string().optional(),
      patch: z
        .object({
          oldFileName: z.string(),
          newFileName: z.string(),
          oldHeader: z.string().optional(),
          newHeader: z.string().optional(),
          hunks: z.array(
            z.object({
              oldStart: z.number(),
              oldLines: z.number(),
              newStart: z.number(),
              newLines: z.number(),
              lines: z.array(z.string()),
            }),
          ),
          index: z.string().optional(),
        })
        .optional(),
      encoding: z.literal("base64").optional(),
      mimeType: z.string().optional(),
    })
    .meta({
      ref: "FileContent",
    })
  export type Content = z.infer<typeof Content>

  export const Event = {
    Edited: BusEvent.define(
      "file.edited",
      z.object({
        file: z.string(),
      }),
    ),
  }

  const state = Instance.state(async () => {
    type Entry = { files: string[]; dirs: string[] }
    let cache: Entry = { files: [], dirs: [] }
    let fetching = false

    const isGlobalHome = Instance.directory === Global.Path.home && Instance.project.id === "global"

    const fn = async (result: Entry) => {
      // Disable scanning if in root of file system
      if (Instance.directory === path.parse(Instance.directory).root) return
      fetching = true

      if (isGlobalHome) {
        const indexed = indexGlobalHomeDirs(Instance.directory, process.platform)
        result.files = indexed.files
        result.dirs = indexed.dirs
        cache = result
        fetching = false
        return
      }

      const indexed = indexPathsCached(Instance.directory, true, false, undefined, false)
      result.files.push(...indexed.files)
      result.dirs.push(...indexed.dirs)
      cache = result
      fetching = false
    }
    fn(cache)

    return {
      async files() {
        if (!fetching) {
          fn({
            files: [],
            dirs: [],
          })
        }
        return cache
      },
    }
  })

  export function init() {
    state()
  }

  export async function status() {
    const project = Instance.project
    if (project.vcs !== "git") return []
    const status = gitStatus(Instance.directory)
    const added = status.filter((item) => item.status === "added").map((item) => item.path)
    const counts = new Map(countUntrackedLines(Instance.directory, added).map((item) => [item.path, item.lines]))
    return status.map((x) => {
      const full = path.isAbsolute(x.path) ? x.path : path.join(Instance.directory, x.path)
      return {
        ...x,
        added: x.status === "added" ? counts.get(x.path) ?? x.added : x.added,
        path: path.relative(Instance.directory, full),
      }
    })
  }

  export async function read(file: string): Promise<Content> {
    using _ = log.time("read", { file })
    const project = Instance.project
    const full = path.join(Instance.directory, file)

    // TODO: Filesystem.contains is lexical only - symlinks inside the project can escape.
    // TODO: On Windows, cross-drive paths bypass this check. Consider realpath canonicalization.
    if (!Instance.containsPath(full)) {
      throw new Error(`Access denied: path escapes project directory`)
    }

    const native = readFull(full, file)
    if (!native.exists) return { type: "text", content: "" }
    if (native.kind === "binary") return { type: "binary", content: "", mimeType: native.mimeType }
    const content = native.content
    if (native.encoding === "base64") {
      return { type: "text", content, mimeType: native.mimeType, encoding: "base64" }
    }

    if (project.vcs === "git") {
      const snapshot = readDiffSnapshot(Instance.directory, file)
      if (snapshot.hasDiff) {
        const built = buildDiffPatch(file, snapshot.original, content)
        return { type: "text", content, patch: built.patch, diff: built.diff }
      }
    }
    return { type: "text", content }
  }

  export async function list(dir?: string) {
    const exclude = [".git", ".DS_Store"]
    const project = Instance.project
    const resolved = dir ? path.join(Instance.directory, dir) : Instance.directory

    // TODO: Filesystem.contains is lexical only - symlinks inside the project can escape.
    // TODO: On Windows, cross-drive paths bypass this check. Consider realpath canonicalization.
    if (!Instance.containsPath(resolved)) {
      throw new Error(`Access denied: path escapes project directory`)
    }

    const entries =
      project.vcs === "git"
        ? listDirectoryProject(resolved, Instance.directory, Instance.worktree, exclude)
        : listDirectory(resolved, Instance.directory, exclude)
    return entries.map((entry) => {
      return {
        name: entry.name,
        path: entry.path,
        absolute: entry.absolute,
        type: entry.entryType,
        ignored: entry.ignored,
      }
    })
  }

  export async function search(input: { query: string; limit?: number; dirs?: boolean; type?: "file" | "directory" }) {
    const query = input.query.trim()
    const ascii = /^[\x00-\x7F]*$/.test(query)
    const limit = input.limit ?? 100
    const kind = input.type ?? (input.dirs === false ? "file" : "all")
    log.info("search", { query, kind })
    const isGlobalHome = Instance.directory === Global.Path.home && Instance.project.id === "global"
    if (!isGlobalHome) {
      let output = searchPaths(Instance.directory, query, kind, limit, true, false)
      if (!output.length && ascii) {
        output = searchPaths(Instance.directory, caseFoldAscii(query), kind, limit, true, false)
      }
      log.info("search", { query, kind, results: output.length })
      return output
    }

    const result = await state().then((x) => x.files())
    let output = searchIndexedPaths(result, query, kind, limit)
    if (!output.length && ascii) {
      output = searchIndexedPaths(result, caseFoldAscii(query), kind, limit)
    }

    log.info("search", { query, kind, results: output.length })
    return output
  }
}
