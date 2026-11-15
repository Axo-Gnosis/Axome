<script lang="ts">
  import { Badge } from '$lib/components/ui/badge'
  import { Button } from '$lib/components/ui/button'
  import { Chip } from '$lib/components/ui/chip'
  import { StatusDot } from '$lib/components/ui/status-dot'
  import * as Message from '$lib/features/chat/elements/message'
  import type { ChatDeliverable } from '$lib/types/backend'

  let {
    item,
    folderPath,
    sessionId,
    onSelect
  }: {
    item: Extract<ChatDeliverable, { kind: 'child' }>
    folderPath: string
    sessionId: string | null
    onSelect?: () => void
  } = $props()

  let expanded = $state(false)

  const compact = $derived(item.value == null ? '' : JSON.stringify(item.value))
  const pretty = $derived(item.value == null ? '' : JSON.stringify(item.value, null, 2))
  const trimmed = $derived(item.text.trim())
  const showText = $derived(trimmed.length > 0 && trimmed !== compact && trimmed !== pretty)
  const isLong = $derived(trimmed.split('\n').length > 4 || trimmed.length > 280)
</script>

<div
  data-slot="child-deliverable-card"
  class="flex min-w-0 flex-col gap-2 rounded-lg border border-edge bg-surface px-3 py-2 text-sm"
>
  <div class="flex flex-wrap items-center gap-2">
    <StatusDot tone={item.isError ? 'danger' : item.note ? 'waiting' : 'success'} size="sm" />
    <span class="font-mono text-xs text-fg">{item.team ? `${item.team.member} (${item.jobId})` : item.jobId}</span>
    {#if item.agent}
      <Chip class="font-mono text-xs">{item.agent}</Chip>
    {/if}
    <Badge variant={item.isError ? 'danger' : item.note ? 'default' : 'accent'}>
      {item.note ? 'Note' : item.partial ? 'Partial Result' : 'Result'}
    </Badge>
    {#if onSelect}
      <Button variant="text" size="xs" class="ms-auto px-0" onclick={onSelect}>View Agent</Button>
    {/if}
  </div>
  {#if item.value != null}
    <pre class="m-0 max-h-48 overflow-auto rounded bg-ground p-2 font-mono text-xs text-fg">{pretty}</pre>
  {/if}
  {#if showText}
    {#if !expanded && isLong}
      <div class="line-clamp-4 text-sm whitespace-pre-wrap text-muted">{trimmed}</div>
      <Button variant="text" size="xs" class="self-start px-0" onclick={() => (expanded = true)}
        >Show full response</Button
      >
    {:else}
      <Message.Response text={trimmed} streaming={false} {folderPath} {sessionId} />
      {#if expanded && isLong}
        <Button variant="text" size="xs" class="self-start px-0" onclick={() => (expanded = false)}>Show less</Button>
      {/if}
    {/if}
  {/if}
</div>
