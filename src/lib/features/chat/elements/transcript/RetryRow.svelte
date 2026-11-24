<script lang="ts">
  import { StatusDot } from '$lib/components/ui/status-dot'
  import { formatRetry } from '$lib/features/chat/format'
  import type { ChatRetry } from '$lib/types/backend'
  import { cn } from '$lib/utils/cn'

  let { retry, dot = false, class: className }: { retry: ChatRetry; dot?: boolean; class?: string } = $props()
</script>

<div data-slot="retry-row" class={cn('flex min-w-0 items-center gap-2 text-sm text-warning', className)}>
  {#if dot}<StatusDot tone="waiting" size="sm" />{/if}
  <span class="shrink-0 tabular-nums">{formatRetry(retry)}</span>
  {#if retry.error}
    <span class="min-w-0 truncate text-muted" title={retry.error}>{retry.error}</span>
  {/if}
</div>
