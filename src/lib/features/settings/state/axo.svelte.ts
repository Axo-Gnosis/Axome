import { backend } from '$lib/api/backend'
import type { AxoSettings, ChatModel, ChatModelRole } from '$lib/types/backend'
import { getErrorMessage } from '$lib/utils/client-error'
import { saveHostSection } from './settings.svelte'

/** Models a host offers, loaded once per call; call during component init. */
export function axoModels(host: string | null): { models: ChatModel[]; loading: boolean; error: string | null } {
  const state = $state<{ models: ChatModel[]; loading: boolean; error: string | null }>({
    models: [],
    loading: true,
    error: null
  })
  void backend.axo
    .models(host)
    .then((models) => {
      state.models = models
    })
    .catch((error) => {
      state.error = getErrorMessage(error)
    })
    .finally(() => {
      state.loading = false
    })
  return state
}

export const BUILTIN_ROLES = ['default', 'reason', 'work', 'dash', 'conduct'] as const

/** Roles as settings define them, for pickers: built-ins first, then custom names. */
export function settingsRoles(modelRoles: Record<string, string> | undefined, extra: string[] = []): ChatModelRole[] {
  const names = new Set([...BUILTIN_ROLES, ...Object.keys(modelRoles ?? {}), ...extra])
  return [...names].map((name) => ({ name, model: modelRoles?.[name] ?? null }))
}

export function saveAxo(patch: Record<string, unknown>, host: string | null): Promise<AxoSettings> {
  return saveHostSection(
    host,
    (folder) => backend.settings.setAxo(patch, folder),
    (current, axo) => ({ ...current, axo })
  )
}
