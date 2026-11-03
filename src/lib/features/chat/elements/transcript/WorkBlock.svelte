<script lang="ts">
  import type { Snippet } from 'svelte'
  import { StatusDot } from '$lib/components/ui/status-dot'
  import { formatCount, formatDuration } from '$lib/features/chat/format'
  import type { BlockView } from '$lib/features/chat/view'
  import { ChevronRightIcon } from '@hugeicons/core-free-icons'
  import Icon from '$lib/icons/Icon.svelte'
  import { cn } from '$lib/utils/cn'
  import type { TranscriptStep, WorkSegment } from './groupSteps'

  let {
    segment,
    running,
    duration,
    expansion,
    overrides,
    asking,
    onView,
    render
  }: {
    segment: WorkSegment
    running: boolean
    duration: number | null
    expansion: BlockView
    overrides: Record<string, BlockView>
    asking: Set<string>
    onView: (id: string, view: BlockView) => void
    render: Snippet<[TranscriptStep]>
  } = $props()

  const open = $derived((overrides[segment.id] ?? expansion) !== 'collapsed')
  const { calls, failed, thoughts } = $derived.by(() => {
    const counts = { calls: 0, failed: 0, thoughts: 0 }
    for (const step of segment.steps) {
      if (step.kind === 'tool') {
        counts.calls++
        if (step.status === 'error') counts.failed++
      } else if (step.kind === 'toolGroup') {
        counts.calls += step.calls.length
        for (const call of step.calls) {
          if (call.status === 'error') counts.failed++
        }
      } else if (step.kind === 'thinking') {
        counts.thoughts++
      }
    }
    return counts
  })
  const summary = $derived(
    [
      running ? 'Working' : '',
      running && duration !== null ? formatDuration(duration) : '',
      calls ? formatCount(calls, 'tool call') : '',
      thoughts ? formatCount(thoughts, 'thought') : ''
    ]
      .filter(Boolean)
      .join(' · ')
  )
  // Questions must remain answerable even when the surrounding work is folded.
  const shown = $derived(
    open
      ? segment.steps
      : segment.steps.filter((step) =>
          step.kind === 'tool'
            ? asking.has(step.callId)
            : step.kind === 'toolGroup' && step.calls.some((call) => asking.has(call.callId))
        )
  )
</script>

<div data-slot="work-block" data-view-id={segment.id} class="flex min-w-0 flex-col gap-3">
  <button
    type="button"
    aria-expanded={open}
    onclick={() => onView(segment.id, open ? 'collapsed' : 'preview')}
    class="flex cursor-pointer items-center gap-2 self-start rounded text-sm text-muted transition-colors hover:text-fg focus-visible:shadow-focus-ring focus-visible:outline-none"
  >
    {#if running}<StatusDot tone="working" size="sm" />{/if}
    <span class="tabular-nums">{summary}</span>
    {#if failed}<span class="text-danger">{failed} failed</span>{/if}
    <Icon icon={ChevronRightIcon} size={14} class={cn('transition-transform', open && 'rotate-90')} />
  </button>
  {#if shown.length}
    <div class={cn('flex min-w-0 flex-col gap-3', open && 'border-l border-edge pl-3')}>
      {#each shown as step (step.id)}{@render render(step)}{/each}
    </div>
  {/if}
</div>
