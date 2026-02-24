import { BusEvent } from "@/bus/bus-event"
import { Bus } from "@/bus"
import z from "zod"
import { Instance } from "../project/instance"
import { Log } from "../util/log"
import { FileIgnore } from "./ignore"
import { Config } from "../config/config"
import path from "path"
// @ts-ignore
import { createWrapper } from "@parcel/watcher/wrapper"
import { lazy } from "@/util/lazy"
import { withTimeout } from "@/util/timeout"
import type ParcelWatcher from "@parcel/watcher"
import { $ } from "bun"
import { Flag } from "@/flag/flag"
import { readdir } from "fs/promises"

const SUBSCRIBE_TIMEOUT_MS = 10_000
const POLL_MS = 100

declare const OPENCODE_LIBC: string | undefined

type CoreEvent = {
  path: string
  kind: string
}

type CoreWatcher = {
  watch: (path: string) => void
  unwatch: () => void
  nextEvent: () => CoreEvent | null | undefined
}

type Core = {
  FileWatcher?: new () => CoreWatcher
}

export namespace FileWatcher {
  const log = Log.create({ service: "file.watcher" })

  export const Event = {
    Updated: BusEvent.define(
      "file.watcher.updated",
      z.object({
        file: z.string(),
        event: z.union([z.literal("add"), z.literal("change"), z.literal("unlink")]),
      }),
    ),
  }

  const watcher = lazy((): typeof import("@parcel/watcher") | undefined => {
    try {
      const binding = require(
        `@parcel/watcher-${process.platform}-${process.arch}${process.platform === "linux" ? `-${OPENCODE_LIBC || "glibc"}` : ""}`,
      )
      return createWrapper(binding) as typeof import("@parcel/watcher")
    } catch (error) {
      log.error("failed to load watcher binding", { error })
      return
    }
  })

  const state = Instance.state(
    async () => {
      log.info("init")
      const cfg = await Config.get()
      const backend = (() => {
        if (process.platform === "win32") return "windows"
        if (process.platform === "darwin") return "fs-events"
        if (process.platform === "linux") return "inotify"
      })()
      if (!backend) {
        log.error("watcher backend not supported", { platform: process.platform })
        return {}
      }
      log.info("watcher backend", { platform: process.platform, backend })

      const cfgIgnores = cfg.watcher?.ignore ?? []
      const core = (await import("@opencode-ai/core").catch(() => undefined)) as Core | undefined
      const ctor = core?.FileWatcher
      if (Flag.OPENCODE_EXPERIMENTAL_FILEWATCHER && ctor) {
        const watched = [{ dir: Instance.directory, ignore: [...FileIgnore.PATTERNS, ...cfgIgnores] }]
        if (Instance.project.vcs === "git") {
          const vcsDir = await $`git rev-parse --git-dir`
            .quiet()
            .nothrow()
            .cwd(Instance.worktree)
            .text()
            .then((x) => path.resolve(Instance.worktree, x.trim()))
            .catch(() => undefined)
          if (vcsDir && !cfgIgnores.includes(".git") && !cfgIgnores.includes(vcsDir)) {
            const gitDirContents = await readdir(vcsDir).catch(() => [])
            watched.push({
              dir: vcsDir,
              ignore: gitDirContents.filter((entry) => entry !== "HEAD"),
            })
          }
        }

        const rust = watched.flatMap((item) => {
          const w = new ctor()
          const ok = Promise.resolve()
            .then(() => w.watch(item.dir))
            .then(() => true)
            .catch((error) => {
              log.error("failed to start rust watcher", { error, path: item.dir })
              return false
            })
          return [{ dir: item.dir, ignore: item.ignore, watcher: w, ok }]
        })

        const ready = await Promise.all(rust.map((item) => item.ok))
        const active = rust.filter((_, i) => ready[i])
        if (!active.length) return {}

        const timer = setInterval(() => {
          for (const item of active) {
            for (;;) {
              const evt = item.watcher.nextEvent()
              if (!evt) break
              const rel = path.relative(item.dir, evt.path)
              const file = rel.startsWith("..") ? evt.path : rel
              if (FileIgnore.match(file, { extra: item.ignore })) continue
              if (evt.kind === "create") Bus.publish(Event.Updated, { file: evt.path, event: "add" })
              if (evt.kind === "write") Bus.publish(Event.Updated, { file: evt.path, event: "change" })
              if (evt.kind === "remove") Bus.publish(Event.Updated, { file: evt.path, event: "unlink" })
            }
          }
        }, POLL_MS)

        return {
          stop: async () => {
            clearInterval(timer)
            await Promise.all(
              active.map((item) =>
                Promise.resolve()
                  .then(() => item.watcher.unwatch())
                  .catch(() => {}),
              ),
            )
          },
        }
      }

      const w = watcher()
      if (!w) return {}

      const subscribe: ParcelWatcher.SubscribeCallback = (err, evts) => {
        if (err) return
        for (const evt of evts) {
          if (evt.type === "create") Bus.publish(Event.Updated, { file: evt.path, event: "add" })
          if (evt.type === "update") Bus.publish(Event.Updated, { file: evt.path, event: "change" })
          if (evt.type === "delete") Bus.publish(Event.Updated, { file: evt.path, event: "unlink" })
        }
      }

      const subs: ParcelWatcher.AsyncSubscription[] = []

      if (Flag.OPENCODE_EXPERIMENTAL_FILEWATCHER) {
        const pending = w.subscribe(Instance.directory, subscribe, {
          ignore: [...FileIgnore.PATTERNS, ...cfgIgnores],
          backend,
        })
        const sub = await withTimeout(pending, SUBSCRIBE_TIMEOUT_MS).catch((err) => {
          log.error("failed to subscribe to Instance.directory", { error: err })
          pending.then((s) => s.unsubscribe()).catch(() => {})
          return undefined
        })
        if (sub) subs.push(sub)
      }

      if (Instance.project.vcs === "git") {
        const vcsDir = await $`git rev-parse --git-dir`
          .quiet()
          .nothrow()
          .cwd(Instance.worktree)
          .text()
          .then((x) => path.resolve(Instance.worktree, x.trim()))
          .catch(() => undefined)
        if (vcsDir && !cfgIgnores.includes(".git") && !cfgIgnores.includes(vcsDir)) {
          const gitDirContents = await readdir(vcsDir).catch(() => [])
          const ignoreList = gitDirContents.filter((entry) => entry !== "HEAD")
          const pending = w.subscribe(vcsDir, subscribe, {
            ignore: ignoreList,
            backend,
          })
          const sub = await withTimeout(pending, SUBSCRIBE_TIMEOUT_MS).catch((err) => {
            log.error("failed to subscribe to vcsDir", { error: err })
            pending.then((s) => s.unsubscribe()).catch(() => {})
            return undefined
          })
          if (sub) subs.push(sub)
        }
      }

      return { subs }
    },
    async (state) => {
      if (state.stop) {
        await state.stop()
        return
      }
      if (!state.subs) return
      await Promise.all(state.subs.map((sub) => sub?.unsubscribe()))
    },
  )

  export function init() {
    if (Flag.OPENCODE_EXPERIMENTAL_DISABLE_FILEWATCHER) {
      return
    }
    state()
  }
}
