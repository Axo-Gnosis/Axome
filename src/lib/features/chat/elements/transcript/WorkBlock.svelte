<script lang="ts">
  import type { Snippet } from 'svelte'
  import { CircleCheckIcon } from '@hugeicons/core-free-icons'
  import Icon from '$lib/icons/Icon.svelte'
  import { Spinner } from '$lib/components/ui/progress'
  import { DisclosureChevron } from '$lib/components/ui/disclosure-chevron'
  import { formatCount } from '$lib/features/chat/format'
  import { chatShortcut } from '$lib/features/chat/keymap/bindings.svelte'
  import type { BlockView } from '$lib/features/chat/view'
  import { cn } from '$lib/utils/cn'
  import { disclose } from './disclose'
  import { countsLabel, narrationTitle, workCounts, type TranscriptStep, type WorkSegment } from './groupSteps'

  let {
    segment,
    live,
    bare = false,
    open,
    onView,
    render
  }: {
    segment: WorkSegment
    /** The step being worked on: drawn with a spinner. */
    live: boolean
    /** The turn's summary header is this step's toggle; render the body only. */
    bare?: boolean
    open: boolean
    onView: (id: string, view: BlockView) => void
    render: Snippet<[TranscriptStep]>
  } = $props()

  const counts = $derived(workCounts(segment.steps))
  const summary = $derived(countsLabel(counts))
  const narration = $derived(segment.narration?.text ?? null)
  const title = $derived(narration === null ? null : narrationTitle(narration))
  // A one-line narration is fully shown by the wrapped title; a longer one renders in the body.
  const long = $derived(narration !== null && narration.trim().includes('\n'))
  // While the body shows the full narration, the row counts the work instead of repeating its first line.
  const heading = $derived(open && long ? null : title)
  const cycleKey = $derived(chatShortcut('cycleDetails'))
</script>

<div data-slot="work-block" data-view-id={bare ? undefined : segment.id} class="flex min-w-0 flex-col">
  {#if !bare}
    <button
      type="button"
      aria-expanded={open}
      title={cycleKey ? `Toggle step (${cycleKey} cycles all)` : 'Toggle step'}
      onclick={() => onView(segment.id, open ? 'collapsed' : 'preview')}
      class="flex min-h-7 w-full min-w-0 cursor-pointer items-start gap-2 rounded py-1 text-left text-sm text-muted transition-colors hover:text-fg focus-visible:shadow-focus-ring focus-visible:outline-none"
    >
      <span aria-hidden="true" class="flex h-5 w-3.5 shrink-0 items-center justify-center">
        {#if live}<Spinner size={12} class="text-accent" />{:else}<Icon icon={CircleCheckIcon} size={12} />{/if}
      </span>
      <span
        class={cn(
          'min-w-0 flex-1 leading-5',
          heading !== null ? 'text-fg' : 'tabular-nums',
          open ? 'wrap-anywhere' : 'truncate'
        )}>{heading ?? (summary || 'Work')}</span
      >
      {#if heading !== null && counts.calls}
        <span class="shrink-0 font-mono text-xs leading-5 tabular-nums">{formatCount(counts.calls, 'call')}</span>
      {/if}
      {#if counts.failed}
        <span class="shrink-0 font-mono text-xs leading-5 text-danger-bright">{counts.failed} failed</span>
      {/if}
      <span class="flex h-5 shrink-0 items-center"><DisclosureChevron {open} size={12} /></span>
    </button>
  {/if}
  {#if open}
    <div data-disclose transition:disclose={{ duration: 150 }} class={cn('min-w-0 pb-1.5', !bare && 'pt-0.5')}>
      {#if long && segment.narration}
        <div class="pb-2">{@render render(segment.narration)}</div>
      {/if}
      <div class="relative flex min-w-0 flex-col gap-0.5">
        <div aria-hidden="true" class="absolute inset-y-0 left-1.75 w-px bg-edge"></div>
        {#each segment.steps as step (step.id)}{@render render(step)}{/each}
      </div>
    </div>
  {/if}
</div>
