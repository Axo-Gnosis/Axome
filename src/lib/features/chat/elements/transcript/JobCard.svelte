<!--
  @component
  JobCard — every event of one subagent or team within a step, as one card on the rail.
  Folded: header plus one live or result line. Open: the events in order, then the result.
  A team shows its team-wide events, then one section per member.
-->

<script lang="ts">
  import { Button } from '$lib/components/ui/button'
  import { Chip } from '$lib/components/ui/chip'
  import { DisclosureChevron } from '$lib/components/ui/disclosure-chevron'
  import { Spinner } from '$lib/components/ui/progress'
  import { StatusDot, statusToneText, type StatusTone } from '$lib/components/ui/status-dot'
  import { firstLine, formatDuration } from '$lib/features/chat/format'
  import { isActiveJob, jobDurationMs, jobState, type TranscriptJob } from '$lib/features/chat/jobs'
  import type { BlockView } from '$lib/features/chat/view'
  import type { ChatChildProgress, ChatDeliverable, ChatJob } from '$lib/types/backend'
  import { cn } from '$lib/utils/cn'
  import { UserGroupIcon } from '@hugeicons/core-free-icons'
  import { record } from '../tool/payload'
  import { spawnReceipt } from '../tool/receipt'
  import { disclose } from './disclose'
  import type { JobEvent, JobStep } from './groupSteps'
  import JobResult from './JobResult.svelte'
  import RailItem from './RailItem.svelte'

  let {
    step,
    turnId,
    jobs,
    liveJobs,
    now,
    view,
    overrides,
    onView,
    folderPath,
    sessionId,
    onOpenJob
  }: {
    step: JobStep
    turnId: string
    jobs: ReadonlyMap<string, TranscriptJob>
    liveJobs: ChatJob[]
    now: number
    view: BlockView
    overrides: Record<string, BlockView>
    onView: (id: string, view: BlockView) => void
    folderPath: string
    sessionId: string | null
    onOpenJob?: (ref?: string) => void
  } = $props()

  type ChildItem = Extract<ChatDeliverable, { kind: 'child' }>

  const team = $derived(step.target === 'team')
  const index = $derived(jobs.get(step.job))
  const open = $derived(view !== 'collapsed')

  function liveOf(id: string): ChatJob | undefined {
    return liveJobs.find((job) => job.id === id)
  }
  function activeLive(id: string): ChatJob | null {
    const live = liveOf(id)
    return live && isActiveJob(live.status) ? live : null
  }
  function progressOf(id: string): ChatChildProgress | null {
    const live = liveOf(id)
    return live?.kind.kind === 'child' ? live.kind.progress : null
  }
  function progressLine(progress: ChatChildProgress): string {
    const tools = progress.tools.join(', ')
    return `turn ${progress.turns} of ${progress.maxTurns}${tools ? ` · ${tools}` : ''}`
  }
  function childItem(event: JobEvent): ChildItem | null {
    return event.kind === 'deliverable' && event.item.kind === 'child' ? event.item : null
  }
  function finals(events: JobEvent[]): ChildItem[] {
    return events.flatMap((event) => {
      const item = childItem(event)
      return item && !item.note ? [item] : []
    })
  }

  // Members come from the index; a member only the events name (index missing) still gets a section.
  const members = $derived.by(() => {
    const list = [...(index?.members ?? [])]
    for (const event of step.events)
      if (event.member !== null && !list.some(([name]) => name === event.member)) list.push([event.member, ''])
    return list.map(([name, id]) => ({
      name,
      id,
      state: jobState(jobs.get(id), liveOf(id)),
      agent: jobs.get(id)?.agent ?? null,
      events: step.events.filter((event) => event.member === name)
    }))
  })

  const header = $derived.by((): { tone: StatusTone; label: string; title: string; duration: number | null } => {
    if (team) {
      const done = members.filter((member) => member.state.label === 'Done').length
      // Failed if any member failed, working while any runs, done once all are.
      const tone: StatusTone = members.some((member) => member.state.label === 'Failed')
        ? 'danger'
        : members.some((member) => member.state.tone === 'working')
          ? 'working'
          : members.length > 0 && done === members.length
            ? 'success'
            : 'idle'
      const name = step.events.flatMap((event) => childItem(event)?.team?.name ?? [])[0]
      return { tone, label: `${done} of ${members.length} done`, title: name ?? index?.title ?? '', duration: null }
    }
    const live = liveOf(step.job)
    const state = jobState(index, live)
    return {
      tone: state.tone,
      label: state.label,
      title: index?.title ?? '',
      duration: jobDurationMs(index, live, now)
    }
  })
  const failed = $derived(header.tone === 'danger')
  const progress = $derived(team ? null : progressOf(step.job))
  const results = $derived(team ? [] : finals(step.events))
  const folded = $derived.by(() => {
    if (progress && activeLive(step.job)) return progressLine(progress)
    const text = firstLine(finals(step.events).at(-1)?.text)
    if (text) return text
    return step.continues ? 'Continues in a later step' : ''
  })
  const ownEvents = $derived(step.events.filter((event) => event.member === null))
  const SEND_LABEL = { steer: 'steer', followUp: 'follow-up', mail: 'mail' } as const

  function memberView(name: string): BlockView {
    return overrides[`${step.id}:${name}`] ?? 'preview'
  }
