import { backend } from '$lib/api/backend'
import type { ChatModelCatalog, SettingsChangedEvent } from '$lib/types/backend'
import { getErrorMessage } from '$lib/utils/client-error'
import { splitRemotePath } from '$lib/utils/paths'

type ModelState = { catalog: ChatModelCatalog | null; loading: boolean; error: string | null }

export const MODELS_HINT = 'Add a provider in Settings › Providers.'

const states = new Map<string, ModelState>()
const loads = new Map<string, Promise<void>>()
const queued = new Map<string, Promise<void>>()
const generations = new Map<string, number>()

function currentGeneration(folderPath: string): number {
  return generations.get(folderPath) ?? 0
}

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
  if (existing) {
    if (!force) return existing
    let next = queued.get(folderPath)
    if (!next) {
      const generation = currentGeneration(folderPath)
      next = existing.then(() => {
        if (queued.get(folderPath) === next) queued.delete(folderPath)
        if (currentGeneration(folderPath) !== generation) return
        return loadChatModels(folderPath, true)
      })
      queued.set(folderPath, next)
    }
    return next
  }
  const state = getChatModels(folderPath)
  if (state.catalog && !force) return Promise.resolve()

  const generation = currentGeneration(folderPath)
  state.loading = true
  state.error = null
  const load = backend.chat
    .models(folderPath)
    .then((catalog) => {
      if (currentGeneration(folderPath) !== generation) return
      state.catalog = catalog
    })
    .catch((error: unknown) => {
      if (currentGeneration(folderPath) !== generation) return
      state.error = getErrorMessage(error)
    })
    .finally(() => {
      if (loads.get(folderPath) === load) loads.delete(folderPath)
      if (currentGeneration(folderPath) !== generation) return
      state.loading = false
    })
  loads.set(folderPath, load)
  return load
}

export function releaseChatModelsFolder(folderPath: string): void {
  generations.set(folderPath, currentGeneration(folderPath) + 1)
  states.delete(folderPath)
  loads.delete(folderPath)
  queued.delete(folderPath)
}

export async function reloadChatModels(server: string | null): Promise<void> {
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
