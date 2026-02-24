import { BusEvent } from "@/bus/bus-event"
import { Bus } from "@/bus"
import { FileWatcher as CoreFileWatcher } from "@/core/native"
import z from "zod"
import { Instance } from "../project/instance"
import { Log } from "../util/log"
import { FileIgnore } from "./ignore"
import { Config } from "../config/config"
import path from "path"
import { $ } from "bun"
import { Flag } from "@/flag/flag"
import { readdir } from "fs/promises"

const POLL_MS = 100

type CoreEvent = {
  path: string
  kind: "add" | "change" | "unlink"
}

type CoreWatcher = {
  watch: (path: string) => void
  unwatch: () => void
  nextEvent: () => CoreEvent | null | undefined
  nextEvents: (limit?: number, ignorePatterns?: string[]) => CoreEvent[]
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

  const state = Instance.state(
    async () => {
      log.info("init")
      const cfg = await Config.get()
      const cfgIgnores = cfg.watcher?.ignore ?? []

      if (!Flag.OPENCODE_EXPERIMENTAL_FILEWATCHER) return {}

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
        const watcher = new CoreFileWatcher() as CoreWatcher
        const ok = Promise.resolve()
          .then(() => watcher.watch(item.dir))
          .then(() => true)
          .catch((error) => {
            log.error("failed to start rust watcher", { error, path: item.dir })
            return false
          })
        return [{ dir: item.dir, ignore: item.ignore, watcher, ok }]
      })

      const ready = await Promise.all(rust.map((item) => item.ok))
      const active = rust.filter((_, i) => ready[i])
      if (!active.length) return {}

      const timer = setInterval(() => {
        for (const item of active) {
          for (const evt of item.watcher.nextEvents(128, item.ignore)) {
            Bus.publish(Event.Updated, { file: evt.path, event: evt.kind })
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
    },
    async (state) => {
      if (!state.stop) return
      await state.stop()
    },
  )

  export function init() {
    if (Flag.OPENCODE_EXPERIMENTAL_DISABLE_FILEWATCHER) {
      return
    }
    state()
  }
}
