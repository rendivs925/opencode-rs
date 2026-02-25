import z from "zod"
import * as path from "path"
import * as fs from "fs/promises"
import { Log } from "../util/log"
import { deriveNewContentsFromChunks as derivePatchChunks, parseApplyPatch } from "../core/native"

export namespace Patch {
  const log = Log.create({ service: "patch" })

  // Schema definitions
  export const PatchSchema = z.object({
    patchText: z.string().describe("The full patch text that describes all changes to be made"),
  })

  export type PatchParams = z.infer<typeof PatchSchema>

  // Core types matching the Rust implementation
  export interface ApplyPatchArgs {
    patch: string
    hunks: Hunk[]
    workdir?: string
  }

  export type Hunk =
    | { type: "add"; path: string; contents: string }
    | { type: "delete"; path: string }
    | { type: "update"; path: string; move_path?: string; chunks: UpdateFileChunk[] }

  export interface UpdateFileChunk {
    old_lines: string[]
    new_lines: string[]
    change_context?: string
    is_end_of_file?: boolean
  }

  export interface ApplyPatchAction {
    changes: Map<string, ApplyPatchFileChange>
    patch: string
    cwd: string
  }

  export type ApplyPatchFileChange =
    | { type: "add"; content: string }
    | { type: "delete"; content: string }
    | { type: "update"; unified_diff: string; move_path?: string; new_content: string }

  export interface AffectedPaths {
    added: string[]
    modified: string[]
    deleted: string[]
  }

  export enum ApplyPatchError {
    ParseError = "ParseError",
    IoError = "IoError",
    ComputeReplacements = "ComputeReplacements",
    ImplicitInvocation = "ImplicitInvocation",
  }

  export enum MaybeApplyPatch {
    Body = "Body",
    ShellParseError = "ShellParseError",
    PatchParseError = "PatchParseError",
    NotApplyPatch = "NotApplyPatch",
  }

  export enum MaybeApplyPatchVerified {
    Body = "Body",
    ShellParseError = "ShellParseError",
    CorrectnessError = "CorrectnessError",
    NotApplyPatch = "NotApplyPatch",
  }

  export function parsePatch(patchText: string): { hunks: Hunk[] } {
    const parsed = parseApplyPatch(patchText)
    const hunks: Hunk[] = []
    for (const hunk of parsed.hunks) {
      if (hunk.hunkType === "add") {
        hunks.push({ type: "add", path: hunk.path, contents: hunk.contents ?? "" })
        continue
      }
      if (hunk.hunkType === "delete") {
        hunks.push({ type: "delete", path: hunk.path })
        continue
      }
      hunks.push({
        type: "update",
        path: hunk.path,
        move_path: hunk.movePath,
        chunks: (hunk.chunks ?? []).map((chunk) => ({
          old_lines: chunk.oldLines,
          new_lines: chunk.newLines,
          change_context: chunk.changeContext,
          is_end_of_file: chunk.isEndOfFile,
        })),
      })
    }
    return { hunks }
  }

