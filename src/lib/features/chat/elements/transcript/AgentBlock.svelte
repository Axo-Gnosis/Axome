<script lang="ts">
  import { IconButton } from '$lib/components/ui/button'
  import { StatusDot } from '$lib/components/ui/status-dot'
  import type { ChatController } from '$lib/features/chat/controller.svelte'
  import * as Message from '$lib/features/chat/elements/message'
  import { formatCost, formatDuration, formatTokens } from '$lib/features/chat/format'
  import { turnDurationMs, type Turn } from '$lib/features/chat/turns'
  import { modelName } from '$lib/features/sessions/query'
  import type { BlockView } from '$lib/features/chat/view'
  import { TriangleAlertIcon, CircleXIcon, CopyIcon, GitForkIcon, InfoIcon } from '@hugeicons/core-free-icons'
  import Icon from '$lib/icons/Icon.svelte'
  import { copyToClipboard } from '$lib/utils/clipboard'
  import { cn } from '$lib/utils/cn'
  import ThinkingBlock from './ThinkingBlock.svelte'
  import ToolBlock from './ToolBlock.svelte'
  import SummaryBlock from './SummaryBlock.svelte'
  import ToolGroup from './ToolGroup.svelte'
  import WorkBlock from './WorkBlock.svelte'
  import { groupSteps, segmentSteps, type TranscriptStep } from './groupSteps'

  let {
    turn,
    controller,
    folderPath,
    canBranch,
    now,
    expansion = 'collapsed',
    overrides,
    onView,
    onOpenLocation
  }: {
    turn: Turn
    controller: ChatController
    folderPath: string
    canBranch: boolean
    now: number
    expansion?: BlockView
    overrides: Record<string, BlockView>
    onView: (stepId: string, view: BlockView) => void
    onOpenLocation: (path: string, line?: number) => void
  } = $props()

  const detailView = $derived(expansion === 'full' ? 'full' : 'collapsed')
  const lastAssistant = $derived(turn.assistants.at(-1))
  const duration = $derived(turnDurationMs(turn, now))
  const segments = $derived(segmentSteps(groupSteps(turn.steps)))
  const asking = $derived(new Set(controller.state?.live.questions.map((questions) => questions.callId)))
  // All visible messages belong to one reply, with one copy/fork/details footer.
  const copyText = $derived(turn.steps.flatMap((step) => (step.kind === 'text' ? [step.text] : [])).join('\n\n'))
  const usage = $derived.by(() => {
    if (!turn.assistants.length) return ''
    const sum = { input: 0, output: 0, cacheRead: 0, cacheWrite: 0, cost: 0 }
    for (const entry of turn.assistants) {
      sum.input += entry.body.usage.input
      sum.output += entry.body.usage.output
      sum.cacheRead += entry.body.usage.cacheRead
      sum.cacheWrite += entry.body.usage.cacheWrite
      sum.cost += entry.body.usage.cost
    }
    return [
      `${formatTokens(sum.input)} in`,
      `${formatTokens(sum.output)} out`,
      sum.cacheRead ? `${formatTokens(sum.cacheRead)} cache R` : '',
      sum.cacheWrite ? `${formatTokens(sum.cacheWrite)} cache W` : '',
      formatCost(sum.cost)
    ]
      .filter(Boolean)
      .join(' · ')
  })
  const notices = $derived.by(() => {
    const rows: { text: string; danger: boolean }[] = []
    for (const entry of turn.assistants) {
      const body = entry.body
      if (body.stopReason === 'error') rows.push({ text: body.error || 'The response failed.', danger: true })
      else if (body.stopReason === 'length') rows.push({ text: 'Stopped at the output token limit.', danger: false })
      else if (body.stopReason === 'aborted') rows.push({ text: 'The response was aborted.', danger: false })
    }
    const end = turn.end
    if (end && end.outcome !== 'done') {
      const fallback = {
        aborted: 'The turn was aborted.',
        failed: 'The turn failed.',
        interrupted: 'The turn was interrupted.'
      }
      const text = end.reason || fallback[end.outcome]
      if (!rows.some((row) => row.text === text)) rows.push({ text, danger: end.outcome === 'failed' })
    }
    return rows
  })
