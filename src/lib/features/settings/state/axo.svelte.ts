import { onMount } from 'svelte'
import { backend } from '$lib/api/backend'
import type { AxoSettings, ChatModel, ChatModelRole } from '$lib/types/backend'
import { getErrorMessage } from '$lib/utils/client-error'
import { splitRemotePath } from '$lib/utils/paths'
import { saveHostSection } from './settings.svelte'

/** Host-scoped axo data, loaded on mount and refreshed when models or extensions change. */
export function axoHostResource<T>(
  host: string | null,
  initial: T,
  load: (host: string | null) => Promise<T>,
  options: { loadingOnRefresh?: boolean } = {}
): { data: T; loading: boolean; error: string | null; refresh: () => Promise<void> } {
  const state = $state<{ data: T; loading: boolean; error: string | null }>({
    data: initial,
    loading: true,
    error: null
  })
  let refreshCurrent: (() => Promise<void>) | null = null
  onMount(() => {
    const server = host === null ? null : (splitRemotePath(host)?.server ?? null)
    let disposed = false
    let generation = 0

    async function refresh() {
      if (disposed) return
      const request = ++generation
      if (options.loadingOnRefresh) {
        state.loading = true
        state.error = null
      }
      try {
        const next = await load(host)
        if (request !== generation) return
        state.data = next
        state.error = null
      } catch (error) {
        if (request === generation) state.error = getErrorMessage(error)
      } finally {
        if (request === generation) state.loading = false
      }
    }

    refreshCurrent = refresh
    const changed = (event: { server: string | null }) => {
      if (event.server === server) void refresh()
    }
    const listeners = [backend.axo.onModelsChanged(changed), backend.axo.onExtensionsChanged(changed)]
    void Promise.allSettled(listeners).then(refresh)
    return () => {
      disposed = true
      refreshCurrent = null
      generation++
      for (const listener of listeners)
        void listener.then(
          (stop) => stop(),
          () => {}
        )
    }
  })
  return {
    get data() {
      return state.data
    },
    get loading() {
      return state.loading
    },
    get error() {
      return state.error
    },
    refresh: () => refreshCurrent?.() ?? Promise.resolve()
  }
}

/** Models a host offers, kept current while the calling component is mounted. */
export function axoModels(host: string | null): { models: ChatModel[]; loading: boolean; error: string | null } {
  const resource = axoHostResource(host, [] as ChatModel[], backend.axo.models)
  return {
    get models() {
      return resource.data
    },
    get loading() {
      return resource.loading
    },
    get error() {
      return resource.error
    }
  }
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
