import { $ } from "bun"
import path from "path"

type Core = {
  extractArchive?: (archivePath: string, outputDir: string) => unknown
}

const core = (await import("@opencode-ai/core").catch(() => undefined)) as Core | undefined

export namespace Archive {
  export async function extractZip(zipPath: string, destDir: string) {
    const native = core?.extractArchive
    if (native) {
      const ok = await Promise.resolve()
        .then(() => native(zipPath, destDir))
        .then(() => true)
        .catch(() => false)
      if (ok) return
    }

    if (process.platform === "win32") {
      const winZipPath = path.resolve(zipPath)
      const winDestDir = path.resolve(destDir)
      // $global:ProgressPreference suppresses PowerShell's blue progress bar popup
      const cmd = `$global:ProgressPreference = 'SilentlyContinue'; Expand-Archive -Path '${winZipPath}' -DestinationPath '${winDestDir}' -Force`
      await $`powershell -NoProfile -NonInteractive -Command ${cmd}`.quiet()
    } else {
      await $`unzip -o -q ${zipPath} -d ${destDir}`.quiet()
    }
  }
}
