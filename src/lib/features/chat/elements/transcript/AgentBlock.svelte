<script lang="ts">
  import { Button, IconButton, iconButtonVariants } from '$lib/components/ui/button'
  import * as Tooltip from '$lib/components/ui/tooltip'
  import { StatusDot } from '$lib/components/ui/status-dot'
  import { Separator } from '$lib/components/ui/separator'
  import type { ChatController } from '$lib/features/chat/controller.svelte'
  import * as Message from '$lib/features/chat/elements/message'
  import {
    cacheHitPercent,
    formatCost,
    formatDuration,
    formatRetry,
    formatTokens,
    modelName
  } from '$lib/features/chat/format'
  import { compactionTokensAfter, turnDurationMs, type Step, type Turn } from '$lib/features/chat/turns'
  import type { BlockView } from '$lib/features/chat/view'
  import { notify } from '$lib/features/notifications/state.svelte'
  import {
    ArrowDown02Icon,
    ArrowUp02Icon,
    CircleXIcon,
    Clock01Icon,
    CopyIcon,
    DatabaseExportIcon,
    DatabaseImportIcon,
    DollarCircleIcon,
    GitForkIcon,
    InfoIcon,
    SigmaIcon,
    StopIcon,
    TriangleAlertIcon
  } from '@hugeicons/core-free-icons'
  import Icon, { type IconData } from '$lib/icons/Icon.svelte'
  import { copyToClipboard } from '$lib/utils/clipboard'
  import { cn } from '$lib/utils/cn'
  import { getErrorMessage } from '$lib/utils/client-error'
  import ThinkingBlock from './ThinkingBlock.svelte'
  import NoticeBlock from './NoticeBlock.svelte'
  import ToolBlock from './ToolBlock.svelte'
  import ToolReceipt from './ToolReceipt.svelte'
  import SummaryBlock from './SummaryBlock.svelte'
  import ReviewCard from './ReviewCard.svelte'
  import TurnChangesCard from './TurnChangesCard.svelte'
  import ToolGroup from './ToolGroup.svelte'
  import WorkBlock from './WorkBlock.svelte'
  import WorkingIndicator from './WorkingIndicator.svelte'
  import { groupSteps, segmentSteps, type TranscriptStep } from './groupSteps'

  let {
    turn,
    turns,
    turnIndex,
    controller,
    folderPath,
    sessionId,
    canBranch,
    now,
    expansion,
    overrides,
    onView,
    onOpenJob,
    onOpenPlan
  }: {
    turn: Turn
    turns: Turn[]
    turnIndex: number
    controller: ChatController
    folderPath: string
    sessionId: string | null
    canBranch: boolean
    now: number
    expansion: BlockView
    overrides: Record<string, BlockView>
    onView: (stepId: string, view: BlockView) => void
    onOpenJob?: (ref?: string) => void
    onOpenPlan?: () => void
  } = $props()
  const detailView = $derived(expansion === 'full' ? 'full' : 'collapsed')
  const lastAssistant = $derived(turn.assistants.at(-1))
  const duration = $derived(turnDurationMs(turn, now))
  const compactionSteps = $derived(
    turn.steps.filter((step): step is Extract<Step, { kind: 'compaction' }> => step.kind === 'compaction')
  )
  const segments = $derived(segmentSteps(groupSteps(turn.steps.filter((step) => step.kind !== 'compaction'))))
  const asking = $derived(new Set(controller.state?.live.questions.map((questions) => questions.callId)))
  const reviews = $derived(controller.state?.live.reviews ?? [])
  const canReview = $derived(controller.connection === 'live' && !controller.readOnly)
  const retry = $derived(turn.running ? (controller.state?.live.retry ?? null) : null)
  const canStop = $derived(turn.running && controller.connection === 'live' && !controller.readOnly)
  // All visible messages belong to one reply, with one copy/fork/details footer.
  const copyText = $derived(turn.steps.flatMap((step) => (step.kind === 'text' ? [step.text] : [])).join('\n\n'))
  const usage = $derived.by(() => {
    if (!turn.assistants.length) return null
    const sum = { input: 0, output: 0, cacheRead: 0, cacheWrite: 0, totalTokens: 0, cost: 0 }
    let assistantMs = 0
    let hasAssistantMs = false
    for (const entry of turn.assistants) {
      sum.input += entry.body.usage.input
      sum.output += entry.body.usage.output
      sum.cacheRead += entry.body.usage.cacheRead
      sum.cacheWrite += entry.body.usage.cacheWrite
      sum.totalTokens += entry.body.usage.totalTokens
      sum.cost += entry.body.usage.cost
      if (entry.body.durationMs !== null) {
        assistantMs += entry.body.durationMs
        hasAssistantMs = true
      }
    }
    const totalTokens = sum.totalTokens || sum.input + sum.output + sum.cacheRead + sum.cacheWrite
    const rateMs = hasAssistantMs ? assistantMs : duration
    const tokensPerSec =
      sum.output > 0 && rateMs !== null && rateMs > 0 ? Math.round(sum.output / (rateMs / 1000)) : null
    return { ...sum, totalTokens, cacheHitPercent: cacheHitPercent(sum), tokensPerSec }
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

  async function stop() {
    try {
      await controller.stop()
    } catch (error) {
      notify.error('Stop chat failed', getErrorMessage(error))
    }
  }
</script>

{#snippet stat(icon: IconData, label: string, value: string, note?: string)}
  <div class="flex items-center justify-between gap-4">
    <dt class="flex items-center gap-2 text-muted">
      <Icon {icon} size={14} class="shrink-0" />
      <span>{label}</span>
    </dt>
    <dd class="font-mono text-fg tabular-nums">
      {value}{#if note}
        <span class="text-muted">· {note}</span>
      {/if}
    </dd>
  </div>
{/snippet}

{#snippet stopControl()}
  {#if canStop}
    <Button type="button" variant="text" size="xs" class="ms-auto gap-1.5" aria-keyshortcuts="Escape" onclick={stop}>
      <Icon icon={StopIcon} size={12} class="text-danger" />
      Stop
    </Button>
  {/if}
{/snippet}

{#snippet render(step: TranscriptStep)}
  {#if step.kind === 'text'}
    <Message.Response text={step.text} streaming={step.entryId === null} {folderPath} {sessionId} />
  {:else if step.kind === 'thinking'}
    <ThinkingBlock thought={step} view={overrides[step.id] ?? detailView} {onView} {folderPath} {sessionId} />
  {:else if step.kind === 'toolGroup'}
    <ToolGroup
      group={step}
      expansion={detailView}
      {overrides}
      {onView}
      {folderPath}
      {sessionId}
      {controller}
      {onOpenJob}
    />
  {:else if step.kind === 'tool'}
    <ToolBlock
      call={step}
      expansion={detailView}
      {overrides}
      {onView}
      {folderPath}
      {sessionId}
      {controller}
      {onOpenJob}
    />
  {:else if step.kind === 'receipt'}
    {@const review =
      step.call.name === 'yield' ? reviews.find((review) => review.callId === step.call.callId) : undefined}
    <!-- Match `callId` (axo's tool call id), never `id` (the transcript step id). -->
    <ToolReceipt call={step.call} {review} {folderPath} {sessionId} {onOpenJob} {onOpenPlan} />
    {#if review && !review.plan}
      <ReviewCard
        {review}
        {folderPath}
        {sessionId}
        disabled={!canReview}
        onAccept={() => controller.acceptResult(review.callId)}
        onRevise={(feedback) => controller.reviseResult(review.callId, feedback)}
        onDismiss={async () => {
          // dismiss reports its own failure as a notification.
          await controller.dismiss(review.callId)
          return null
        }}
      />
    {/if}
  {:else if step.kind === 'notice'}
    <NoticeBlock notice={step.notice} {folderPath} {sessionId} {onOpenJob} />
  {:else if step.kind === 'branchSummary'}
    <SummaryBlock {step} view={overrides[step.id] ?? detailView} {onView} {folderPath} {sessionId} />
  {:else if step.kind === 'contextMoved' || step.kind === 'agentChange' || step.kind === 'modeChange'}
    <div class="flex items-center gap-2 text-sm text-muted" data-command={step.kind}>
      <Icon icon={InfoIcon} size={14} />
      <span
        >{step.kind === 'agentChange'
          ? `Persona → ${step.agent ?? 'Default'}`
          : step.kind === 'modeChange'
            ? step.mode
              ? `Mode → ${step.mode}`
              : 'Exited mode'
            : `Moved ${step.blocks} ${step.blocks === 1 ? 'block' : 'blocks'} out of context`}</span
      >
    </div>
  {/if}
{/snippet}

<div data-slot="agent-block" class="flex min-w-0 flex-col gap-3 py-2">
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
        stop={stopControl}
      />
    {:else}
      {@render render(segment)}
    {/if}
  {/each}
  {#if turn.running && segments.at(-1)?.kind !== 'work'}
    <div class="flex min-w-0 items-center gap-3">
      <WorkingIndicator {duration} />
      {@render stopControl()}
    </div>
  {/if}
  {#if retry}
    <div class="flex min-w-0 items-center gap-2 text-sm text-warning">
      <StatusDot tone="waiting" size="sm" />
      <span class="shrink-0 tabular-nums">{formatRetry(retry)}</span>
      {#if retry.error}
        <span class="min-w-0 truncate text-muted" title={retry.error}>{retry.error}</span>
      {/if}
    </div>
  {/if}
  {#each notices as notice}
    <div class={cn('flex items-start gap-2 text-sm', notice.danger ? 'text-danger' : 'text-warning')}>
      <Icon icon={notice.danger ? CircleXIcon : TriangleAlertIcon} size={14} class="shrink-0" />
      <span class="whitespace-pre-wrap">{notice.text}</span>
    </div>
  {/each}
  {#if !turn.running}
    <TurnChangesCard {turn} {folderPath} {sessionId} />
    <div class="flex min-h-5 max-w-full items-center gap-1 self-start text-subtle">
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
        <Tooltip.Root>
          <Tooltip.Trigger
            type="button"
            aria-label="Response Details"
            class={cn(iconButtonVariants({ size: 'sm' }), 'shrink-0 text-subtle')}
          >
            <Icon icon={InfoIcon} size={14} />
          </Tooltip.Trigger>
          <Tooltip.Content side="top" align="start" class="w-64 p-3">
            <div class="mb-2 text-xs font-medium tracking-wide text-muted uppercase">Message usage</div>
            <dl class="flex flex-col gap-1.5 text-sm">
              {#if duration !== null}
                {@render stat(Clock01Icon, 'Duration', formatDuration(duration))}
              {/if}
              {#if usage}
                {@render stat(ArrowUp02Icon, 'Input', formatTokens(usage.input))}
                {@render stat(ArrowDown02Icon, 'Output', formatTokens(usage.output))}
                {#if usage.cacheRead > 0}
                  {@render stat(
                    DatabaseExportIcon,
                    'Cache read',
                    formatTokens(usage.cacheRead),
                    usage.cacheHitPercent !== null ? `${usage.cacheHitPercent}% hit` : undefined
                  )}
                {/if}
                {#if usage.cacheWrite > 0}
                  {@render stat(DatabaseImportIcon, 'Cache write', formatTokens(usage.cacheWrite))}
                {/if}
                <Separator class="my-0.5 bg-edge-strong" />
                {@render stat(SigmaIcon, 'Total', formatTokens(usage.totalTokens))}
                {#if usage.tokensPerSec !== null}
                  {@render stat(ArrowDown02Icon, 'Rate', `${usage.tokensPerSec} tok/s avg`)}
                {/if}
                {@render stat(DollarCircleIcon, 'Cost', formatCost(usage.cost))}
              {/if}
            </dl>
          </Tooltip.Content>
        </Tooltip.Root>
      {/if}
      {#if lastAssistant}
        <span class="ml-1 truncate font-mono text-xs text-muted" title={lastAssistant.body.model}
          >{modelName(lastAssistant.body.model)}:{turn.config.thinking ?? 'unknown'}</span
        >
      {/if}
    </div>
  {/if}
  {#each compactionSteps as step (step.id)}
    <SummaryBlock
      {step}
      tokensAfter={compactionTokensAfter(turns, turnIndex, step.id, controller.state?.live.stats.contextTokens)}
      view={overrides[step.id] ?? (detailView === 'full' ? 'preview' : 'collapsed')}
      {onView}
      {folderPath}
      {sessionId}
    />
  {/each}
</div>
