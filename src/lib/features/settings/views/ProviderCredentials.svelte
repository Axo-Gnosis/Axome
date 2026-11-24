<script lang="ts" module>
  import type { StatusTone } from '$lib/components/ui/status-dot'
  import type { AxoProvider } from '$lib/types/backend'

  export const isReady = (provider: AxoProvider) => provider.status.kind === 'ready'
  export const isConfigured = (provider: AxoProvider) => provider.status.kind !== 'needsCredentials'
  export const credentialReminder =
    'Remove and Log Out delete stored credentials only. Keys in models.jsonc or the environment remain in effect.'

  export function providerStatus(provider: AxoProvider): { tone: StatusTone; label: string } {
    if (isReady(provider)) return { tone: 'success', label: 'Configured' }
    if (provider.status.kind === 'error') return { tone: 'danger', label: 'Needs attention' }
    return { tone: 'idle', label: 'Not configured' }
  }

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
  import type { Snippet } from 'svelte'
  import { backend } from '$lib/api/backend'
  import { Button } from '$lib/components/ui/button'
  import { notify } from '$lib/features/notifications/state.svelte'
  import { getErrorMessage } from '$lib/utils/client-error'
  import ApiKeyDialog from './ApiKeyDialog.svelte'
  import ProviderLoginDialog from './ProviderLoginDialog.svelte'

  let {
    host,
    scopeFolder,
    disabled,
    onChanged,
    children
  }: {
    host: string | null
    scopeFolder: string | null
    disabled: boolean
    onChanged: () => void
    children: Snippet<[actions: Snippet<[AxoProvider]>]>
  } = $props()

  let keyProvider = $state<string | null>(null)
  let loginProvider = $state<string | null>(null)
  let key = $state('')
  let busy = $state(false)
  let error = $state<string | null>(null)
  const active = $derived(disabled || busy || keyProvider !== null || loginProvider !== null)

  function closeKey() {
    if (busy) return
    keyProvider = null
    key = ''
    error = null
  }

  async function saveKey() {
    if (disabled || !keyProvider || !key.trim() || busy) return
    busy = true
    error = null
    try {
      await backend.axo.setProviderKey(keyProvider, key, scopeFolder, host)
      onChanged()
    } catch (cause) {
      error = getErrorMessage(cause)
      return
    } finally {
      busy = false
    }
    closeKey()
  }

  async function remove(provider: AxoProvider) {
    if (active) return
    busy = true
    try {
      const warning = await backend.axo.removeProvider(provider.id, scopeFolder, host)
      if (warning !== null) notify.warning('Provider logout warning', warning)
      onChanged()
    } catch (cause) {
      notify.error('Could not remove stored credentials', getErrorMessage(cause))
    } finally {
      busy = false
    }
  }
</script>

{#snippet actions(provider: AxoProvider)}
  {#if provider.login}
    <Button
      size="xs"
      variant={isReady(provider) ? 'default' : 'accent'}
      disabled={active}
      onclick={() => {
        error = null
        loginProvider = provider.id
      }}>Log In</Button
    >
  {/if}
  {#if provider.requiresKey && !provider.oauthOnly}
    <Button
      size="xs"
      disabled={active}
      onclick={() => {
        error = null
        keyProvider = provider.id
      }}
    >
      {provider.source.kind === 'apiKey' ? 'Update Key' : 'Add Key'}
    </Button>
  {/if}
  {#if provider.source.kind === 'apiKey' || provider.source.kind === 'oauth'}
    <Button size="xs" disabled={active} onclick={() => remove(provider)}
      >{provider.source.kind === 'oauth' ? 'Log Out' : 'Remove'}</Button
    >
  {/if}
{/snippet}

{@render children(actions)}

<ApiKeyDialog
  open={keyProvider !== null}
  title={keyProvider ? `Set Key · ${keyProvider}` : 'Set Key'}
  bind:key
  {busy}
  {disabled}
  {error}
  onSave={saveKey}
  onClose={closeKey}
/>
<ProviderLoginDialog bind:provider={loginProvider} {host} {scopeFolder} {disabled} onDone={onChanged} />
