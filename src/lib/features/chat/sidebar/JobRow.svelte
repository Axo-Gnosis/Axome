<script lang="ts">
  import { IconButton } from '$lib/components/ui/button'
  import { isActiveJob } from '$lib/features/chat/jobs'
  import Icon from '$lib/icons/Icon.svelte'
  import type { ChatJob } from '$lib/types/backend'
  import { cn } from '$lib/utils/cn'
  import { StopIcon } from '@hugeicons/core-free-icons'
  import type { Snippet } from 'svelte'

  let {
    job,
    selected,
    class: className,
    onSelect,
    onStop,
    children
  }: {
    job: ChatJob
    selected: boolean
    class?: string
    onSelect: () => void
    onStop: () => void
    children: Snippet
  } = $props()

  const active = $derived(isActiveJob(job.status))
</script>

<div
  data-slot="job-row"
  class={cn(
    'flex items-center gap-1 border-b border-edge/30',
    selected ? 'bg-accent-bg text-bright' : 'hover:bg-surface',
    active && 'pr-1.5'
  )}
>
  <button
    type="button"
    aria-current={selected}
    class={cn(
      'flex min-w-0 flex-1 cursor-pointer py-1.5 text-left focus-visible:shadow-focus-ring focus-visible:outline-none',
      className
    )}
    onclick={onSelect}
  >
    {@render children()}
  </button>
  {#if active}
    <IconButton tooltip="Stop" size="sm" class="shrink-0" onclick={onStop}>
      <Icon icon={StopIcon} size={12} />
    </IconButton>
  {/if}
</div>
