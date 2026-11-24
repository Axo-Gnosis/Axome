<script lang="ts">
  import { Chip } from '$lib/components/ui/chip'
  import * as Message from '$lib/features/chat/elements/message'
  import { formatTime } from '$lib/features/chat/format'
  import Icon from '$lib/icons/Icon.svelte'
  import type { ChatNotice } from '$lib/types/backend'
  import { Mail01Icon, TriangleAlertIcon } from '@hugeicons/core-free-icons'

  let {
    notice,
    timestamp,
    folderPath,
    sessionId
  }: {
    /** Job wakes fold into their job's card or row. */
    notice: Exclude<ChatNotice, { kind: 'jobWake' }>
    timestamp: string | null
    folderPath: string
    sessionId: string | null
  } = $props()
  const time = $derived(formatTime(timestamp))
</script>

{#if notice.kind === 'mail'}
  <div
    data-slot="notice-block"
    class="flex min-w-0 flex-col gap-1.5 rounded-lg border border-edge bg-surface px-3 py-2"
  >
    <div class="flex items-center gap-1.5 text-xs text-muted">
      <Icon icon={Mail01Icon} size={14} aria-hidden="true" />
      <span>From</span>
      <Chip tone="accent" class="font-mono">{notice.sender}</Chip>
    </div>
    <Message.Response text={notice.text} streaming={false} {folderPath} {sessionId} />
  </div>
{:else if notice.kind === 'extension'}
  <div data-slot="notice-block" class="flex min-w-0 flex-col gap-1.5">
    <div class="text-xs text-muted">
      <span class="font-mono">{notice.id}</span> · posted{#if time}
        · <time datetime={timestamp ?? undefined} class="font-mono">{time}</time>
      {/if}
    </div>
    {#if notice.text}<Message.Response text={notice.text} streaming={false} {folderPath} {sessionId} />{/if}
  </div>
{:else if notice.kind === 'turnBudget' || notice.kind === 'yieldNudge'}
  <div data-slot="notice-block" class="flex items-center gap-2 text-xs text-warning">
    <Icon icon={TriangleAlertIcon} size={14} class="shrink-0" aria-hidden="true" />
    <span>{notice.text}</span>
  </div>
{:else}
  <div data-slot="notice-block" class="text-sm whitespace-pre-wrap text-muted">{notice.text}</div>
{/if}
