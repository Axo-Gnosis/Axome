<script lang="ts" module>
  import type { AxoProvider } from '$lib/types/backend'

  export const isReady = (provider: AxoProvider) => provider.status.kind === 'ready'
  export const isConfigured = (provider: AxoProvider) => provider.status.kind !== 'needsCredentials'
  export const credentialReminder =
    'Remove and Log Out delete stored credentials only. Keys in models.jsonc or the environment remain in effect.'

  export function sourceLabel(provider: AxoProvider): string | null {
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

  export function sourceDetail(provider: AxoProvider): string | null {
    if (provider.source.kind === 'env')
      return `${provider.source.name} is set in the environment. Unset it to stop using it.`
    if (provider.source.kind === 'none' && provider.requiresKey && provider.env.length)
      return `Set ${provider.env.join(' or ')}`
    return null
  }
</script>

<script lang="ts">
  import { onDestroy } from 'svelte'
  import { backend } from '$lib/api/backend'
  import { Button } from '$lib/components/ui/button'
  import { notify } from '$lib/features/notifications/state.svelte'
  import { getErrorMessage } from '$lib/utils/client-error'
  import ApiKeyDialog from './ApiKeyDialog.svelte'
  import ProviderLoginDialog from './ProviderLoginDialog.svelte'

  let {
    provider,
    host,
    scopeFolder,
    disabled,
    claimCredentials,
    onChanged
  }: {
    provider: AxoProvider
    host: string | null
    scopeFolder: string | null
    disabled: boolean
    claimCredentials: () => (() => void) | null
    onChanged: () => void
  } = $props()

  let release: (() => void) | null = null
  let keyProvider = $state<string | null>(null)
  let loginProvider = $state<string | null>(null)
  let key = $state('')
  let busy = $state(false)
  let error = $state<string | null>(null)
  const controlsDisabled = $derived(disabled || busy || keyProvider !== null || loginProvider !== null)

  function claim() {
    if (disabled || release) return false
    release = claimCredentials()
    error = null
    return release !== null
  }

  function finish() {
    release?.()
    release = null
  }
  onDestroy(finish)

  function closeKey() {
    if (busy) return
    keyProvider = null
    key = ''
    finish()
  }

  async function saveKey() {
    if (disabled || !keyProvider || !key.trim() || busy) return
    busy = true
    error = null
    try {
      await backend.axo.setProviderKey(keyProvider, key, host, scopeFolder)
      onChanged()
    } catch (cause) {
      error = getErrorMessage(cause)
      return
    } finally {
      busy = false
    }
    closeKey()
  }

  async function remove() {
    if (!claim()) return
    busy = true
    try {
      const warning = await backend.axo.removeProvider(provider.id, host, scopeFolder)
      if (warning !== null) notify.warning('Provider logout warning', warning)
      onChanged()
    } catch (cause) {
      notify.error('Could not remove stored credentials', getErrorMessage(cause))
    } finally {
      busy = false
      finish()
    }
  }
</script>

{#if provider.login}
  <Button
    size="xs"
    variant={isReady(provider) ? 'default' : 'accent'}
    disabled={controlsDisabled}
    onclick={() => {
      if (claim()) loginProvider = provider.id
    }}>Log In</Button
  >
{/if}
{#if provider.requiresKey}
  <Button
    size="xs"
    disabled={controlsDisabled}
    onclick={() => {
      if (claim()) keyProvider = provider.id
    }}
  >
    {provider.source.kind === 'apiKey' ? 'Update Key' : 'Add Key'}
  </Button>
{/if}
{#if provider.source.kind === 'apiKey' || provider.source.kind === 'oauth'}
  <Button size="xs" disabled={controlsDisabled} onclick={remove}
    >{provider.source.kind === 'oauth' ? 'Log Out' : 'Remove'}</Button
  >
{/if}
<ApiKeyDialog
  open={keyProvider !== null}
  title={`Set Key · ${keyProvider ?? provider.id}`}
  bind:key
  {busy}
  {disabled}
  {error}
  onSave={saveKey}
  onClose={closeKey}
/>
<ProviderLoginDialog
  bind:provider={
    () => loginProvider,
    (value) => {
      loginProvider = value
      if (value === null) finish()
    }
  }
  {host}
  {scopeFolder}
  {disabled}
  onDone={onChanged}
/>
