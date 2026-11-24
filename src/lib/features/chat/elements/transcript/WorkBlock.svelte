<script lang="ts">
  import type { Snippet } from 'svelte'
  import { formatCount, formatList } from '$lib/features/chat/format'
  import { isActiveJob } from '$lib/features/chat/jobs'
  import type { BlockView } from '$lib/features/chat/view'
  import type { ChatJob } from '$lib/types/backend'
  import { DisclosureChevron } from '$lib/components/ui/disclosure-chevron'
  import { cn } from '$lib/utils/cn'
  import { disclose } from './disclose'
  import type { TranscriptStep, WorkSegment } from './groupSteps'
  import WorkingIndicator from './WorkingIndicator.svelte'

  let {
    segment,
    running,
    duration,
    liveJobs,
    expansion,
    overrides,
    onView,
    render,
    stop
  }: {
    segment: WorkSegment
    running: boolean
    /** The turn's duration, for the live header. */
    duration: number | null
    liveJobs: ChatJob[]
    expansion: BlockView
    overrides: Record<string, BlockView>
    onView: (id: string, view: BlockView) => void
    render: Snippet<[TranscriptStep]>
    stop?: Snippet
  } = $props()

  // A running block stays open until it folds; a manual choice in `overrides` still wins.
  const open = $derived((overrides[segment.id] ?? (running ? 'preview' : expansion)) !== 'collapsed')
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
  const waiting = $derived.by(() => {
    if (
      !segment.steps.some(
        (step) => step.kind === 'receipt' && step.call.name === 'wait' && step.call.status === 'running'
      )
    )
      return ''
    const ids = liveJobs.filter((job) => isActiveJob(job.status)).map((job) => job.id)
    return ids.length ? `waiting on ${formatList(ids)}` : 'waiting'
  })
</script>

{#snippet rail(steps: TranscriptStep[])}
  <div class="relative flex min-w-0 flex-col gap-0.5">
    <div aria-hidden="true" class="absolute inset-y-0 left-1.75 w-px bg-edge"></div>
    {#each steps as step (step.id)}{@render render(step)}{/each}
  </div>
{/snippet}

<div data-slot="work-block" data-view-id={segment.id} class="flex min-w-0 flex-col">
  <!-- Keep the live header on one line; shrink the suffix before Working or Stop. -->
  <div class="flex min-w-0 items-center gap-3">
    <button
      type="button"
      aria-expanded={open}
      title="Toggle work block (Alt+O cycles all)"
      onclick={() => onView(segment.id, open ? 'collapsed' : 'preview')}
      class={cn(
        'flex max-w-full min-w-0 cursor-pointer items-center gap-2 rounded text-sm text-muted transition-colors hover:text-fg focus-visible:shadow-focus-ring focus-visible:outline-none',
        running && 'min-h-8 flex-1'
      )}
    >
      {#if running}
        <WorkingIndicator {duration} />
        {#if waiting}<span class="min-w-0 truncate">{waiting}</span>{/if}
        <DisclosureChevron {open} size={14} class="shrink-0" />
      {:else}
        {#if summary}<span class="min-w-0 truncate tabular-nums" title={summary}>{summary}</span>{/if}
        <DisclosureChevron {open} size={14} class="shrink-0" />
      {/if}
    </button>
    {#if running && stop}{@render stop()}{/if}
  </div>
  {#if open}
    <div data-disclose transition:disclose={{ duration: 150 }} class="min-w-0 pt-1.5">
      {@render rail(segment.steps)}
    </div>
  {/if}
</div>
