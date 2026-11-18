<script lang="ts">
  import { Button } from '$lib/components/ui/button'
  import * as Message from '$lib/features/chat/elements/message'
  import type { ChatDeliverable } from '$lib/types/backend'
  import { cn } from '$lib/utils/cn'

  let {
    item,
    tone,
    folderPath,
    sessionId
  }: {
    item: Extract<ChatDeliverable, { kind: 'child' }>
    tone: 'note' | 'result'
    folderPath: string
    sessionId: string | null
  } = $props()

  let expanded = $state(false)

  const compact = $derived(item.value == null ? '' : JSON.stringify(item.value))
  const pretty = $derived(item.value == null ? '' : JSON.stringify(item.value, null, 2))
  const trimmed = $derived(item.text.trim())
  const showText = $derived(trimmed.length > 0 && trimmed !== compact && trimmed !== pretty)
  const isLong = $derived(trimmed.split('\n').length > 4 || trimmed.length > 280)
  const color = $derived(item.isError ? 'text-danger-bright' : tone === 'note' ? 'text-fg' : 'text-bright')
  // Markdown sets its own size and color (unlayered CSS), so the tone reaches it with `!` like ThinkingBlock.
  const markdownClass = '[&_.markdown-body]:text-sm! [&_.markdown-body]:text-inherit!'
</script>

<div data-slot="job-result" class={cn('flex min-w-0 flex-col gap-2 text-sm', color)}>
  {#if item.value != null}
    <pre class="m-0 max-h-48 overflow-auto rounded bg-ground p-2 font-mono text-xs text-fg">{pretty}</pre>
  {/if}
  {#if showText}
    {#if !expanded && isLong}
      <div class="line-clamp-4 whitespace-pre-wrap">{trimmed}</div>
      <Button variant="text" size="xs" class="self-start px-0" onclick={() => (expanded = true)}
        >Show full response</Button
      >
    {:else}
      <Message.Response text={trimmed} streaming={false} {folderPath} {sessionId} class={markdownClass} />
      {#if expanded && isLong}
        <Button variant="text" size="xs" class="self-start px-0" onclick={() => (expanded = false)}>Show less</Button>
      {/if}
    {/if}
  {/if}
</div>
