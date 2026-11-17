<script lang="ts">
  import CodeBlock from '$lib/components/markdown/CodeBlock.svelte'
  import { Button } from '$lib/components/ui/button'
  import { DisclosureChevron } from '$lib/components/ui/disclosure-chevron'
  import * as Message from '$lib/features/chat/elements/message'
  import { formatTokens } from '$lib/features/chat/format'
  import type { Step } from '$lib/features/chat/turns'
  import type { BlockView } from '$lib/features/chat/view'
  import Icon from '$lib/icons/Icon.svelte'
  import { GitForkIcon, UnfoldLessIcon } from '@hugeicons/core-free-icons'
  import { disclose } from './disclose'

  let {
    step,
    tokensAfter = null,
    view,
    onView,
    folderPath,
    sessionId
  }: {
    step: Extract<Step, { kind: 'compaction' | 'branchSummary' }>
    tokensAfter?: number | null
    view: BlockView
    onView: (stepId: string, view: BlockView) => void
    folderPath: string
    sessionId: string | null
  } = $props()

  const open = $derived(view !== 'collapsed')
  const compactLabel = $derived(
    step.kind === 'compaction'
      ? tokensAfter !== null
        ? `compacted · ${formatTokens(step.tokensBefore)} → ${formatTokens(tokensAfter)} tokens`
        : `compacted · ${formatTokens(step.tokensBefore)} tokens`
      : ''
  )
</script>

{#if step.kind === 'compaction'}
  <div data-slot="summary-block" data-view-id={step.id} data-collapsed={!open} class="min-w-0">
    <button
      type="button"
      aria-expanded={open}
      onclick={() => onView(step.id, open ? 'collapsed' : 'preview')}
      class="group flex w-full cursor-pointer items-center gap-3 py-1 text-xs text-muted transition-colors hover:text-fg focus-visible:shadow-focus-ring focus-visible:outline-none"
    >
      <span class="h-px flex-1 bg-edge transition-colors group-hover:bg-edge-strong"></span>
      <span class="inline-flex shrink-0 items-center gap-1.5 font-mono tabular-nums">
        <Icon icon={UnfoldLessIcon} size={13} class="shrink-0 text-subtle transition-colors group-hover:text-muted" />
        <span>{compactLabel}</span>
        <DisclosureChevron {open} size={12} class="text-subtle group-hover:text-muted" />
      </span>
      <span class="h-px flex-1 bg-edge transition-colors group-hover:bg-edge-strong"></span>
    </button>
    {#if open}
      <div data-disclose transition:disclose class="mt-2 min-w-0">
        <CodeBlock
          code={step.summary}
          lang="markdown"
          label="Compaction summary"
          bind:expanded={() => view === 'full', (full) => onView(step.id, full ? 'full' : 'preview')}
          {folderPath}
          {sessionId}
        />
      </div>
    {/if}
  </div>
{:else}
  <div data-slot="summary-block" data-view-id={step.id} class="min-w-0">
    <Button
      type="button"
      variant="text"
      size="xs"
      aria-expanded={open}
      class="flex gap-2 px-0 py-1 text-sm text-muted"
      onclick={() => onView(step.id, open ? 'collapsed' : 'preview')}
    >
      <Icon icon={GitForkIcon} size={14} />
      <span>Branch summary</span>
    </Button>
    {#if open}
      <Message.Response text={step.summary} streaming={false} {folderPath} {sessionId} />
    {/if}
  </div>
{/if}
