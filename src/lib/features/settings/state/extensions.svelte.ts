import { backend } from '$lib/api/backend'
import type { Unsubscribe } from '$lib/api/transport'
import type {
  AxoEnabledExtension,
  AxoExtension,
  AxoExtensionAdds,
  AxoExtensions,
  AxoProjectAccess,
  AxoProvidersPayload
} from '$lib/types/backend'
import { getErrorMessage } from '$lib/utils/client-error'
import { splitRemotePath } from '$lib/utils/paths'

interface ExtensionCatalog {
  readonly payload: AxoExtensions | null
  readonly loading: boolean
  readonly error: string | null
  readonly busy: boolean
  readonly stale: boolean
  readonly providers: AxoProvidersPayload | null
  readonly providerError: string | null
  refresh(): Promise<void>
  run(action: () => Promise<void>): Promise<boolean>
}

interface ExtensionEntry extends ExtensionCatalog {
  server: string | null
  refs: number
  invalidate(): Promise<void>
  dispose(): void
}

export interface ExtensionsState extends ExtensionCatalog {
  setEnabled(id: string, project: boolean, enabled: boolean): Promise<boolean>
  move(id: string, project: boolean, direction: -1 | 1): Promise<boolean>
  setGroup(id: string, project: boolean, group: string | null): Promise<boolean>
  release(): void
}

function createEntry(host: string | null, scopeFolder: string | null, server: string | null): ExtensionEntry {
  let payload = $state.raw<AxoExtensions | null>(null)
  let providers = $state.raw<AxoProvidersPayload | null>(null)
  let loading = $state(true)
  let loadError = $state<string | null>(null)
  let providerError = $state<string | null>(null)
  let mutationError = $state<string | null>(null)
  let busy = $state(false)
  let disposed = false
  let dirty = false
  let fetching: Promise<void> | null = null
  let listeners: [Unsubscribe, Unsubscribe] | null = null

  async function subscribe() {
    const changed = (event: { server: string | null }) => {
      if (event.server === server && !disposed) void fetchLatest()
    }
    const results = await Promise.allSettled([
      backend.axo.onExtensionsChanged(changed),
      backend.axo.onModelsChanged(changed)
    ])
    const [extensions, models] = results
    if (!disposed && extensions.status === 'fulfilled' && models.status === 'fulfilled') {
      listeners = [extensions.value, models.value]
      return null
    }
    for (const result of results) if (result.status === 'fulfilled') result.value()
    const failed = results.find((result) => result.status === 'rejected')
    return failed ? getErrorMessage(failed.reason) : null
  }

  function fetchLatest(): Promise<void> {
    if (disposed) return Promise.resolve()
    dirty = true
    return (fetching ??= Promise.resolve()
      .then(async () => {
        const subscriptionError = listeners ? null : await subscribe()
        while (dirty && !disposed) {
          dirty = false
          loading = true
          const [next, nextProviders] = await Promise.allSettled([
            backend.axo.extensions(scopeFolder, host),
            backend.axo.providers(host, scopeFolder)
          ])
          if (disposed) return
          if (next.status === 'fulfilled') {
            payload = next.value
            loadError = subscriptionError
          } else loadError = getErrorMessage(next.reason)
          if (nextProviders.status === 'fulfilled') {
            providers = nextProviders.value
            providerError = subscriptionError
          } else providerError = getErrorMessage(nextProviders.reason)
        }
      })
      .finally(() => {
        fetching = null
        if (disposed) return
        if (dirty) return fetchLatest()
        loading = false
      }))
  }

  void fetchLatest()
  return {
    server,
    refs: 0,
    get payload() {
      return payload
    },
    get providers() {
      return providers
    },
    get loading() {
      return loading
    },
    get error() {
      return mutationError ?? loadError
    },
    get stale() {
      return loadError !== null
    },
    get providerError() {
      return providerError
    },
    get busy() {
      return busy
    },
    async refresh() {
      mutationError = null
      await fetchLatest()
    },
    invalidate: fetchLatest,
    async run(action: () => Promise<void>): Promise<boolean> {
      if (busy || disposed || !payload || loadError !== null) return false
      busy = true
      mutationError = null
      try {
        await action()
        return true
      } catch (error) {
        mutationError = getErrorMessage(error)
        return false
      } finally {
        await fetchLatest()
        busy = false
      }
    },
    dispose() {
      disposed = true
      listeners?.forEach((stop) => stop())
    }
  }
}

const entries = new Map<string, ExtensionEntry>()

