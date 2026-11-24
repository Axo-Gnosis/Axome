import type { ExtensionSettingsTarget } from '$lib/features/settings/dialog/state.svelte'
import type { ExtensionsState } from '$lib/features/settings/state/extensions.svelte'
import type { AxoExtensions } from '$lib/types/backend'
import { splitRemotePath } from '$lib/utils/paths'
import { parseExtensionOrigin, type ExtensionOrigin } from './elements/tool/payload'

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
