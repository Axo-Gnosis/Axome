<script lang="ts">
  import { Button, IconButton, iconButtonVariants } from '$lib/components/ui/button'
  import * as Tooltip from '$lib/components/ui/tooltip'
  import { Separator } from '$lib/components/ui/separator'
  import * as Alert from '$lib/components/ui/alert'
  import type { ChatController } from '$lib/features/chat/controller.svelte'
  import { chatExtensionTarget, findExtension } from '$lib/features/chat/extensions'
  import { openExtensionSettings } from '$lib/features/settings/dialog/state.svelte'
  import * as Message from '$lib/features/chat/elements/message'
  import { TIER_LABELS } from '$lib/features/chat/elements/model-picker/selector'
  import {
    cacheHitPercent,
    formatCost,
    formatDuration,
    formatList,
    formatTokens,
    modelName
  } from '$lib/features/chat/format'
  import { compactionTokensAfter, turnDurationMs, turnEndNotice, turnSetup, type Turn } from '$lib/features/chat/turns'
  import type { CaptureRun } from '$lib/features/memory/captures.svelte'
  import { isActiveJob, type TranscriptJob } from '$lib/features/chat/jobs'
  import type { BlockView } from '$lib/features/chat/view'
  import { notify } from '$lib/features/notifications/state.svelte'
  import type { ChatUsage } from '$lib/types/backend'
  import {
    ArrowDown02Icon,
    ArrowUp02Icon,
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
  import YieldReceipt from './YieldReceipt.svelte'
  import ReceiptRow from './ReceiptRow.svelte'
  import RetryRow from './RetryRow.svelte'
  import JobCard from './JobCard.svelte'
  import CommandJobRow from './CommandJobRow.svelte'
  import BackgroundBand from './BackgroundBand.svelte'
  import ReviewCard from './ReviewCard.svelte'
  import TurnChangesCard from './TurnChangesCard.svelte'
  import ToolGroup from './ToolGroup.svelte'
  import WorkBlock from './WorkBlock.svelte'
  import WorkingIndicator from './WorkingIndicator.svelte'
  import { groupSteps, segmentSteps, type BandStep, type TranscriptStep } from './groupSteps'

  let {
    turn,
    turns,
    jobs,
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
    onOpenPlan,
    captures = [],
    onOpenMemory
  }: {
    turn: Turn
    turns: Turn[]
    /** The session's job index (`transcriptJobs`). */
    jobs: ReadonlyMap<string, TranscriptJob>
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
    /** Capture runs anchored to this turn's compactions. */
    captures?: CaptureRun[]
    onOpenMemory?: () => void
  } = $props()
  const detailView = $derived(expansion === 'full' ? 'full' : 'collapsed')
  const lastAssistant = $derived(turn.assistants.at(-1))
  const setup = $derived(turnSetup(turns, turnIndex))
  const tierLabel = $derived.by(() => {
    const first = turn.assistants[0]?.body.serviceTier
    if (!first) return null
    if (turn.assistants.some(({ body }) => body.serviceTier !== first)) return 'Mixed tiers'
    return first === 'standard' ? null : TIER_LABELS[first]
  })
  const duration = $derived(turnDurationMs(turn, now))
  const segments = $derived(segmentSteps(groupSteps(turn.steps), jobs))
  // A band that ends a finished turn renders after its footer.
  const tail = $derived.by(() => {
    const last = segments.at(-1)
    return !turn.running && last?.kind === 'band' ? last : null
  })
  const flow = $derived(tail ? segments.slice(0, -1) : segments)
  const maintenance = $derived(turnIndex === turns.length - 1 ? (controller.state?.live.maintenance ?? null) : null)
  const liveJobs = $derived(controller.state?.live.jobs ?? [])
  const reviews = $derived(controller.state?.live.reviews ?? [])
  const canReview = $derived(controller.connection === 'live' && !controller.readOnly)
  const retry = $derived(turn.running ? (controller.state?.live.retry ?? null) : null)
  const canStop = $derived(turn.running && controller.connection === 'live' && !controller.readOnly)
  // All visible messages belong to one reply, with one copy/fork/details footer.
  const copyText = $derived(turn.steps.flatMap((step) => (step.kind === 'text' ? [step.text] : [])).join('\n\n'))
  const usage = $derived.by(() => {
    if (!turn.assistants.length) return null
    const sum: ChatUsage = { input: 0, output: 0, cacheRead: 0, cacheWrite: 0, totalTokens: 0, cost: 0 }
    let assistantMs = 0
    let hasAssistantMs = false
    for (const entry of turn.assistants) {
      sum.input += entry.body.usage.input
      sum.output += entry.body.usage.output
      sum.cacheRead += entry.body.usage.cacheRead
      sum.cacheWrite += entry.body.usage.cacheWrite
      sum.totalTokens += entry.body.usage.totalTokens
      sum.cost = sum.cost === null || entry.body.usage.cost === null ? null : sum.cost + entry.body.usage.cost
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
  const endNotice = $derived(turnEndNotice(turn))
  const failedAssistant = $derived(turn.assistants.findLast(({ body }) => body.stopReason === 'error'))
  const failedProvider = $derived(
    failedAssistant?.body.providerUnavailable ? failedAssistant.body.model.split('/')[0] : null
  )
  const providerOrigin = $derived.by(() => {
    const state = controller.extensions
    if (!failedProvider || !state || state.stale || state.providerError || controller.readOnly) return null
    const provider = state.providers?.providers.find(({ id }) => id === failedProvider)
    return provider?.origin ? { id: provider.origin, scoped: provider.originProject } : null
  })
  const providerExtension = $derived(
    providerOrigin && findExtension(controller.extensions?.payload ?? null, providerOrigin)
  )
  async function retryProvider() {
    const state = controller.extensions
    const origin = providerOrigin
    if (
      !state ||
      state.busy ||
      state.stale ||
      controller.readOnly ||
      controller.connection !== 'live' ||
      !origin ||
      !providerExtension?.enabled
    )
      return
    const ok = await state.retry(origin.id, origin.scoped)
    if (!ok && state.error) notify.error('Retry failed', state.error)
  }
  // Jobs this turn started that outlive a stopped turn.
  const stillRunning = $derived.by(() => {
    const ids = liveJobs
      .filter((job) => isActiveJob(job.status) && jobs.get(job.id)?.turnId === turn.id)
      .map((job) => job.id)
    return ids.length ? ` ${formatList(ids)} ${ids.length === 1 ? 'is' : 'are'} still running.` : ''
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
      {now}
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
      {now}
      {onOpenJob}
    />
  {:else if step.kind === 'job'}
    {#if step.target === 'command'}
      <CommandJobRow {step} {jobs} {liveJobs} {now} view={overrides[step.id] ?? detailView} {onView} {onOpenJob} />
    {:else}
      <JobCard
        {step}
        turnId={turn.id}
        {jobs}
        {liveJobs}
        {now}
        view={overrides[step.id] ?? detailView}
        {overrides}
        {onView}
        {folderPath}
        {sessionId}
        {onOpenJob}
      />
    {/if}
  {:else if step.kind === 'receipt'}
    {#if step.call.name === 'yield'}
      {@const review = reviews.find((review) => review.callId === step.call.callId)}
      <!-- Match `callId` (axo's tool call id), never `id` (the transcript step id). -->
      <YieldReceipt call={step.call} {review} {folderPath} {sessionId} {onOpenPlan} />
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
    {:else}
      <ReceiptRow call={step.call} {onOpenJob} />
    {/if}
  {:else if step.kind === 'notice' && step.notice.kind !== 'jobWake'}
    <NoticeBlock notice={step.notice} timestamp={step.timestamp} {folderPath} {sessionId} />
  {:else if step.kind === 'band'}
    {@render band(step, turn.running && maintenance !== null && step === flow.at(-1))}
  {/if}
{/snippet}

<!-- `live`: the session's running maintenance joins this band. -->
{#snippet band(step: BandStep | null, live: boolean)}
  {@const id = step?.id ?? `band:live:${turn.id}`}
  <BackgroundBand
    {id}
    steps={step?.steps}
    captures={step ? captures.filter((run) => step.steps.some((s) => s.id === run.anchorEntryId)) : []}
    maintenance={live ? maintenance : null}
    contextTokens={live ? (controller.state?.live.stats.contextTokens ?? null) : null}
    tokensAfter={(stepId) =>
      compactionTokensAfter(turns, turnIndex, stepId, controller.state?.live.stats.contextTokens)}
    view={overrides[id] ?? (detailView === 'full' ? 'preview' : 'collapsed')}
    {onView}
    {folderPath}
    {sessionId}
    {onOpenMemory}
  />
{/snippet}

<div data-slot="agent-block" class="flex min-w-0 flex-col gap-3 py-2">
  {#each flow as segment, index (segment.id)}
    {#if segment.kind === 'work'}
      <WorkBlock
        {segment}
        running={turn.running && index === flow.length - 1}
        {duration}
        {liveJobs}
        {expansion}
        {overrides}
        {onView}
        {render}
        stop={stopControl}
      />
    {:else}
      {@render render(segment)}
    {/if}
  {/each}
  {#if turn.running && maintenance && flow.at(-1)?.kind !== 'band'}
    {@render band(null, true)}
  {/if}
  {#if turn.running && segments.at(-1)?.kind !== 'work'}
    <div class="flex min-w-0 items-center gap-3">
      <WorkingIndicator {duration} />
      {@render stopControl()}
    </div>
  {/if}
  {#if retry}<RetryRow {retry} dot />{/if}
  {#if endNotice?.kind === 'stopped'}
    <div class="flex items-center gap-2 text-sm text-muted">
      <span class="size-2 shrink-0 rounded-xs bg-muted"></span>
      <span>Stopped.{stillRunning}</span>
    </div>
  {:else if endNotice?.kind === 'failed'}
    {#if failedProvider}
      <Alert.Root variant="danger">
        <Alert.Title>Request failed: provider {failedProvider} is unavailable</Alert.Title>
        <Alert.Description>
          {#if providerOrigin}
            It comes from {providerOrigin.id}, which {providerExtension?.phase === 'disabled'
              ? 'stopped after a failure.'
              : "isn't running right now."}
          {/if}
          Work so far is kept above.
          <details class="mt-1">
            <summary class="cursor-pointer focus-visible:shadow-focus-ring focus-visible:outline-none"
              >Error details</summary
            >
            <p class="mt-1 wrap-anywhere whitespace-pre-wrap">{failedAssistant?.body.error ?? endNotice.text}</p>
          </details>
        </Alert.Description>
        {#if providerOrigin}
          {@const target = chatExtensionTarget(folderPath, providerOrigin)}
          <div class="flex flex-wrap gap-1">
            <Button variant="text" size="xs" onclick={() => openExtensionSettings(target)}>Open Extensions</Button>
            {#if providerExtension?.enabled && controller.connection === 'live' && !controller.readOnly}
              <Button variant="text" size="xs" disabled={controller.extensions?.busy} onclick={retryProvider}
                >Retry</Button
              >
            {/if}
          </div>
        {/if}
      </Alert.Root>
    {:else}
      <Alert.Root variant="danger">
        <Alert.Title>Turn Failed</Alert.Title>
        <Alert.Description class="whitespace-pre-wrap">{endNotice.text}</Alert.Description>
      </Alert.Root>
    {/if}
  {:else if endNotice?.kind === 'note'}
    <div class="flex items-start gap-2 text-sm text-muted">
      <Icon icon={TriangleAlertIcon} size={14} class="shrink-0" />
      <span class="whitespace-pre-wrap">{endNotice.text}</span>
    </div>
  {/if}
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
          <Tooltip.Content
            side="top"
            align="start"
            class="max-h-[min(70dvh,var(--bits-tooltip-content-available-height,70dvh))] w-80 overflow-y-auto p-3"
          >
            <div class="mb-2 text-xs font-medium tracking-wide text-muted uppercase">Response Details</div>
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
                {@render stat(DollarCircleIcon, 'Estimated total', formatCost(usage.cost))}
              {/if}
            </dl>
            {#if usage}
              <Separator class="my-2 bg-edge-strong" />
              <div class="mb-1 text-xs text-muted">Requests</div>
              <ol class="list-decimal space-y-1.5 ps-4 text-xs marker:text-muted">
                {#each turn.assistants as { id, body } (id)}
                  <li>
                    <div class="flex items-baseline justify-between gap-2">
                      <span class="min-w-0 truncate font-mono text-fg" title={body.model}>
                        {modelName(body.model)} · {body.thinking ?? 'unknown'}
                      </span>
                      <span class="shrink-0 text-muted">{TIER_LABELS[body.serviceTier]}</span>
                      <span class="shrink-0 font-mono tabular-nums">{formatCost(body.usage.cost)}</span>
                    </div>
                  </li>
                {/each}
              </ol>
              <p class="mt-2 text-xs text-muted">Estimates use the tier recorded on each response.</p>
            {/if}
          </Tooltip.Content>
        </Tooltip.Root>
      {/if}
      {#if lastAssistant}
        <span class="ml-1 truncate font-mono text-xs text-muted" title={lastAssistant.body.model}>
          {#if setup.persona}<span class={setup.persona.changed ? 'text-fg' : ''}>{setup.persona.text}</span
            >{' · '}{/if}
          {#if setup.mode}<span class={setup.mode.changed ? 'text-fg' : ''}>{setup.mode.text}</span>{' · '}{/if}
          <span class={setup.modelChanged ? 'text-fg' : ''}>{modelName(lastAssistant.body.model)}</span>{' · '}
          <span class={setup.thinkingChanged ? 'text-fg' : ''}>{lastAssistant.body.thinking ?? 'unknown'}</span
          >{tierLabel ? ` · ${tierLabel}` : ''}
        </span>
      {/if}
    </div>
  {/if}
  {#if tail}
    {@render band(tail, maintenance !== null)}
  {:else if !turn.running && maintenance}
    {@render band(null, true)}
  {/if}
</div>
