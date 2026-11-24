<script lang="ts">
  import CodeBlock from '$lib/components/markdown/CodeBlock.svelte'
  import { Button } from '$lib/components/ui/button'
  import { DisclosureChevron } from '$lib/components/ui/disclosure-chevron'
  import { Spinner } from '$lib/components/ui/progress'
  import * as Message from '$lib/features/chat/elements/message'
  import { formatClock, formatTokens } from '$lib/features/chat/format'
  import type { BlockView } from '$lib/features/chat/view'
  import CaptureFacts from '$lib/features/memory/CaptureFacts.svelte'
  import { countsLabel, factCounts, openModels, type CaptureRun } from '$lib/features/memory/captures.svelte'
  import Icon, { type IconData } from '$lib/icons/Icon.svelte'
  import type { ChatMaintenance } from '$lib/types/backend'
  import { cn } from '$lib/utils/cn'
  import { BrainIcon, GitForkIcon, InfoIcon, UnfoldLessIcon } from '@hugeicons/core-free-icons'
  import { disclose } from './disclose'
  import type { MaintenanceStep } from './groupSteps'

  let {
    id,
    steps = [],
    captures = [],
    maintenance = null,
    contextTokens = null,
    tokensAfter,
    view,
    onView,
    folderPath,
    sessionId,
    onOpenMemory
  }: {
    id: string
    steps?: MaintenanceStep[]
    captures?: CaptureRun[]
    /** Live maintenance joining this band. */
    maintenance?: ChatMaintenance | null
    contextTokens?: number | null
    tokensAfter?: (stepId: string) => number | null
    view: BlockView
    onView: (id: string, view: BlockView) => void
    folderPath: string
    sessionId: string | null
    onOpenMemory?: () => void
  } = $props()

  type Part = { key: string; label: string; icon?: IconData; live?: boolean; danger?: boolean }

  const MAINTENANCE: Record<ChatMaintenance, string> = {
    compact: 'Compacting',
    shake: 'Shaking',
    branchSummary: 'Summarizing branch'
  }

  // Only finished work has anything to open; a band of live maintenance alone is a status line.
  const openable = $derived(steps.length > 0 || captures.length > 0)
  const open = $derived(openable && view !== 'collapsed')
  const afterClosing = $derived(steps.length === 0 && captures.length > 0 && captures.every(isClose))

  function isClose(run: CaptureRun) {
    return run.trigger === 'close'
  }

  function tokens(step: Extract<MaintenanceStep, { kind: 'compaction' }>) {
    const after = tokensAfter?.(step.id) ?? null
    return after === null
      ? formatTokens(step.tokensBefore)
      : `${formatTokens(step.tokensBefore)} → ${formatTokens(after)}`
  }

  function moved(step: Extract<MaintenanceStep, { kind: 'contextMoved' }>) {
    return `Moved ${step.blocks} ${step.blocks === 1 ? 'block' : 'blocks'} out of context`
  }

  function captureLabel(run: CaptureRun): Part {
    const key = `capture:${run.id}`
    if (run.endedAt === null) return { key, label: 'Saving to memory', live: true }
    if (run.error) return { key, label: 'Memory · failed', icon: BrainIcon, danger: true }
    if (isClose(run)) {
      const { added, updated } = factCounts(run)
      return { key, label: `Memory · ${added + updated} saved after closing`, icon: BrainIcon }
    }
    return { key, label: `Memory · ${countsLabel(run, false) || 'nothing new'}`, icon: BrainIcon }
  }

  const parts = $derived.by((): Part[] => {
    const parts: Part[] = steps.map((step) => {
      switch (step.kind) {
        case 'compaction':
          return { key: step.id, label: `Compacted · ${tokens(step)}`, icon: UnfoldLessIcon }
        case 'contextMoved':
          return { key: step.id, label: moved(step), icon: InfoIcon }
        case 'branchSummary':
          return { key: step.id, label: 'Summarized branch', icon: GitForkIcon }
      }
    })
    if (maintenance) {
      const label =
        maintenance === 'compact' && contextTokens
          ? `Compacting · ${formatTokens(contextTokens)} tokens`
          : MAINTENANCE[maintenance]
      parts.push({ key: 'maintenance', label, live: true })
    }
    const closed = captures.find(isClose)
    if (closed) {
      const date = new Date(closed.startedAt).toLocaleString('en-US', {
        month: 'short',
        day: 'numeric',
        hour: '2-digit',
        minute: '2-digit',
        hour12: false
      })
      parts.push({ key: 'closed', label: `Closed ${date}` })
    }
    return [...parts, ...captures.map(captureLabel)]
  })

  const startedAt = $derived(steps[0]?.timestamp ?? captures[0]?.startedAt ?? null)
