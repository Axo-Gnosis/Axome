<script lang="ts">
  import { StatusDot } from '$lib/components/ui/status-dot'
  import type { ChatDeliverable } from '$lib/types/backend'
  import { cn } from '$lib/utils/cn'
  import { formatCommandExit } from '../tool/bashOutput'
  import ChildDeliverableCard from './ChildDeliverableCard.svelte'

  let {
    items,
    folderPath,
    sessionId,
    onOpenJob
  }: {
    items: ChatDeliverable[]
    folderPath: string
    sessionId: string | null
    onOpenJob?: (ref?: string) => void
  } = $props()
</script>

{#if items.length}
  <div data-slot="deliverables" class="flex min-w-0 flex-col gap-1.5 border-l border-edge pl-4">
    {#each items as item, idx (idx)}
      {#if item.kind === 'child'}
        <ChildDeliverableCard
          {item}
          {folderPath}
          {sessionId}
          onSelect={onOpenJob ? () => onOpenJob(item.jobId) : undefined}
        />
      {:else if item.kind === 'command'}
        {@const exit = item.exit ? formatCommandExit(item.exit) : null}
        <div class="flex min-w-0 items-center gap-2 text-xs text-muted">
          <StatusDot tone={item.isError ? 'danger' : 'success'} size="sm" />
          <span class="shrink-0 font-mono text-fg">{item.jobId}</span>
          <code class="min-w-0 truncate font-mono">{item.command}</code>
          {#if exit}
            <span class={cn('shrink-0', exit.failed ? 'text-danger' : 'text-subtle')}>{exit.label}</span>
          {/if}
        </div>
      {/if}
    {/each}
  </div>
{/if}
