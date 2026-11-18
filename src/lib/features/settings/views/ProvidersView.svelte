<script lang="ts">
  import { backend } from '$lib/api/backend'
  import * as Alert from '$lib/components/ui/alert'
  import { Badge } from '$lib/components/ui/badge'
  import { Button } from '$lib/components/ui/button'
  import * as Dialog from '$lib/components/ui/dialog'
  import { Input, SearchInput } from '$lib/components/ui/input'
  import { Spinner } from '$lib/components/ui/progress'
  import { StatusDot } from '$lib/components/ui/status-dot'
  import { reloadChatModels } from '$lib/features/chat/models.svelte'
  import { getSettingsHost } from '../state/settings.svelte'
  import { platform } from '$lib/platform'
  import type { AxoLoginStarted, AxoProvider, AxoProvidersPayload } from '$lib/types/backend'
  import { getErrorMessage } from '$lib/utils/client-error'
  import { onMount, onDestroy } from 'svelte'
  import ApiKeyDialog from './ApiKeyDialog.svelte'
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
  const changed = () => Promise.all([refresh(), reloadChatModels(host)])
  let busy = $state(false)
  let error = $state<string | null>(null)
  let keyProvider = $state<string | null>(null)
  let key = $state('')
  let loginOpen = $state(false)
  let login = $state<AxoLoginStarted | null>(null)
  let loginError = $state<string | null>(null)
  let redirect = $state('')
  let submitted = $state(false)
  let loginGeneration = 0
  let loginTimer: ReturnType<typeof setTimeout> | undefined
  let query = $state('')

  const isReady = (provider: AxoProvider) => provider.source.kind !== 'none' || !provider.requiresKey
  const all = $derived(providers.payload?.providers ?? [])
  const readyCount = $derived(all.filter(isReady).length)
  const readyModels = $derived(all.filter(isReady).reduce((sum, provider) => sum + provider.models, 0))
  const matches = $derived.by(() => {
    const needle = query.trim().toLowerCase()
    if (!needle) return all
    return all.filter(
      (provider) => provider.id.includes(needle) || provider.env.some((name) => name.toLowerCase().includes(needle))
    )
  })
  const ready = $derived(matches.filter(isReady))
  const unconfigured = $derived(matches.filter((provider) => !isReady(provider)))

  onMount(() => {
    void refresh()
  })
  onDestroy(() => {
    void cancelLogin()
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
    if (provider.source.kind === 'env') return provider.source.name
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
    await changed()
    key = ''
    keyProvider = null
  }

  async function cancelLogin() {
    loginGeneration++
    clearTimeout(loginTimer)
    const current = login
    login = null
    loginOpen = false
    if (current) {
      try {
        await backend.axo.loginCancel(current.loginId, host)
      } catch (cause) {
        error = getErrorMessage(cause)
      }
    }
  }

  async function startLogin(provider: string) {
    const generation = ++loginGeneration
    loginOpen = true
    loginError = null
    redirect = ''
    submitted = false
    try {
      const started = await backend.axo.loginStart(provider, host)
      if (generation !== loginGeneration) {
        await backend.axo.loginCancel(started.loginId, host)
        return
      }
      login = started
      async function poll() {
        try {
          const status = await backend.axo.loginStatus(started.loginId, host)
          if (generation !== loginGeneration) return
          if (status.state === 'done') {
            login = null
            loginOpen = false
          } else if (status.state === 'failed') {
            login = null
            loginError = status.message
          }
          if (status.state === 'pending') loginTimer = setTimeout(() => void poll(), 1000)
          else await changed()
        } catch (cause) {
          if (generation === loginGeneration) loginError = getErrorMessage(cause)
        }
      }
      loginTimer = setTimeout(() => void poll(), 1000)
      await platform.links.openExternal(started.url)
    } catch (cause) {
      if (generation === loginGeneration) loginError = getErrorMessage(cause)
    }
  }

  async function submitLogin() {
    if (!login || !redirect.trim() || submitted) return
    try {
      await backend.axo.loginSubmit(login.loginId, redirect.trim(), host)
      submitted = true
    } catch (cause) {
      loginError = getErrorMessage(cause)
    }
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
            await backend.axo.reloadModels(host)
            await changed()
          })}>Reload Models</Button
      >
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
      {#if providers.payload}{all.length} providers · {readyCount} configured · {readyModels} models{/if}
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
      <StatusDot tone={live ? 'success' : 'idle'} label={live ? 'Configured' : 'Not configured'} />
    </header>
    {#if label || provider.oauth}
      <div class="flex flex-wrap gap-1.5">
        {#if label}<Badge>{label}</Badge>{/if}
        {#if provider.oauth}<Badge>OAuth</Badge>{/if}
      </div>
    {/if}
    {#if detail}
      <p class="truncate font-mono text-xs text-subtle" title={detail}>{detail}</p>
    {/if}
    <div class="mt-auto flex flex-wrap gap-1.5 border-t border-edge pt-2.5">
      {#if provider.oauth}
        {#if provider.source.kind === 'oauth'}
          <Button
            size="xs"
            disabled={busy || loginOpen}
            onclick={() =>
              run(async () => {
                await backend.axo.removeProvider(provider.id, host)
                await changed()
              })}>Log Out</Button
          >
        {:else}
          <Button
            size="xs"
            variant={live ? 'default' : 'accent'}
            disabled={busy || loginOpen}
            onclick={() => startLogin(provider.id)}>Log In</Button
          >
        {/if}
      {/if}
      {#if provider.requiresKey}
        <Button
          size="xs"
          disabled={busy || loginOpen}
          onclick={() => {
            keyProvider = provider.id
            key = ''
            error = null
          }}>{provider.source.kind === 'apiKey' ? 'Update Key' : 'Add Key'}</Button
        >
      {/if}
      {#if provider.source.kind === 'apiKey'}
        <Button
          size="xs"
          disabled={busy || loginOpen}
          onclick={() =>
            run(async () => {
              await backend.axo.removeProvider(provider.id, host)
              await changed()
            })}>Remove</Button
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
  {@render group('Configured', ready)}
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

<Dialog.Root
  open={loginOpen}
  onOpenChange={(open) => {
    if (!open) void cancelLogin()
  }}
>
  <Dialog.Content onModalClose={() => void cancelLogin()}>
    <Dialog.Header>
      <Dialog.Title>Provider Login</Dialog.Title>
      <Dialog.Description>Complete login in the browser, or paste the redirect URL below.</Dialog.Description>
    </Dialog.Header>
    <div class="flex flex-col gap-3">
      {#if login}
        <label class="flex flex-col gap-2 text-sm text-muted">Login URL<Input value={login.url} readonly /></label>
        <div class="flex gap-2">
          <Button size="sm" onclick={() => run(() => platform.links.openExternal(login!.url))}>Open Login</Button>
          <Button size="sm" onclick={() => run(() => platform.clipboard.writeText(login!.url))}>Copy Link</Button>
        </div>
        <form
          class="flex flex-col gap-3"
          onsubmit={(event) => {
            event.preventDefault()
            void submitLogin()
          }}
        >
          <label class="flex flex-col gap-2 text-sm text-muted"
            >Paste redirect URL<Input bind:value={redirect} disabled={submitted} /></label
          >
          <Button type="submit" disabled={submitted || !redirect.trim()}>Submit</Button>
        </form>
        <p class="flex items-center gap-2 text-xs text-muted"><Spinner />Waiting for login…</p>
      {:else if !loginError}<Spinner label="Starting login" />{/if}
      {#if loginError}<p role="alert" class="text-sm text-danger">{loginError}</p>{/if}
      <Dialog.Footer><Button onclick={() => cancelLogin()}>Cancel</Button></Dialog.Footer>
    </div>
  </Dialog.Content>
</Dialog.Root>
