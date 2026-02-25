import path from "path"
import fs from "fs/promises"
import { Log } from "../util/log"
import { Flag } from "../flag/flag"
import { Global } from "../global"
import z from "zod"
import { Config } from "../config/config"
import { Instance } from "../project/instance"
import { Scheduler } from "../scheduler"
import { gitExec, gitExecEnv, snapshotDiffFull } from "@/core/native"
import { Bus } from "@/bus"
import { File } from "@/file"
import { FileWatcher } from "@/file/watcher"

export namespace Snapshot {
  const log = Log.create({ service: "snapshot" })
  const hour = 60 * 60 * 1000
  const prune = "7.days"
  const tracker = Instance.state(
    () => {
      const result = {
        dirty: true,
        unsubs: [] as (() => void)[],
      }
      result.unsubs.push(
        Bus.subscribe(File.Event.Edited, () => {
          result.dirty = true
        }),
      )
      result.unsubs.push(
        Bus.subscribe(FileWatcher.Event.Updated, () => {
          result.dirty = true
        }),
      )
      return result
    },
    async (entry) => {
      for (const unsub of entry.unsubs) {
        unsub()
      }
    },
  )

  export function init() {
    Scheduler.register({
      id: "snapshot.cleanup",
      interval: hour,
      run: cleanup,
      scope: "instance",
    })
  }

  export async function cleanup() {
    if (Instance.project.vcs !== "git" || Flag.OPENCODE_CLIENT === "acp") return
    const cfg = await Config.get()
    if (cfg.snapshot === false) return
    const git = gitdir()
    const exists = await fs
      .stat(git)
      .then(() => true)
      .catch(() => false)
    if (!exists) return
    const result = runGit(["gc", `--prune=${prune}`], { gitDir: git, workTree: Instance.worktree, cwd: Instance.directory })
    if (result.exitCode !== 0) {
      log.warn("cleanup failed", {
        exitCode: result.exitCode,
        stderr: result.stderr,
        stdout: result.stdout,
      })
      return
    }
    log.info("cleanup", { prune })
  }

  export async function track() {
    if (Instance.project.vcs !== "git" || Flag.OPENCODE_CLIENT === "acp") return
    const cfg = await Config.get()
    if (cfg.snapshot === false) return
    const git = gitdir()
    const exists = await fs.stat(git).then(() => true).catch(() => false)
    await fs.mkdir(git, { recursive: true })
    if (!exists) {
      runGit(["init"], { gitDir: git, workTree: Instance.worktree, cwd: Instance.directory })
      // Configure git to not convert line endings on Windows
      runGit(["config", "core.autocrlf", "false"], { gitDir: git, workTree: Instance.worktree, cwd: Instance.directory })
      tracker().dirty = true
      log.info("initialized")
    }
    await add(git)
    const hash = runGit(["write-tree"], { gitDir: git, workTree: Instance.worktree, cwd: Instance.directory }).stdout
    log.info("tracking", { hash, cwd: Instance.directory, git })
    return hash.trim()
  }

  export const Patch = z.object({
    hash: z.string(),
    files: z.string().array(),
  })
  export type Patch = z.infer<typeof Patch>

  export async function patch(hash: string): Promise<Patch> {
    const git = gitdir()
    await add(git)
    const result = runGit(
      ["-c", "core.autocrlf=false", "-c", "core.quotepath=false", "diff", "--no-ext-diff", "--name-only", hash, "--", "."],
      { gitDir: git, workTree: Instance.worktree, cwd: Instance.directory },
    )

    // If git diff fails, return empty patch
    if (result.exitCode !== 0) {
      log.warn("failed to get diff", { hash, exitCode: result.exitCode })
      return { hash, files: [] }
    }

    const files = result.stdout
    return {
      hash,
      files: files
        .trim()
        .split("\n")
        .map((x) => x.trim())
        .filter(Boolean)
        .map((x) => path.join(Instance.worktree, x).replaceAll("\\", "/")),
    }
  }

  export async function restore(snapshot: string) {
    log.info("restore", { commit: snapshot })
    const git = gitdir()
    const readTree = runGit(["read-tree", snapshot], { gitDir: git, workTree: Instance.worktree, cwd: Instance.worktree })
    const checkout = readTree.exitCode === 0
      ? runGit(["checkout-index", "-a", "-f"], { gitDir: git, workTree: Instance.worktree, cwd: Instance.worktree })
      : readTree
    const result = checkout

    if (result.exitCode !== 0) {
      log.error("failed to restore snapshot", {
        snapshot,
        exitCode: result.exitCode,
        stderr: result.stderr,
        stdout: result.stdout,
      })
    }
    tracker().dirty = true
  }

