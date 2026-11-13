import { backend } from '$lib/api/backend'
import type { ChatModelCatalog, SettingsChangedEvent } from '$lib/types/backend'
import { getErrorMessage } from '$lib/utils/client-error'
import { splitRemotePath } from '$lib/utils/paths'

type ModelState = { catalog: ChatModelCatalog | null; loading: boolean; error: string | null }

export const MODELS_HINT = 'Add a provider in Settings › Providers.'

const states = new Map<string, ModelState>()
const loads = new Map<string, Promise<void>>()

export function getChatModels(folderPath: string): ModelState {
  const existing = states.get(folderPath)
  if (existing) return existing
  const state = $state<ModelState>({ catalog: null, loading: false, error: null })
  states.set(folderPath, state)
  return state
}

export function loadChatModels(folderPath: string, force = false): Promise<void> {
  const existing = loads.get(folderPath)
  // A provider change must fetch again after any pre-change request has settled.
  if (existing) return force ? existing.then(() => loadChatModels(folderPath, true)) : existing
  const state = getChatModels(folderPath)
  if (state.catalog && !force) return Promise.resolve()

  state.loading = true
  state.error = null
  const load = backend.chat
    .models(folderPath)
    .then((catalog) => {
      state.catalog = catalog
    })
    .catch((error: unknown) => {
      state.error = getErrorMessage(error)
    })
    .finally(() => {
      state.loading = false
      loads.delete(folderPath)
    })
  loads.set(folderPath, load)
  return load
}

export async function reloadChatModels(host: string | null): Promise<void> {
  const server = host === null ? null : (splitRemotePath(host)?.server ?? null)
  await Promise.all(
    [...states.keys()]
      .filter((folder) => (splitRemotePath(folder)?.server ?? null) === server)
      .map((folder) => loadChatModels(folder, true))
  )
}

/** Settings changes affect every loaded folder in the layer's scope. */
export function refreshChatModels(event: SettingsChangedEvent): void {
  if (event.layer === 'global') {
    for (const folder of states.keys()) void loadChatModels(folder, true)
  } else if (event.folder_path && states.has(event.folder_path)) {
    void loadChatModels(event.folder_path, true)
  }
}