  // Apply patch functionality
  export function maybeParseApplyPatch(
    argv: string[],
  ):
    | { type: MaybeApplyPatch.Body; args: ApplyPatchArgs }
    | { type: MaybeApplyPatch.PatchParseError; error: Error }
    | { type: MaybeApplyPatch.NotApplyPatch } {
    const APPLY_PATCH_COMMANDS = ["apply_patch", "applypatch"]

    // Direct invocation: apply_patch <patch>
    if (argv.length === 2 && APPLY_PATCH_COMMANDS.includes(argv[0])) {
      try {
        const { hunks } = parsePatch(argv[1])
        return {
          type: MaybeApplyPatch.Body,
          args: {
            patch: argv[1],
            hunks,
          },
        }
      } catch (error) {
        return {
          type: MaybeApplyPatch.PatchParseError,
          error: error as Error,
        }
      }
    }

    // Bash heredoc form: bash -lc 'apply_patch <<"EOF" ...'
    if (argv.length === 3 && argv[0] === "bash" && argv[1] === "-lc") {
      // Simple extraction - in real implementation would need proper bash parsing
      const script = argv[2]
      const heredocMatch = script.match(/apply_patch\s*<<['"](\w+)['"]\s*\n([\s\S]*?)\n\1/)

      if (heredocMatch) {
        const patchContent = heredocMatch[2]
        try {
          const { hunks } = parsePatch(patchContent)
          return {
            type: MaybeApplyPatch.Body,
            args: {
              patch: patchContent,
              hunks,
            },
          }
        } catch (error) {
          return {
            type: MaybeApplyPatch.PatchParseError,
            error: error as Error,
          }
        }
      }
    }

    return { type: MaybeApplyPatch.NotApplyPatch }
  }

  // File content manipulation
  interface ApplyPatchFileUpdate {
    unified_diff: string
    content: string
  }

  export function deriveNewContentsFromChunks(filePath: string, chunks: UpdateFileChunk[]): ApplyPatchFileUpdate {
    const mapped = chunks.map((chunk) => ({
      oldLines: chunk.old_lines,
      newLines: chunk.new_lines,
      changeContext: chunk.change_context,
      isEndOfFile: chunk.is_end_of_file,
    }))
    const next = derivePatchChunks(filePath, mapped)
    return {
      unified_diff: next.unifiedDiff,
      content: next.content,
    }
  }

  // Apply hunks to filesystem
  export async function applyHunksToFiles(hunks: Hunk[]): Promise<AffectedPaths> {
    if (hunks.length === 0) {
      throw new Error("No files were modified.")
    }

    const added: string[] = []
    const modified: string[] = []
    const deleted: string[] = []

    for (const hunk of hunks) {
      switch (hunk.type) {
        case "add":
          // Create parent directories
          const addDir = path.dirname(hunk.path)
          if (addDir !== "." && addDir !== "/") {
            await fs.mkdir(addDir, { recursive: true })
          }

          await fs.writeFile(hunk.path, hunk.contents, "utf-8")
          added.push(hunk.path)
          log.info(`Added file: ${hunk.path}`)
          break

        case "delete":
          await fs.unlink(hunk.path)
          deleted.push(hunk.path)
          log.info(`Deleted file: ${hunk.path}`)
          break

        case "update":
          const fileUpdate = deriveNewContentsFromChunks(hunk.path, hunk.chunks)

          if (hunk.move_path) {
            // Handle file move
            const moveDir = path.dirname(hunk.move_path)
            if (moveDir !== "." && moveDir !== "/") {
              await fs.mkdir(moveDir, { recursive: true })
            }

            await fs.writeFile(hunk.move_path, fileUpdate.content, "utf-8")
            await fs.unlink(hunk.path)
            modified.push(hunk.move_path)
            log.info(`Moved file: ${hunk.path} -> ${hunk.move_path}`)
          } else {
            // Regular update
            await fs.writeFile(hunk.path, fileUpdate.content, "utf-8")
            modified.push(hunk.path)
            log.info(`Updated file: ${hunk.path}`)
          }
          break
      }
    }

    return { added, modified, deleted }
  }

  // Main patch application function
  export async function applyPatch(patchText: string): Promise<AffectedPaths> {
    const { hunks } = parsePatch(patchText)
    return applyHunksToFiles(hunks)
  }

  // Async version of maybeParseApplyPatchVerified
  export async function maybeParseApplyPatchVerified(
    argv: string[],
    cwd: string,
  ): Promise<
    | { type: MaybeApplyPatchVerified.Body; action: ApplyPatchAction }
    | { type: MaybeApplyPatchVerified.CorrectnessError; error: Error }
    | { type: MaybeApplyPatchVerified.NotApplyPatch }
  > {
    // Detect implicit patch invocation (raw patch without apply_patch command)
    if (argv.length === 1) {
      try {
        parsePatch(argv[0])
        return {
          type: MaybeApplyPatchVerified.CorrectnessError,
          error: new Error(ApplyPatchError.ImplicitInvocation),
        }
      } catch {
        // Not a patch, continue
      }
    }

    const result = maybeParseApplyPatch(argv)

    switch (result.type) {
      case MaybeApplyPatch.Body:
        const { args } = result
        const effectiveCwd = args.workdir ? path.resolve(cwd, args.workdir) : cwd
        const changes = new Map<string, ApplyPatchFileChange>()

        for (const hunk of args.hunks) {
          const resolvedPath = path.resolve(
            effectiveCwd,
            hunk.type === "update" && hunk.move_path ? hunk.move_path : hunk.path,
          )

          switch (hunk.type) {
            case "add":
              changes.set(resolvedPath, {
                type: "add",
                content: hunk.contents,
              })
              break

            case "delete":
              // For delete, we need to read the current content
              const deletePath = path.resolve(effectiveCwd, hunk.path)
              try {
                const content = await fs.readFile(deletePath, "utf-8")
                changes.set(resolvedPath, {
                  type: "delete",
                  content,
                })
              } catch (error) {
                return {
                  type: MaybeApplyPatchVerified.CorrectnessError,
                  error: new Error(`Failed to read file for deletion: ${deletePath}`),
                }
              }
              break

            case "update":
              const updatePath = path.resolve(effectiveCwd, hunk.path)
              try {
                const fileUpdate = deriveNewContentsFromChunks(updatePath, hunk.chunks)
                changes.set(resolvedPath, {
                  type: "update",
                  unified_diff: fileUpdate.unified_diff,
                  move_path: hunk.move_path ? path.resolve(effectiveCwd, hunk.move_path) : undefined,
                  new_content: fileUpdate.content,
                })
              } catch (error) {
                return {
                  type: MaybeApplyPatchVerified.CorrectnessError,
                  error: error as Error,
                }
              }
              break
          }
        }

        return {
          type: MaybeApplyPatchVerified.Body,
          action: {
            changes,
            patch: args.patch,
            cwd: effectiveCwd,
          },
        }

      case MaybeApplyPatch.PatchParseError:
        return {
          type: MaybeApplyPatchVerified.CorrectnessError,
          error: result.error,
        }

      case MaybeApplyPatch.NotApplyPatch:
        return { type: MaybeApplyPatchVerified.NotApplyPatch }
    }
  }
}
