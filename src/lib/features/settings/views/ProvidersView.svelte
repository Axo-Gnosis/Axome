<script lang="ts">
  import { backend } from '$lib/api/backend'
  import * as Alert from '$lib/components/ui/alert'
  import { Badge } from '$lib/components/ui/badge'
  import { Button } from '$lib/components/ui/button'
  import { SearchInput } from '$lib/components/ui/input'
  import { Spinner } from '$lib/components/ui/progress'
  import { StatusDot } from '$lib/components/ui/status-dot'
  import { getSettingsHost, getSettingsServer } from '../state/settings.svelte'
  import { platform } from '$lib/platform'
  import type { AxoProvider, AxoProvidersPayload } from '$lib/types/backend'
  import { getErrorMessage } from '$lib/utils/client-error'
  import { onMount } from 'svelte'
  import ApiKeyDialog from './ApiKeyDialog.svelte'
  import ProviderLoginDialog from './ProviderLoginDialog.svelte'
  import SettingsSection from '../dialog/SettingsSection.svelte'

  const host = getSettingsHost()
  let providers = $state<{ payload: AxoProvidersPayload | null; loading: boolean; error: string | null }>({
    payload: null,
    loading: false,
    error: null
  })
  let refreshGeneration = 0
  async function refresh() {
    const generation = ++refreshGeneration
    providers.loading = true
    providers.error = null
    try {
      const payload = await backend.axo.providers(host)
      if (generation === refreshGeneration) providers.payload = payload
    } catch (cause) {
      if (generation === refreshGeneration) providers.error = getErrorMessage(cause)
    } finally {
      if (generation === refreshGeneration) providers.loading = false
    }
  }
  let busy = $state(false)
  let error = $state<string | null>(null)
  let keyProvider = $state<string | null>(null)
  let key = $state('')
  let loginProvider = $state<string | null>(null)
  const actionsDisabled = $derived(busy || loginProvider !== null)
  let query = $state('')

  const isReady = (provider: AxoProvider) => provider.status.kind === 'ready'
  const isConfigured = (provider: AxoProvider) => provider.status.kind !== 'needsCredentials'
  const all = $derived(providers.payload?.providers ?? [])
  const configuredCount = $derived(all.filter(isConfigured).length)
  const readyModels = $derived(all.filter(isReady).reduce((sum, provider) => sum + provider.models, 0))
  const matches = $derived.by(() => {
    const needle = query.trim().toLowerCase()
    if (!needle) return all
    return all.filter(
      (provider) => provider.id.includes(needle) || provider.env.some((name) => name.toLowerCase().includes(needle))
    )
  })
  const configured = $derived(matches.filter(isConfigured))
  const unconfigured = $derived(matches.filter((provider) => !isConfigured(provider)))

  onMount(() => {
    const subscription = backend.axo.onModelsChanged(({ server }) => {
      if (server === getSettingsServer()) void refresh()
    })
    void refresh()
    return () => {
      void subscription.then((unsubscribe) => unsubscribe())
    }
  })

  // The group and status dot already say "not configured"; the badge names only a credential source.
  function sourceLabel(provider: AxoProvider): string | null {
    switch (provider.source.kind) {
      case 'config':
        return 'models.jsonc'
      case 'apiKey':
        return 'Stored key'
      case 'oauth':
        return 'Logged in'
      case 'env':
        return 'Environment'
      case 'none':
        return provider.requiresKey ? null : 'No key needed'
    }
  }

  function sourceDetail(provider: AxoProvider): string | null {
    if (provider.source.kind === 'env')
      return `${provider.source.name} is set in the environment. Unset it to stop using it.`
    if (provider.source.kind === 'none' && provider.requiresKey && provider.env.length)
      return `Set ${provider.env.join(' or ')}`
    return null
  }

  async function run(action: () => Promise<unknown>) {
    busy = true
    error = null
    try {
      await action()
    } catch (cause) {
      error = getErrorMessage(cause)
    } finally {
      busy = false
    }
  }

  async function saveKey() {
    if (!keyProvider || !key.trim()) return
    await backend.axo.setProviderKey(keyProvider, key, host)
    await refresh()
    key = ''
    keyProvider = null
  }
</script>

