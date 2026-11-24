import { backend } from '$lib/api/backend'
import { notify } from '$lib/features/notifications/state.svelte'
import { basename, splitRemotePath } from '$lib/utils/paths'
import type { IntegrationCatalog, LanguageFormatterPolicy, SettingsPage } from '$lib/types/backend'

let catalogs = $state.raw<Record<string, IntegrationCatalog>>({})
const loads = new Map<string, Promise<IntegrationCatalog>>()

/** '' for this host (desktop, or the server in the web UI); the server name for axome:// paths. */
export function integrationHostKey(folderPath?: string | null): string {
  return splitRemotePath(folderPath ?? '')?.server ?? ''
}

export function loadIntegrationCatalog(folderPath?: string | null): Promise<IntegrationCatalog> {
  const key = integrationHostKey(folderPath)
  const existing = loads.get(key)
  if (existing) return existing

  const load = backend.integrations
    .getCatalog(folderPath)
    .then((result) => {
      catalogs = { ...catalogs, [key]: result }
      for (const diagnostic of result.diagnostics) {
        notify.warning('Integration not loaded', diagnostic.message)
      }
      return result
    })
    .catch((error) => {
      loads.delete(key)
      throw error
    })

  loads.set(key, load)
  return load
}

export function getIntegrationCatalog(folderPath?: string | null): IntegrationCatalog | null {
  const key = integrationHostKey(folderPath)
  return Object.hasOwn(catalogs, key) ? catalogs[key] : null
}

export function getSettingsPages(folderPath?: string | null): SettingsPage[] {
  return getIntegrationCatalog(folderPath)?.settings.pages ?? []
}

export function getFormatterPolicy(
  folderPath: string | null | undefined,
  languageId: string
): LanguageFormatterPolicy | null {
  return (
    getIntegrationCatalog(folderPath)?.runtime.formatter_policies.find((policy) => policy.language_id === languageId) ??
    null
  )
}

export function getLanguageForFilePath(folderPath: string | null | undefined, filePath: string): string | null {
  const fileName = basename(filePath)
  const extension = getNormalizedExtension(fileName)

  for (const contribution of getIntegrationCatalog(folderPath)?.runtime.languages ?? []) {
    if (contribution.filenames.some((name) => name === fileName)) {
      return contribution.id
    }
    if (
      extension &&
      contribution.extensions.some((value) => value === extension || normalizeExtension(value) === extension)
    ) {
      return contribution.id
    }
  }

  return null
}

function getNormalizedExtension(fileName: string): string | null {
  const lastDot = fileName.lastIndexOf('.')
  if (lastDot < 0) return null
  return normalizeExtension(fileName.slice(lastDot))
}

function normalizeExtension(value: string): string {
  const trimmed = value.trim().toLowerCase()
  return trimmed.startsWith('.') ? trimmed : `.${trimmed}`
}