  export async function revert(patches: Patch[]) {
    const files = new Set<string>()
    const git = gitdir()
    for (const item of patches) {
      for (const file of item.files) {
        if (files.has(file)) continue
        log.info("reverting", { file, hash: item.hash })
        const result = runGit(["checkout", item.hash, "--", file], {
          gitDir: git,
          workTree: Instance.worktree,
          cwd: Instance.worktree,
        })
        if (result.exitCode !== 0) {
          const relativePath = path.relative(Instance.worktree, file)
          const checkTree = runGit(["ls-tree", item.hash, "--", relativePath], {
            gitDir: git,
            workTree: Instance.worktree,
            cwd: Instance.worktree,
          })
          if (checkTree.exitCode === 0 && checkTree.stdout.trim()) {
            log.info("file existed in snapshot but checkout failed, keeping", {
              file,
            })
          } else {
            log.info("file did not exist in snapshot, deleting", { file })
            await fs.unlink(file).catch(() => {})
          }
        }
        files.add(file)
      }
    }
    tracker().dirty = true
  }

  export async function diff(hash: string) {
    const git = gitdir()
    await add(git)
    const result = runGit(
      ["-c", "core.autocrlf=false", "-c", "core.quotepath=false", "diff", "--no-ext-diff", hash, "--", "."],
      { gitDir: git, workTree: Instance.worktree, cwd: Instance.worktree },
    )

    if (result.exitCode !== 0) {
      log.warn("failed to get diff", {
        hash,
        exitCode: result.exitCode,
        stderr: result.stderr,
        stdout: result.stdout,
      })
      return ""
    }

    return result.stdout.trim()
  }

  export const FileDiff = z
    .object({
      file: z.string(),
      before: z.string(),
      after: z.string(),
      additions: z.number(),
      deletions: z.number(),
      status: z.enum(["added", "deleted", "modified"]).optional(),
    })
    .meta({
      ref: "FileDiff",
    })
  export type FileDiff = z.infer<typeof FileDiff>
  export async function diffFull(from: string, to: string): Promise<FileDiff[]> {
    const git = gitdir()
    return snapshotDiffFull(git, Instance.worktree, from, to)
  }

  function gitdir() {
    const project = Instance.project
    return path.join(Global.Path.data, "snapshot", project.id)
  }

  async function add(git: string) {
    const state = tracker()
    if (!state.dirty) {
      const pending = runGit(["status", "--porcelain=v1", "--untracked-files=all", "--", "."], {
        gitDir: git,
        workTree: Instance.worktree,
        cwd: Instance.directory,
      })
      if (pending.exitCode === 0 && pending.stdout.trim().length === 0) return
    }
    await syncExclude(git)
    runGit(["add", "-A", "--", "."], { gitDir: git, workTree: Instance.worktree, cwd: Instance.directory })
    state.dirty = false
  }

  async function syncExclude(git: string) {
    const file = await excludes()
    const target = path.join(git, "info", "exclude")
    await fs.mkdir(path.join(git, "info"), { recursive: true })
    if (!file) {
      await Bun.write(target, "")
      return
    }
    const text = await Bun.file(file)
      .text()
      .catch(() => "")
    await Bun.write(target, text)
  }

  async function excludes() {
    const file = runGit(["rev-parse", "--path-format=absolute", "--git-path", "info/exclude"], {
      cwd: Instance.worktree,
    }).stdout
    if (!file.trim()) return
    const exists = await fs
      .stat(file.trim())
      .then(() => true)
      .catch(() => false)
    if (!exists) return
    return file.trim()
  }

  function runGit(
    args: string[],
    opts?: {
      cwd?: string
      gitDir?: string
      workTree?: string
    },
  ) {
    const cwd = opts?.cwd ?? Instance.directory
    if (opts?.gitDir || opts?.workTree) {
      const result = gitExecEnv(cwd, args, opts?.gitDir, opts?.workTree, process.env.GIT_CONFIG_GLOBAL)
      return {
        exitCode: result.exitCode,
        stdout: result.stdout,
        stderr: result.stderr,
      }
    }
    const result = gitExec(cwd, args)
    return {
      exitCode: result.exitCode,
      stdout: result.stdout,
      stderr: result.stderr,
    }
  }
}
