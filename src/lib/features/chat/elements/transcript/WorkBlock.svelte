<script lang="ts">
  import type { Snippet } from 'svelte'
  import { formatCount } from '$lib/features/chat/format'
  import type { BlockView } from '$lib/features/chat/view'
  import { DisclosureChevron } from '$lib/components/ui/disclosure-chevron'
  import { cn } from '$lib/utils/cn'
  import { disclose } from './disclose'
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
  const { calls, thoughts } = $derived.by(() => {
    const counts = { calls: 0, thoughts: 0 }
    for (const step of segment.steps) {
      if (step.kind === 'tool') {
        counts.calls++
      } else if (step.kind === 'toolGroup') {
        counts.calls += step.calls.length
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
  const askingSteps = $derived(
    segment.steps.filter((step) =>
      step.kind === 'tool'
        ? asking.has(step.callId)
        : step.kind === 'toolGroup' && step.calls.some((call) => asking.has(call.callId))
    )
  )
</script>

<div data-slot="work-block" data-view-id={segment.id} class="flex min-w-0 flex-col">
  <!-- Keep the live header on one line; shrink the summary before Working or Stop. -->
  <div class="flex min-w-0 items-center gap-3">
    <button
      type="button"
      aria-expanded={open}
      title="Toggle work block (Alt+O cycles all)"
      onclick={() => onView(segment.id, open ? 'collapsed' : 'preview')}
      class={cn(
        'flex max-w-full min-w-0 cursor-pointer flex-wrap items-center gap-x-2 gap-y-1 rounded text-sm text-muted transition-colors hover:text-fg focus-visible:shadow-focus-ring focus-visible:outline-none',
        running && 'flex-1 flex-nowrap'
      )}
    >
      {#if running}
        <span class="inline-flex min-h-8 max-w-full shrink-0 items-center gap-2">
          <WorkingIndicator {duration} />
          <DisclosureChevron {open} size={14} />
        </span>
      {/if}
      {#if summary || !running}
        <span class="inline-flex min-w-0 items-center gap-2">
          {#if summary}<span class="truncate tabular-nums" title={summary}>{summary}</span>{/if}
          {#if !running}
            <DisclosureChevron {open} size={14} />
          {/if}
        </span>
      {/if}
    </button>
    {#if running && stop}{@render stop()}{/if}
  </div>
  {#if open}
    <div data-disclose transition:disclose={{ duration: askingSteps.length ? 0 : 150 }} class="min-w-0 pt-3">
      <div class="flex min-w-0 flex-col gap-3 border-l border-edge pl-3">
        {#each segment.steps as step (step.id)}{@render render(step)}{/each}
      </div>
    </div>
  {:else if askingSteps.length}
    <div class="flex min-w-0 flex-col gap-3 pt-3">
      {#each askingSteps as step (step.id)}{@render render(step)}{/each}
    </div>
  {/if}
</div>
