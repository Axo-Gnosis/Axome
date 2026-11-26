import type { FolderEntry } from '$lib/types/backend'
import { backend } from '$lib/api/backend'
import { ensureProjectFiles, getProjectFilePaths } from '$lib/features/files/projectFiles.svelte'
import { rankPaths } from '$lib/features/command-palette/commands/files.svelte'
import { basename, dirname, splitRemotePath } from '$lib/utils/paths'
import { fileReference, joinHostPath } from './attachments'

/** A mention typed as `/…`, `~/…` or `../…` lists that folder on the chat's host instead of the project. */
export const outsidePrefix = /^(\/|~\/|\.\.\/)/

export type ProjectPaths = { folder: string; source: readonly string[]; known: Set<string>; paths: string[] }
let projectPaths: ProjectPaths | null = null
/** The project's files plus their ancestor folders, rebuilt only when the project file list changes. */
export function projectPathsFor(folder: string): ProjectPaths {
  const source = getProjectFilePaths(folder)
  if (projectPaths?.folder === folder && projectPaths.source === source) return projectPaths
  const known = new Set(source)
  for (const path of source) {
    let end = path.lastIndexOf('/')
    while (end > 0) {
      known.add(path.slice(0, end) + '/')
      end = path.lastIndexOf('/', end - 1)
    }
  }
  projectPaths = { folder, source, known, paths: [...known].sort() }
  return projectPaths
}

/** The project's paths plus what `directory` holds beyond them (ignored files). Rejects when the project list can't load. */
export async function projectMentionPaths(folder: string, directory: string): Promise<string[]> {
  await ensureProjectFiles(folder)
  const base = projectPathsFor(folder)
  // A folder still being typed may not exist yet; it adds nothing.
  const entries = await backend.files.readDir(folder, directory, false).catch(() => [])
  const extra = entries.map((entry) => entry.path + (entry.is_dir ? '/' : '')).filter((path) => !base.known.has(path))
  return extra.length ? [...base.paths, ...extra.sort()].sort() : base.paths
}

/** What the folder `typed` names on the chat's host holds; nothing when it doesn't exist. */
export async function outsideEntries(folder: string, typed: string, hidden: boolean): Promise<FolderEntry[]> {
  try {
    const server = splitRemotePath(folder)?.server
    const base = typed.startsWith('~/')
      ? await backend.folders.home(server)
      : typed.startsWith('/')
        ? server
          ? `axome://${server}/`
          : '/'
        : folder
    return await backend.folders.listEntries(joinHostPath(base, typed.replace(/^~\//, '')), hidden)
  } catch {
    return []
  }
}

/** One path a mention can become. `complete` is the typed text that lists a folder's contents. */
export type Mention = { path: string; label: string; detail?: string; folder: boolean; complete?: string }

export function projectMentions(paths: string[], text: string): Mention[] {
  return rankPaths(paths, text)
    .slice(0, 80)
    .map(({ path }) => {
      const bare = path.replace(/\/$/, '')
      const folder = bare !== path
      return {
        path,
        label: basename(bare) + (folder ? '/' : ''),
        detail: dirname(bare) || undefined,
        folder,
        complete: folder ? path : undefined
      }
    })
}

export function outsideMentions(entries: FolderEntry[], text: string, folderPath: string): Mention[] {
  const typed = text.slice(0, text.lastIndexOf('/') + 1)
  const leaf = text.slice(typed.length).toLowerCase()
  return entries
    .filter((entry) => entry.name.toLowerCase().startsWith(leaf))
    .toSorted((a, b) => Number(b.is_dir) - Number(a.is_dir) || a.name.localeCompare(b.name))
    .map((entry) => ({
      path: fileReference(entry.path, folderPath, entry.is_dir),
      label: entry.name + (entry.is_dir ? '/' : ''),
      detail: typed,
      folder: entry.is_dir,
      complete: entry.is_dir ? `${typed}${entry.name}/` : undefined
    }))
}
