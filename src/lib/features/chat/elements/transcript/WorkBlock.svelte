<script lang="ts">
  import type { Snippet } from 'svelte'
  import { formatCount } from '$lib/features/chat/format'
  import type { BlockView } from '$lib/features/chat/view'
  import { ChevronRightIcon } from '@hugeicons/core-free-icons'
  import Icon from '$lib/icons/Icon.svelte'
  import { cn } from '$lib/utils/cn'
  import type { TranscriptStep, WorkSegment } from './groupSteps'
  import WorkingIndicator from './WorkingIndicator.svelte'

  let {
    segment,
    running,
    duration,
    expansion,
    overrides,
    asking,
    onView,
    render,
    stop
  }: {
    segment: WorkSegment
    running: boolean
    duration: number | null
    expansion: BlockView
    overrides: Record<string, BlockView>
    asking: Set<string>
    onView: (id: string, view: BlockView) => void
    render: Snippet<[TranscriptStep]>
    stop?: Snippet
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
    [calls ? formatCount(calls, 'tool call') : '', thoughts ? formatCount(thoughts, 'thought') : '']
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
  <!-- Keep Stop beside Working; only the disclosure contents may wrap. -->
  <div class="flex min-w-0 items-start gap-3">
    <button
      type="button"
      aria-expanded={open}
      title="Toggle work block (Alt+O cycles all)"
      onclick={() => onView(segment.id, open ? 'collapsed' : 'preview')}
      class={cn(
        'flex max-w-full min-w-0 cursor-pointer flex-wrap items-center gap-x-2 gap-y-1 rounded text-sm text-muted transition-colors hover:text-fg focus-visible:shadow-focus-ring focus-visible:outline-none',
        running && 'flex-1'
      )}
    >
      {#if running}
        <span class="inline-flex min-h-8 shrink-0 items-center gap-2">
          <WorkingIndicator {duration} />
          <Icon icon={ChevronRightIcon} size={14} class={cn('shrink-0 transition-transform', open && 'rotate-90')} />
        </span>
      {/if}
      {#if summary || failed || !running}
        <span class="inline-flex min-w-0 items-center gap-2">
          {#if summary}<span class="tabular-nums">{summary}</span>{/if}
          {#if failed}<span class="whitespace-nowrap text-danger">{failed} failed</span>{/if}
          {#if !running}
            <Icon icon={ChevronRightIcon} size={14} class={cn('shrink-0 transition-transform', open && 'rotate-90')} />
          {/if}
        </span>
      {/if}
    </button>
    {#if running && stop}{@render stop()}{/if}
  </div>
  {#if shown.length}
    <div class={cn('flex min-w-0 flex-col gap-3', open && 'border-l border-edge pl-3')}>
      {#each shown as step (step.id)}{@render render(step)}{/each}
    </div>
  {/if}
</div>
