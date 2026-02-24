import { extractArchive } from "@/core/native"

export namespace Archive {
  export async function extractZip(zipPath: string, destDir: string) {
    extractArchive(zipPath, destDir)
  }
}