</script>

{#snippet render(step: TranscriptStep)}
  {#if step.kind === 'text'}
    <Message.Response text={step.text} streaming={step.entryId === null} {folderPath} />
  {:else if step.kind === 'thinking'}
    <ThinkingBlock thought={step} view={overrides[step.id] ?? detailView} {onView} {folderPath} />
  {:else if step.kind === 'toolGroup'}
    <ToolGroup group={step} expansion={detailView} {overrides} {onView} {folderPath} {onOpenLocation} {controller} />
  {:else if step.kind === 'tool'}
    <ToolBlock call={step} expansion={detailView} {overrides} {onView} {folderPath} {onOpenLocation} {controller} />
  {:else if step.kind === 'notice'}
    <div class="text-sm whitespace-pre-wrap text-muted">{step.text}</div>
  {:else if step.kind === 'compaction' || step.kind === 'branchSummary'}
    <SummaryBlock {step} view={overrides[step.id] ?? detailView} {onView} {folderPath} />
  {/if}
{/snippet}

<div data-slot="agent-block" class="group/message flex min-w-0 flex-col gap-3 py-2">
  {#each segments as segment, index (segment.id)}
    {#if segment.kind === 'work'}
      <WorkBlock
        {segment}
        running={turn.running && index === segments.length - 1}
        {duration}
        {expansion}
        {overrides}
        {asking}
        {onView}
        {render}
      />
    {:else}
      {@render render(segment)}
    {/if}
  {/each}
  {#if turn.running && segments.at(-1)?.kind !== 'work'}
    <div class="flex items-center gap-2 text-sm text-muted">
      <StatusDot tone="working" size="sm" />
      <span class="tabular-nums">Working{duration !== null ? ` · ${formatDuration(duration)}` : ''}</span>
    </div>
  {/if}
  {#each notices as notice}
    <div class={cn('flex items-start gap-2 text-sm', notice.danger ? 'text-danger' : 'text-warning')}>
      <Icon icon={notice.danger ? CircleXIcon : TriangleAlertIcon} size={14} class="shrink-0" />
      <span class="whitespace-pre-wrap">{notice.text}</span>
    </div>
  {/each}
  {#if !turn.running}
    <div
      class="flex min-h-5 max-w-full items-center gap-1 self-start text-subtle transition-opacity [@media(hover:hover)]:opacity-0 [@media(hover:hover)]:group-focus-within/message:opacity-100 [@media(hover:hover)]:group-hover/message:opacity-100"
    >
      {#if lastAssistant}
        <IconButton tooltip="Copy" class="shrink-0 text-subtle" onclick={() => void copyToClipboard(copyText)}
          ><Icon icon={CopyIcon} size={14} /></IconButton
        >
        <IconButton
          tooltip="Fork"
          class="shrink-0 text-subtle"
          disabled={!canBranch}
          onclick={() => void controller.fork(lastAssistant.id)}><Icon icon={GitForkIcon} size={14} /></IconButton
        >
      {/if}
      {#if duration !== null || usage}
        <IconButton
          ariaLabel="Response Details"
          tooltip={[duration === null ? null : formatDuration(duration), usage].filter(Boolean).join(' · ')}
          tooltipSide="top"
          class="shrink-0 text-subtle"
        >
          <Icon icon={InfoIcon} size={14} />
        </IconButton>
      {/if}
      {#if lastAssistant}
        <span class="ml-1 truncate font-mono text-xs text-muted" title={lastAssistant.body.model}
          >{modelName(lastAssistant.body.model)}:{turn.config.thinking ?? 'unknown'}</span
        >
      {/if}
    </div>
  {/if}
</div>
