import { extractArchive, extractTarBz2, extractTarGz, listArchiveContents, readArchiveEntry } from "@/core/native"

export namespace Archive {
  export async function extractZip(zipPath: string, destDir: string) {
    if (zipPath.endsWith(".tar.gz") || zipPath.endsWith(".tgz")) {
      extractTarGz(zipPath, destDir)
      return
    }
    if (zipPath.endsWith(".tar.bz2") || zipPath.endsWith(".tbz2")) {
      extractTarBz2(zipPath, destDir)
      return
    }
    extractArchive(zipPath, destDir)
  }

  export async function list(path: string) {
    return listArchiveContents(path)
  }

  export async function read(path: string, entry: string) {
    return readArchiveEntry(path, entry)
  }
}
