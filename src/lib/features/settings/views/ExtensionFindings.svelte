<script lang="ts">
  import * as Alert from '$lib/components/ui/alert'
  import type { AxoExtensionDiagnostic, AxoProviderCollision } from '$lib/types/backend'

  let {
    collisions,
    diagnostics,
    id = null
  }: {
    collisions: AxoProviderCollision[]
    diagnostics: AxoExtensionDiagnostic[]
    id?: string | null
  } = $props()

  const visibleCollisions = $derived(
    id === null
      ? collisions
      : collisions.filter(({ winner, hidden }) => winner === id || (hidden.type === 'extension' && hidden.id === id))
  )
  const visibleDiagnostics = $derived(id === null ? diagnostics : diagnostics.filter((entry) => entry.id === id))
</script>

{#if visibleCollisions.length}
  <Alert.Root variant="warning">
    <h3 class="mb-1 text-md font-semibold">Provider Collisions</h3>
    <ul class="space-y-1 break-words">
      {#each visibleCollisions as { winner, hidden, provider }}
        <li>
          {#if hidden.type === 'loaded'}{winner}: Hides the existing {provider} provider.
          {:else if hidden.type === 'extension'}{winner} hides {hidden.id} for provider {provider}.
          {:else if hidden.type === 'registered'}{winner} hides the registered {provider} provider.
          {:else}{winner} hides another layer's {provider} provider.{/if}
        </li>
      {/each}
    </ul>
  </Alert.Root>
{/if}
{#if visibleDiagnostics.length}
  <Alert.Root variant="warning">
    <h3 class="mb-1 text-md font-semibold">Diagnostics</h3>
    <ul class="space-y-1 break-words whitespace-pre-wrap">
      {#each visibleDiagnostics as { id: extensionId, message }}<li>{extensionId}: {message}</li>{/each}
    </ul>
  </Alert.Root>
{/if}
