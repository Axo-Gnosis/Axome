import { onMount } from 'svelte'
import { backend } from '$lib/api/backend'
import type { AxoSettings, ChatModel, ChatModelRole } from '$lib/types/backend'
import { getErrorMessage } from '$lib/utils/client-error'
import { splitRemotePath } from '$lib/utils/paths'
import { saveHostSection } from './settings.svelte'

/** Models a host offers, kept current while the calling component is mounted. */
export function axoModels(host: string | null): { models: ChatModel[]; loading: boolean; error: string | null } {
  const state = $state<{ models: ChatModel[]; loading: boolean; error: string | null }>({
    models: [],
    loading: true,
    error: null
  })
  onMount(() => {
    const server = host === null ? null : (splitRemotePath(host)?.server ?? null)
    let disposed = false
    let generation = 0

    async function refresh() {
      if (disposed) return
      const request = ++generation
      try {
        const models = await backend.axo.models(host)
        if (request !== generation) return
        state.models = models
        state.error = null
      } catch (error) {
        if (request === generation) state.error = getErrorMessage(error)
      } finally {
        if (request === generation) state.loading = false
      }
    }

    const changed = (event: { server: string | null }) => {
      if (event.server === server) void refresh()
    }
    const listeners = [backend.axo.onModelsChanged(changed), backend.axo.onExtensionsChanged(changed)]
    void Promise.allSettled(listeners).then(refresh)
    return () => {
      disposed = true
      generation++
      for (const listener of listeners)
        void listener.then(
          (stop) => stop(),
          () => {}
        )
    }
  })
  return state
}

export const BUILTIN_ROLES = ['default', 'reason', 'work', 'dash', 'conduct', 'memory'] as const

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