export function createExtensionsState(host: string | null, folderPath: string | null): ExtensionsState {
  const server = host ? (splitRemotePath(host)?.server ?? null) : null
  const scopeFolder = folderPath ? (splitRemotePath(folderPath)?.path ?? folderPath) : null
  const key = JSON.stringify([server, scopeFolder])
  const entry = entries.get(key) ?? createEntry(host, scopeFolder, server)
  entries.set(key, entry)
  entry.refs++
  let released = false
  function replace(project: boolean, change: (enabled: AxoEnabledExtension[]) => boolean): Promise<boolean> {
    const payload = entry.payload
    const items = project ? payload?.project?.items : payload?.host
    if (released || !items || (project && !scopeFolder)) return Promise.resolve(false)
    const enabled = items.filter((item) => item.enabled).map(({ id, group }) => ({ id, group }))
    if (!change(enabled)) return Promise.resolve(false)
    return entry.run(() =>
      project && scopeFolder
        ? backend.axo.setProjectExtensionsEnabled(scopeFolder, enabled, host)
        : backend.axo.setExtensionsEnabled(enabled, host)
    )
  }

  return {
    get payload() {
      return entry.payload
    },
    get providers() {
      return entry.providers
    },
    get loading() {
      return entry.loading
    },
    get error() {
      return entry.error
    },
    get stale() {
      return entry.stale
    },
    get providerError() {
      return entry.providerError
    },
    get busy() {
      return entry.busy
    },
    refresh: entry.refresh,
    run: entry.run,
    setEnabled(id, project, on) {
      return replace(project, (enabled) => {
        const current = enabled.findIndex((item) => item.id === id)
        if (on === current >= 0) return false
        if (on) enabled.push({ id, group: null })
        else enabled.splice(current, 1)
        return true
      })
    },
    move(id, project, direction) {
      return replace(project, (enabled) => {
        const current = enabled.findIndex((item) => item.id === id)
        const next = current + direction
        if (current < 0 || next < 0 || next >= enabled.length) return false
        ;[enabled[current], enabled[next]] = [enabled[next], enabled[current]]
        return true
      })
    },
    setGroup(id, project, group) {
      return replace(project, (enabled) => {
        const item = enabled.find((item) => item.id === id)
        if (!item || item.group === group) return false
        item.group = group
        return true
      })
    },
    release() {
      if (released) return
      released = true
      if (--entry.refs > 0) return
      entry.dispose()
      entries.delete(key)
    }
  }
}

export async function refreshExtensionsForServer(server: string | null): Promise<void> {
  await Promise.all([...entries.values()].filter((entry) => entry.server === server).map((entry) => entry.invalidate()))
}

export function extensionMissing(item: AxoExtension): boolean {
  return item.origin === 'missing' || item.problem?.kind === 'notFound'
}

export function needsExtensionAttention(item: AxoExtension): boolean {
  return (
    item.failure !== null ||
    extensionMissing(item) ||
    (item.phase !== 'restarting' &&
      (item.problem !== null || item.phase === 'disabled' || item.phase === 'unavailable'))
  )
}

export function extensionStateText(item: AxoExtension, access: AxoProjectAccess | null): string {
  if (extensionMissing(item))
    return access === null
      ? "Can't find its folder in your extensions folder"
      : "Can't find its folder in .axome/extensions"
  if (item.enabled && access !== null && access !== 'trust') return 'Turned on · waits for trust'
  if (item.phase === 'restarting') return item.problem ? `Restarting… · ${item.problem.message}` : 'Restarting…'
  if (item.problem) return item.problem.message
  if (!item.enabled) {
    if (item.origin === 'installed' && item.install) return `Installed from ${item.install.origin}`
    if (item.origin === 'userFolder') return 'In your extensions folder'
    return 'Off'
  }
  switch (item.phase) {
    case null:
      return 'Turned on'
    case 'cold':
    case 'ready': {
      const adds = extensionAddsText(item.adds)
      return (item.phase === 'cold' ? 'Starts when first used' : 'Running') + (adds ? ` · ${adds}` : '')
    }
    case 'installing':
      return 'Installing dependencies…'
    case 'starting':
      return 'Starting…'
    case 'disabled':
      return 'Stopped'
    case 'unavailable':
      return 'Unavailable'
  }
}

export function extensionAddsText(adds: AxoExtensionAdds): string {
  const descriptions = adds.providers.map(({ id }) => `provider ${id}`)
  const tools = adds.tools.length
  if (tools) descriptions.push(`${tools} ${tools === 1 ? 'tool' : 'tools'}`)
  for (const command of adds.commands) descriptions.push(`/${command}`)
  for (const policy of adds.toolPolicy)
    descriptions.push(`checks tools ${policy === 'before' ? 'before' : 'after'} they run`)
  if (adds.spawnHook) descriptions.push('checks subagents before they start')
  return descriptions.join(', ')
}

export function extensionBunUnavailable(payload: AxoExtensions): boolean {
  const missingBun = (item: AxoExtension) => item.problem?.kind === 'bun'
  return payload.bun.path === null || payload.host.some(missingBun) || !!payload.project?.items.some(missingBun)
}

export function extensionRuntimeAction(item: AxoExtension, access: AxoProjectAccess | null) {
  if (extensionMissing(item)) return item.enabled ? 'off' : null
  if (item.enabled && access !== null && access !== 'trust') return null
  if (item.problem?.kind === 'dependencyInstall') return 'output'
  if (!item.enabled) return null
  if (item.phase === 'installing' || item.phase === 'starting' || item.phase === 'restarting') return null
  if (item.problem?.kind === 'dependenciesMissing' && item.origin !== 'installed') return 'dependencies'
  if (
    (item.phase === 'disabled' || item.phase === 'unavailable') &&
    (item.problem?.kind === 'crash' || item.problem?.kind === 'load')
  )
    return 'retry'
  return null
}

export function shortRevision(revision: string): string {
  return /^(?:[a-f0-9]{40}|[a-f0-9]{64})$/i.test(revision) ? revision.slice(0, 7) : revision
}
