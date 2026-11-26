import type { FolderEntry } from '$lib/types/backend'
import { backend } from '$lib/api/backend'
import { ensureProjectFiles, getProjectFilePaths } from '$lib/features/files/projectFiles.svelte'
import { rankPaths } from '$lib/features/command-palette/commands/files.svelte'
import { basename, dirname, splitRemotePath } from '$lib/utils/paths'
import { fileReference } from './attachments'

/** The `@mention` being typed at the end of the text before the caret; group 1 is what follows `@`. */
export const mentionAt = /(?:^|\s)@(\S*)$/

/** A mention typed as `/…`, `~/…` or `../…` lists that folder on the chat's host instead of the project. */
export const outsidePrefix = /^(\/|~\/|\.\.\/)/

/** The folder a mention has typed so far: everything up to its last `/`. */
const typedFolder = (text: string) => text.slice(0, text.lastIndexOf('/') + 1)

type ProjectPaths = { folder: string; source: readonly string[]; known: Set<string>; paths: string[] }
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
async function projectMentionPaths(folder: string, directory: string): Promise<string[]> {
  await ensureProjectFiles(folder)
  const base = projectPathsFor(folder)
  // A folder still being typed may not exist yet; it adds nothing.
  const entries = await backend.files.readDir(folder, directory, false).catch(() => [])
  const extra = entries.map((entry) => entry.path + (entry.is_dir ? '/' : '')).filter((path) => !base.known.has(path))
  return extra.length ? [...base.paths, ...extra.sort()].sort() : base.paths
}

/** `relative` (which may climb with `..`) joined onto absolute `base`, keeping an `axome://server` prefix. */
function joinHostPath(base: string, relative: string): string {
  const remote = splitRemotePath(base)
  const segments: string[] = []
  for (const segment of `${remote?.path ?? base}/${relative}`.split('/')) {
    if (segment === '..') segments.pop()
    else if (segment && segment !== '.') segments.push(segment)
  }
  return `${remote ? `axome://${remote.server}` : ''}/${segments.join('/')}`
}

/** What the folder `typed` names on the chat's host holds; nothing when it doesn't exist. */
async function outsideEntries(folder: string, typed: string, hidden: boolean): Promise<FolderEntry[]> {
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

/** What mentions are ranked within: the project's paths, or what the outside folder `typed` holds. */
export type Listing = { paths?: string[]; outside?: { typed: string; entries: FolderEntry[] } }

/**
 * Lists what a mention can name, once per folder typed: the project's paths for a project mention,
 * the folder's entries for an outside one. Each kind keeps its last listing, a rejected one too,
 * so a failure is asked again only when the typed folder changes.
 */
export function mentionLister(): (folder: string, text: string) => Promise<Listing> {
  const held: Partial<Record<'paths' | 'outside', { key: string; listing: Promise<Listing> }>> = {}
  return (folder, text) => {
    const typed = typedFolder(text)
    const outside = outsidePrefix.test(text)
    const hidden = text.slice(typed.length).startsWith('.')
    const kind = outside ? 'outside' : 'paths'
    const key = outside ? `${folder}\0${typed}\0${hidden}` : `${folder}\0${typed}`
    if (held[kind]?.key !== key)
      held[kind] = {
        key,
        listing: outside
          ? outsideEntries(folder, typed, hidden).then((entries) => ({ outside: { typed, entries } }))
          : projectMentionPaths(folder, typed.slice(0, -1)).then((paths) => ({ paths }))
      }
    return held[kind].listing
  }
}

/** One path a mention can become. `complete` is the typed text that lists a folder's contents. */
export type Mention = { path: string; label: string; detail?: string; folder: boolean; complete?: string }

/** The mentions `text` matches within `listing`; none while the outside folder it names isn't listed yet. */
export function listedMentions(listing: Listing, text: string, folderPath: string): Mention[] {
  if (!outsidePrefix.test(text)) return projectMentions(listing.paths ?? [], text)
  return listing.outside?.typed === typedFolder(text) ? outsideMentions(listing.outside.entries, text, folderPath) : []
}

function projectMentions(paths: string[], text: string): Mention[] {
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

function outsideMentions(entries: FolderEntry[], text: string, folderPath: string): Mention[] {
  const typed = typedFolder(text)
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
