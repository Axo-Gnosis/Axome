<!--
  @component
  CommandJobRow — every event of one background command (`bg_N`) within a work block, as one rail row.
-->

<script lang="ts">
  import { Button } from '$lib/components/ui/button'
  import { StatusDot } from '$lib/components/ui/status-dot'
  import { formatDuration } from '$lib/features/chat/format'
  import { isActiveJob, jobDurationMs, jobState, type TranscriptJob } from '$lib/features/chat/jobs'
  import type { BlockView } from '$lib/features/chat/view'
  import type { ChatJob } from '$lib/types/backend'
  import { cn } from '$lib/utils/cn'
  import { TerminalIcon } from '@hugeicons/core-free-icons'
  import BashCommand from '../tool/BashCommand.svelte'
  import { formatCommandExit } from '../tool/bashOutput'
  import { record } from '../tool/payload'
  import type { JobStep } from './groupSteps'
  import RailRow from './RailRow.svelte'

  let {
    step,
    jobs,
    liveJobs,
    now,
    view,
    onView,
    onOpenJob
  }: {
    step: JobStep
    jobs: ReadonlyMap<string, TranscriptJob>
    liveJobs: ChatJob[]
    now: number
    view: BlockView
    onView: (id: string, view: BlockView) => void
    onOpenJob?: (ref?: string) => void
  } = $props()

  const index = $derived(jobs.get(step.job))
  const live = $derived(liveJobs.find((job) => job.id === step.job))
  const liveCommand = $derived(live?.kind.kind === 'command' ? live.kind : null)
  const active = $derived(live !== undefined && isActiveJob(live.status))
  const state = $derived(jobState(index, live))
  const spawn = $derived(step.events.find((event) => event.kind === 'spawn'))
  const delivered = $derived(
    step.events.flatMap((event) => (event.kind === 'deliverable' && event.item.kind === 'command' ? [event.item] : []))
  )
  const exit = $derived(delivered.at(-1)?.exit ?? index?.exit ?? liveCommand?.exit ?? null)
  const exitLabel = $derived(exit ? formatCommandExit(exit) : null)
  const exitFailed = $derived(exit !== null && exit.type !== 'killed' && exitLabel?.failed === true)
  const verb = $derived(
    spawn
      ? active
        ? 'Running in background'
        : 'Ran in background'
      : step.events.some((event) => event.kind === 'kill')
        ? 'Stopped'
        : 'Finished in background'
  )
  const command = $derived.by(() => {
    const spawned = spawn ? record(spawn.call.arguments).command : undefined
    return (typeof spawned === 'string' && spawned) || index?.title || delivered.at(-1)?.command || ''
  })
  const duration = $derived(jobDurationMs(index, live, now))
  const open = $derived(view !== 'collapsed')
</script>

{#snippet target()}<BashCommand {command} inline />{/snippet}

{#snippet aside()}
  <StatusDot tone={state.tone} size="sm" />
  {#if exitLabel}
    <span class={cn(exitFailed && 'text-danger-bright')}>{exitLabel.label}</span>
  {/if}
  {#if duration !== null}<span class="tabular-nums">{formatDuration(duration)}</span>{/if}
  {#if onOpenJob}
    <Button
      variant="text"
      size="xs"
      class="p-0 font-mono text-xs font-normal"
      onclick={(event) => {
        event.stopPropagation()
        onOpenJob(step.job)
      }}>{step.job}</Button
    >
  {:else}
    <span>{step.job}</span>
  {/if}
{/snippet}

<RailRow
  icon={TerminalIcon}
  tone={active ? 'working' : exitFailed ? 'danger' : 'idle'}
  viewId={step.id}
  {verb}
  {open}
  label={`${open ? 'Collapse' : 'Expand'} ${step.job}`}
  onToggle={() => onView(step.id, open ? 'collapsed' : 'preview')}
  {target}
  {aside}
>
  <div class="px-3 py-2"><BashCommand {command} /></div>
  <div class="flex items-center justify-between bg-ground px-3 py-2 text-muted">
    <span>Output is kept with the job.</span>
    {#if onOpenJob}
      <Button variant="text" size="xs" onclick={() => onOpenJob(step.job)}>Open in Jobs</Button>
    {/if}
  </div>
</RailRow>
