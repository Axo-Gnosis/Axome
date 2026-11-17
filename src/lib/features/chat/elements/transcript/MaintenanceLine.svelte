<script lang="ts">
  import { Spinner } from '$lib/components/ui/progress'
  import { formatRetry } from '$lib/features/chat/format'
  import type { ChatLive, ChatMaintenance } from '$lib/types/backend'

  // Maintenance inside a running turn shows in that turn; this line covers standalone maintenance only.
  let { live, turnRunning }: { live: ChatLive; turnRunning: boolean } = $props()

  const LABELS: Record<ChatMaintenance, string> = {
    compact: 'Compacting',
    shake: 'Shaking',
    branchSummary: 'Summarizing Branch'
  }
</script>

{#if live.maintenance !== null && !turnRunning}
  <div data-slot="maintenance-line" class="flex min-w-0 flex-col gap-1 py-2 text-sm">
    <div class="flex items-center gap-2 text-muted">
      <Spinner />
      <span>{LABELS[live.maintenance]}</span>
    </div>
    {#if live.retry}
      <div class="flex min-w-0 items-center gap-2 text-warning">
        <span class="shrink-0 tabular-nums">{formatRetry(live.retry)}</span>
        {#if live.retry.error}
          <span class="min-w-0 truncate text-muted" title={live.retry.error}>{live.retry.error}</span>
        {/if}
      </div>
    {/if}
  </div>
{/if}