</script>

{#snippet line()}
  <span class="h-px flex-1 bg-edge transition-colors group-hover:bg-edge-strong"></span>
  <span class="inline-flex min-w-0 shrink items-center gap-1.5 font-mono text-xs text-muted tabular-nums">
    {#each parts as part, index (part.key)}
      {#if index > 0}<span class="text-subtle">·</span>{/if}
      <span
        class={cn(
          'inline-flex shrink-0 items-center gap-1.5',
          part.live && 'text-accent',
          part.danger && 'text-danger-bright'
        )}
      >
        {#if part.live}
          <Spinner size={12} class="shrink-0" />
        {:else if part.icon}
          <Icon icon={part.icon} size={13} class="shrink-0 text-subtle transition-colors group-hover:text-muted" />
        {/if}
        <span>{part.label}</span>
      </span>
    {/each}
    {#if openable}
      <DisclosureChevron {open} size={12} class="text-subtle group-hover:text-muted" />
    {/if}
  </span>
  <span class="h-px flex-1 bg-edge transition-colors group-hover:bg-edge-strong"></span>
{/snippet}

{#snippet head(icon: IconData, title: string, meta: string, danger = false)}
  <div class="flex min-w-0 items-baseline gap-2">
    <Icon {icon} size={14} class={cn('shrink-0 self-center', danger ? 'text-danger-bright' : 'text-muted')} />
    <span class={cn('text-sm', danger ? 'text-danger-bright' : 'text-fg')}>{title}</span>
    {#if meta}<span class="truncate font-mono text-xs text-muted">{meta}</span>{/if}
  </div>
{/snippet}

<div data-slot="background-band" data-view-id={id} data-collapsed={!open} class="flex min-w-0 flex-col gap-2">
  {#if openable}
    <button
      type="button"
      aria-expanded={open}
      aria-label="Background work"
      onclick={() => onView(id, open ? 'collapsed' : 'preview')}
      class="group flex w-full cursor-pointer items-center gap-3 py-1 transition-colors hover:text-fg focus-visible:shadow-focus-ring focus-visible:outline-none"
    >
      {@render line()}
    </button>
  {:else}
    <div class="flex w-full items-center gap-3 py-1">{@render line()}</div>
  {/if}
  {#if open}
    <div
      data-disclose
      transition:disclose
      aria-label="Background work"
      class="flex flex-col overflow-hidden rounded-lg border border-edge bg-surface/55"
    >
      <div class="border-b border-edge px-3 py-2 text-xs text-muted">
        Background work · {afterClosing ? 'after closing' : startedAt ? formatClock(startedAt) : ''} · not part of the conversation
      </div>
      {#each steps as step (step.id)}
        <section class="flex flex-col gap-1.5 border-t border-edge px-3 py-2.5 first-of-type:border-t-0">
          {#if step.kind === 'compaction'}
            {@render head(UnfoldLessIcon, 'Compacted the conversation', `${tokens(step)} tokens`)}
            <CodeBlock
              code={step.summary}
              lang="markdown"
              label="Compaction summary"
              bind:expanded={() => view === 'full', (full) => onView(id, full ? 'full' : 'preview')}
              {folderPath}
              {sessionId}
            />
          {:else if step.kind === 'branchSummary'}
            {@render head(GitForkIcon, 'Summarized the branch', '')}
            <Message.Response text={step.summary} streaming={false} {folderPath} {sessionId} />
          {:else}
            {@render head(InfoIcon, moved(step), '')}
          {/if}
        </section>
      {/each}
      {#each captures as run (run.id)}
        <section class="flex flex-col gap-1.5 border-t border-edge px-3 py-2.5 first-of-type:border-t-0">
          {#if run.error}
            {@render head(BrainIcon, 'Memory couldn’t save', '', true)}
            <p class="text-sm text-muted">{run.error}</p>
          {:else if run.endedAt === null}
            {@render head(BrainIcon, 'Saving to memory', countsLabel(run))}
          {:else}
            {@render head(BrainIcon, 'Saved to memory', countsLabel(run) || 'nothing new')}
          {/if}
          {#if run.facts.length}
            <CaptureFacts {run} hostFolder={folderPath} />
          {/if}
          <div class="flex items-center gap-3 text-xs text-muted">
            <span>Bank {run.bank} · from this chat · {formatClock(run.endedAt ?? run.startedAt)}</span>
            {#if run.noModel}
              <Button variant="text" size="xs" onclick={openModels}>Open Models</Button>
            {/if}
            {#if onOpenMemory}
              <Button variant="text" size="xs" class="ms-auto" onclick={onOpenMemory}>Open Memory Panel</Button>
            {/if}
          </div>
        </section>
      {/each}
    </div>
  {/if}
</div>
