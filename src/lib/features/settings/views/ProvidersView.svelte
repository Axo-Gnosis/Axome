<script lang="ts">
  import { backend } from '$lib/api/backend'
  import * as Alert from '$lib/components/ui/alert'
  import { Badge } from '$lib/components/ui/badge'
  import { Button } from '$lib/components/ui/button'
  import { SearchInput } from '$lib/components/ui/input'
  import { Spinner } from '$lib/components/ui/progress'
  import { StatusDot } from '$lib/components/ui/status-dot'
  import { axoHostResource } from '../state/axo.svelte'
  import { getSettingsHost } from '../state/settings.svelte'
  import { platform } from '$lib/platform'
  import type { AxoProvider, AxoProvidersPayload } from '$lib/types/backend'
  import { getErrorMessage } from '$lib/utils/client-error'
  import ProviderCredentials, {
    credentialReminder,
    isConfigured,
    isReady,
    providerStatus,
    sourceDetail,
    sourceLabel
  } from './ProviderCredentials.svelte'
  import SettingsSection from '../dialog/SettingsSection.svelte'

  const host = getSettingsHost()
  const providers = axoHostResource<AxoProvidersPayload | null>(
    host,
    null,
    (target) => backend.axo.providers(null, target),
    { loadingOnRefresh: true }
  )
  let busy = $state(false)
  let error = $state<string | null>(null)
  let query = $state('')

  const all = $derived(providers.data?.providers ?? [])
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
</script>

<div class="flex flex-col gap-3 pb-8">
  <div class="flex items-center justify-between gap-4">
    <p class="min-w-0 text-xs text-subtle">
      Built-in catalog plus custom providers from
      <span class="font-mono break-all">{providers.data?.modelsFile.path ?? 'models.jsonc'}</span>.
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
            await providers.refresh()
          })}
      >
        {#if busy}<Spinner />{/if}
        Refresh Models
      </Button>
    </div>
  </div>

  {#if providers.data?.modelsFile.error}
    <Alert.Root variant="danger">models.jsonc: {providers.data.modelsFile.error}</Alert.Root>
  {/if}
  {#if error || providers.error}
    <Alert.Root variant="danger">{error ?? providers.error}</Alert.Root>
  {/if}

  <div class="flex items-center gap-3">
    <div class="flex-1">
      <SearchInput bind:value={query} placeholder="Search providers…" aria-label="Search providers" />
    </div>
    <span class="flex shrink-0 items-center gap-2 text-xs text-muted">
      {#if providers.loading}<Spinner />{/if}
      {#if providers.data}{all.length} providers · {configuredCount} configured · {readyModels} models{/if}
    </span>
  </div>
</div>

<ProviderCredentials
  {host}
  scopeFolder={null}
  disabled={busy || providers.loading || providers.error !== null}
  onChanged={() => void providers.refresh()}
>
  {#snippet children(actions)}
    {#snippet card(provider: AxoProvider)}
      {@const live = isReady(provider)}
      {@const status = providerStatus(provider)}
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
          <StatusDot tone={status.tone} label={status.label} />
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
          {@render actions(provider)}
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
      {#if providers.data && !matches.length}
        <p class="py-6 text-center text-sm text-muted">No providers match “{query.trim()}”.</p>
      {/if}
      {#if providers.data}
        <p class="pb-5 text-xs text-subtle">
          {credentialReminder}
        </p>
      {/if}
    </div>
  {/snippet}
</ProviderCredentials>