<div class="flex flex-col gap-3 pb-8">
  <div class="flex items-center justify-between gap-4">
    <p class="min-w-0 text-xs text-subtle">
      Built-in catalog plus custom providers from
      <span class="font-mono break-all">{providers.payload?.modelsFile.path ?? 'models.jsonc'}</span>.
    </p>
    <div class="flex shrink-0 gap-2">
      {#if platform.native && host === null}
        <Button size="sm" disabled={busy} onclick={() => run(() => platform.native!.settings.openModelsFile())}
          >Open models.jsonc</Button
        >
      {/if}
      <Button
        size="sm"
        disabled={busy || providers.loading}
        onclick={() =>
          run(async () => {
            await backend.axo.refreshModels(host)
            await refresh()
          })}
      >
        {#if busy}<Spinner />{/if}
        Refresh Models
      </Button>
    </div>
  </div>

  {#if providers.payload?.modelsFile.error}
    <Alert.Root variant="danger">models.jsonc: {providers.payload.modelsFile.error}</Alert.Root>
  {/if}
  {#if (error && keyProvider === null) || providers.error}
    <Alert.Root variant="danger">{error ?? providers.error}</Alert.Root>
  {/if}

  <div class="flex items-center gap-3">
    <div class="flex-1">
      <SearchInput bind:value={query} placeholder="Search providers…" aria-label="Search providers" />
    </div>
    <span class="flex shrink-0 items-center gap-2 text-xs text-muted">
      {#if providers.loading}<Spinner />{/if}
      {#if providers.payload}{all.length} providers · {configuredCount} configured · {readyModels} models{/if}
    </span>
  </div>
</div>

{#snippet card(provider: AxoProvider)}
  {@const live = isReady(provider)}
  {@const detail = sourceDetail(provider)}
  {@const label = sourceLabel(provider)}
  <article class="flex flex-col gap-2.5 rounded-lg border border-edge bg-surface p-3">
    <header class="flex items-center gap-2.5">
      <span
        aria-hidden="true"
        class="grid size-8 shrink-0 place-items-center rounded-md border border-edge bg-raised font-mono text-md uppercase
               {live ? 'text-accent' : 'text-subtle'}">{provider.id[0]}</span
      >
      <div class="flex min-w-0 flex-1 flex-col">
        <span class="truncate text-md font-semibold text-bright" title={provider.id}>{provider.id}</span>
        <span class="text-xs text-muted">{provider.models} {provider.models === 1 ? 'model' : 'models'}</span>
      </div>
      <StatusDot
        tone={live ? 'success' : provider.status.kind === 'error' ? 'danger' : 'idle'}
        label={live ? 'Configured' : provider.status.kind === 'error' ? 'Needs attention' : 'Not configured'}
      />
    </header>
    {#if label}
      <div><Badge>{label}</Badge></div>
    {/if}
    {#if provider.status.kind === 'error'}
      <p class="text-xs text-danger">{provider.status.message}</p>
    {/if}
    {#if detail}
      <p class="truncate font-mono text-xs text-subtle" title={detail}>{detail}</p>
    {/if}
    <div class="mt-auto flex flex-wrap gap-1.5 border-t border-edge pt-2.5">
      {#if provider.login}
        <Button
          size="xs"
          variant={live ? 'default' : 'accent'}
          disabled={actionsDisabled}
          onclick={() => (loginProvider = provider.id)}>Log In</Button
        >
      {/if}
      {#if provider.requiresKey}
        <Button
          size="xs"
          disabled={actionsDisabled}
          onclick={() => {
            keyProvider = provider.id
            key = ''
            error = null
          }}>{provider.source.kind === 'apiKey' ? 'Update Key' : 'Add Key'}</Button
        >
      {/if}
      {#if provider.source.kind === 'apiKey' || provider.source.kind === 'oauth'}
        <Button
          size="xs"
          disabled={actionsDisabled}
          onclick={() =>
            run(async () => {
              await backend.axo.removeProvider(provider.id, host)
              await refresh()
            })}>{provider.source.kind === 'oauth' ? 'Log Out' : 'Remove'}</Button
        >
      {/if}
    </div>
  </article>
{/snippet}

{#snippet group(label: string, list: AxoProvider[])}
  {#if list.length}
    <SettingsSection title={label} description={`${list.length} providers`}>
      <div class="grid grid-cols-[repeat(auto-fill,minmax(14rem,1fr))] gap-3">
        {#each list as provider (provider.id)}{@render card(provider)}{/each}
      </div>
    </SettingsSection>
  {/if}
{/snippet}

<div>
  {@render group('Configured', configured)}
  {@render group('Not Configured', unconfigured)}
  {#if providers.payload && !matches.length}
    <p class="py-6 text-center text-sm text-muted">No providers match “{query.trim()}”.</p>
  {/if}
  {#if providers.payload}
    <p class="pb-5 text-xs text-subtle">
      Remove and Log Out delete stored credentials only. Keys in models.jsonc or the environment remain in effect.
    </p>
  {/if}
</div>

<ApiKeyDialog
  open={keyProvider !== null}
  title={`Set Key · ${keyProvider}`}
  bind:key
  {busy}
  {error}
  onSave={() => void run(saveKey)}
  onClose={() => {
    keyProvider = null
    key = ''
  }}
/>

<ProviderLoginDialog bind:provider={loginProvider} {host} onDone={() => void refresh()} />
