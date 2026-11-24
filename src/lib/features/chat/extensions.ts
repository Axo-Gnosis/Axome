import type { ExtensionSettingsTarget } from '$lib/features/settings/dialog/state.svelte'
import type { ExtensionsState } from '$lib/features/settings/state/extensions.svelte'
import type { AxoExtensions } from '$lib/types/backend'
import { splitRemotePath } from '$lib/utils/paths'

export type ExtensionOrigin = { id: string; scoped: boolean }

/** Mirrors axo_extensions::ExtensionOrigin::parse; the scope key isn't display data. */
export function parseExtensionOrigin(label: string | null): ExtensionOrigin | null {
  const prefix = 'extension '
  if (!label?.startsWith(prefix)) return null
  const rest = label.slice(prefix.length)
  const delimiter = ' (scope '
  const split = rest.indexOf(delimiter)
  const id = split === -1 ? rest : rest.slice(0, split)
  if (!id || id.includes(' ')) return null
  if (split === -1) return { id, scoped: false }
  const key = rest.slice(split + delimiter.length, -1)
  return rest.endsWith(')') && key && !key.includes(' ') ? { id, scoped: true } : null
}

export function chatExtensionTarget(folderPath: string, origin: ExtensionOrigin | null): ExtensionSettingsTarget {
  return {
    hostFolder: splitRemotePath(folderPath) ? folderPath : null,
    projectFolder: origin?.scoped ? folderPath : null,
    id: origin?.id ?? null
  }
}

export function findExtension(payload: AxoExtensions | null, { id, scoped }: ExtensionOrigin) {
  return (scoped ? payload?.project?.items : payload?.host)?.find((item) => item.id === id)
}

/** Commands carry an id but not a scope. Only claim stopped if every possible owner is stopped. */
export function stoppedCommandTarget(state: ExtensionsState | null, folderPath: string, id: string | null) {
  if (!id || !state || state.stale) return null
  const host = findExtension(state.payload, { id, scoped: false })
  const project = findExtension(state.payload, { id, scoped: true })
  if (
    (!host && !project) ||
    (host && !(host.enabled && host.phase === 'disabled')) ||
    (project && !(project.enabled && project.phase === 'disabled'))
  )
    return null
  return chatExtensionTarget(folderPath, host && project ? null : { id, scoped: !!project })
}

export function commandClash(diagnostic: string): { name: string; origin: ExtensionOrigin } | null {
  const match = /^(.*): command "\/(\S+)" from (.+) is hidden by the (.*)$/.exec(diagnostic)
  if (!match) return null
  const [, label, name, repeated, winner] = match
  if (label !== repeated || (winner !== `built-in /${name} command` && winner !== `mode /${name}`)) return null
  const origin = parseExtensionOrigin(label)
  return origin && { name, origin }
}