</script>

{#snippet row(label: string)}
  <span class="font-mono text-xs text-muted">{label}</span>
{/snippet}

{#snippet rows(events: JobEvent[], jobId: string, spawnFallback: boolean)}
  {@const live = activeLive(jobId)}
  {@const liveProgress = progressOf(jobId)}
  <div class="grid grid-cols-[64px_1fr] gap-x-2 gap-y-1.5 text-sm">
    {#if spawnFallback && !events.some((event) => event.kind === 'spawn')}
      {@render row('spawned')}
      <span class="text-muted">{index?.turnId === turnId ? 'In an earlier step' : 'In an earlier reply'}</span>
    {/if}
    {#each events as event, idx (idx)}
      {#if event.kind === 'spawn'}
        {@const spawned = spawnReceipt(event.call.details)}
        {@const task = record(event.call.arguments).task}
        {@render row('spawned')}
        <span class="line-clamp-2 min-w-0 text-fg"
          >{spawned?.kind === 'team'
            ? spawned.members.map(([name]) => name).join(', ')
            : typeof task === 'string'
              ? task
              : ''}</span
        >
      {:else if event.kind === 'send'}
        {@const message = record(event.call.arguments).message}
        {@render row(SEND_LABEL[event.send])}
        <span class="line-clamp-4 min-w-0 whitespace-pre-wrap text-fg"
          >{typeof message === 'string' ? message : ''}</span
        >
      {:else if event.kind === 'wait'}
        {@const ms = event.call.result?.durationMs}
        {@render row('waited')}
        <span class="font-mono text-xs text-muted tabular-nums">{ms != null ? formatDuration(ms) : ''}</span>
      {:else if event.kind === 'kill'}
        {@render row('stopped')}
        <span class="text-muted">{event.stopping ? 'Stopping' : 'Already stopped'}</span>
      {:else if event.kind === 'deliverable' && event.item.kind === 'child' && event.item.note}
        {@render row('note')}
        <JobResult item={event.item} tone="note" {folderPath} {sessionId} />
      {/if}
    {/each}
    {#if live && !step.continues}
      {@render row('now')}
      <span class="flex min-w-0 items-center gap-1.5 text-muted">
        <Spinner size={12} class="shrink-0" />
        <span class="min-w-0 truncate">{liveProgress ? progressLine(liveProgress) : ''}</span>
      </span>
    {/if}
  </div>
{/snippet}

{#snippet resultSection(items: ChildItem[])}
  {#each items as item, idx (idx)}
    <div class="flex min-w-0 flex-col gap-1.5">
      <span class="text-xs font-semibold tracking-wide text-muted uppercase"
        >{item.partial ? 'Partial Result' : 'Result'}</span
      >
      <JobResult {item} tone="result" {folderPath} {sessionId} />
    </div>
  {/each}
{/snippet}

<RailItem icon={UserGroupIcon} tone={header.tone} viewId={step.id}>
  <div
    data-slot="job-card"
    class={cn('mb-1.5 min-w-0 rounded-lg border bg-surface', failed ? 'border-danger-border' : 'border-edge')}
  >
    <button
      type="button"
      aria-expanded={open}
      onclick={() => onView(step.id, open ? 'collapsed' : 'preview')}
      class="flex h-8 w-full min-w-0 cursor-pointer items-center gap-2 rounded-lg px-2.5 text-left focus-visible:shadow-focus-ring focus-visible:outline-none"
    >
      <StatusDot tone={header.tone} />
      <span class="shrink-0 font-mono text-xs text-bright">{step.job}</span>
      <span class="min-w-0 flex-1 truncate text-sm text-muted">{header.title}</span>
      {#if header.label}
        <span class={cn('shrink-0 text-xs font-medium', statusToneText[header.tone])}>{header.label}</span>
      {/if}
      {#if header.duration !== null}
        <span class="shrink-0 font-mono text-xs text-muted tabular-nums">{formatDuration(header.duration)}</span>
      {/if}
      <DisclosureChevron {open} size={14} class="shrink-0 text-muted" />
    </button>
    {#if !open}
      {#if folded}<div class="truncate px-2.5 pb-2 text-sm text-muted">{folded}</div>{/if}
    {:else}
      <div data-disclose transition:disclose class="min-w-0">
        <div class="flex min-w-0 flex-col gap-2 border-t border-edge px-3 py-2.5">
          {@render rows(ownEvents, step.job, true)}
          {#if team}
            {#each members.filter((member) => member.events.length > 0) as member (member.name)}
              {@const memberOpen = memberView(member.name) !== 'collapsed'}
              <div class="flex min-w-0 flex-col gap-2 border-t border-edge/60 pt-2">
                <div class="flex min-w-0 items-center gap-2">
                  <button
                    type="button"
                    aria-expanded={memberOpen}
                    onclick={() => onView(`${step.id}:${member.name}`, memberOpen ? 'collapsed' : 'preview')}
                    class="flex min-w-0 flex-1 cursor-pointer items-center gap-2 rounded text-left focus-visible:shadow-focus-ring focus-visible:outline-none"
                  >
                    <StatusDot tone={member.state.tone} size="sm" />
                    <span class="shrink-0 font-mono text-xs text-fg">{member.name}</span>
                    {#if member.agent}<Chip class="font-mono text-xs">{member.agent}</Chip>{/if}
                    {#if member.state.label}
                      <span class={cn('shrink-0 text-xs font-medium', statusToneText[member.state.tone])}
                        >{member.state.label}</span
                      >
                    {/if}
                    <DisclosureChevron open={memberOpen} size={12} class="shrink-0 text-muted" />
                  </button>
                  {#if onOpenJob && member.id}
                    <Button variant="text" size="xs" class="ms-auto px-0" onclick={() => onOpenJob(member.id)}
                      >View Agent</Button
                    >
                  {/if}
                </div>
                {#if memberOpen}
                  {@render rows(member.events, member.id, false)}
                  {@render resultSection(finals(member.events))}
                {/if}
              </div>
            {/each}
          {/if}
        </div>
        {#if results.length}
          <div class="flex min-w-0 flex-col gap-2.5 border-t border-edge px-3 py-2.5">
            {@render resultSection(results)}
          </div>
        {/if}
        <div class="flex h-7 items-center justify-between gap-2 border-t border-edge px-3 font-mono text-xs text-muted">
          <span class="min-w-0 truncate">{progress ? progressLine(progress) : ''}</span>
          {#if onOpenJob}
            <Button variant="text" size="xs" class="px-0" onclick={() => onOpenJob(step.job)}>View Agent</Button>
          {/if}
        </div>
      </div>
    {/if}
  </div>
</RailItem>
