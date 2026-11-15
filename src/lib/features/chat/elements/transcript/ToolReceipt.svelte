<script lang="ts">
  import { Chip } from '$lib/components/ui/chip'
  import { Spinner } from '$lib/components/ui/progress'
  import type { ToolStep } from '$lib/features/chat/turns'
  import type { ChatContent } from '$lib/types/backend'
  import Icon from '$lib/icons/Icon.svelte'
  import { cn } from '$lib/utils/cn'
  import { CircleXIcon, TerminalIcon } from '@hugeicons/core-free-icons'
  import { formatCommandExit, parseBashDetails } from '../tool/bashOutput'
  import { record } from '../tool/payload'
  import { killReceipt, sendReceipt, spawnReceipt, waitReceipt } from '../tool/receipt'
  import Deliverables from './Deliverables.svelte'
  import JobRef from './JobRef.svelte'

  let {
    call,
    folderPath,
    sessionId,
    onOpenJob
  }: {
    call: ToolStep
    folderPath: string
    sessionId: string | null
    onOpenJob?: (ref?: string) => void
  } = $props()

  const args = $derived(record(call.arguments))
  const details = $derived(record(call.details))
  const bash = $derived(call.name === 'bash' ? parseBashDetails(call.details) : null)
  const outputText = $derived(
    call.output
      .flatMap((part: ChatContent) => (part.type === 'text' ? [part.text] : []))
      .join(' ')
      .trim()
  )

  const FAILED: Partial<Record<string, string>> = {
    spawn: 'Spawn failed',
    kill: 'Stop failed',
    send: 'Send failed'
  }
</script>

<div data-slot="tool-receipt" data-tool={call.name} class="flex min-w-0 flex-col gap-1">
  <div class={cn('flex min-w-0 items-center gap-2 text-sm text-muted', call.name === 'wait' && 'flex-wrap')}>
    {#if call.status === 'error' && FAILED[call.name]}
      <Icon icon={CircleXIcon} size={14} class="shrink-0 text-danger" aria-hidden="true" />
      <span class="truncate text-danger">{outputText || FAILED[call.name]}</span>
    {:else if call.name === 'bash' && bash?.job}
      <Icon icon={TerminalIcon} size={14} class="shrink-0" aria-hidden="true" />
      <span class="shrink-0">Backgrounded</span>
      <JobRef ref={bash.job} onOpen={onOpenJob} />
      {#if typeof args.command === 'string' && args.command}
        <code class="min-w-0 truncate font-mono text-xs text-fg">{args.command}</code>
      {/if}
      {#if bash.exit}
        {@const exit = formatCommandExit(bash.exit)}
        <span class={cn('shrink-0 text-xs', exit.failed ? 'text-danger' : 'text-subtle')}>{exit.label}</span>
      {/if}
    {:else if call.name === 'spawn'}
      {@const spawned = spawnReceipt(call.details)}
      <span class="shrink-0">Spawned</span>
      {#if spawned?.kind === 'team'}
        {@const members = spawned.members.map(([name]) => name).join(', ')}
        <JobRef ref={spawned.team} onOpen={onOpenJob} />
        {#if members}
          <span class="min-w-0 truncate">{members}</span>
        {/if}
      {:else}
        <JobRef ref={spawned?.job ?? 'subagent'} onOpen={onOpenJob} />
        {#if typeof args.agent === 'string' && args.agent}
          <span class="shrink-0 text-fg">{args.agent}</span>
        {/if}
        {#if typeof args.task === 'string' && args.task}
          <span class="min-w-0 truncate">{args.task}</span>
        {/if}
      {/if}
    {:else if call.name === 'wait'}
      {@const waited = waitReceipt(call.details)}
      {#if call.status === 'running'}
        <Spinner size={12} class="shrink-0 text-muted" />
        <span>Waiting for background work…</span>
      {:else if waited?.reason === 'results' && waited.jobs.length > 0}
        <span class="shrink-0">Waited on</span>
        {#each waited.jobs as ref, idx (idx)}
          <JobRef {ref} onOpen={onOpenJob} />
        {/each}
      {:else if waited?.reason === 'mail'}
        <span class="truncate">Wait ended · mail arrived</span>
      {:else if waited?.reason === 'interrupted'}
        <span class="truncate">Wait interrupted by a new message</span>
      {:else if waited?.reason === 'idle'}
        <span class="truncate">Nothing was running</span>
      {:else}
        <span class="min-w-0 truncate">{outputText || 'Waited'}</span>
      {/if}
    {:else if call.name === 'kill'}
      {@const killed = killReceipt(call.details)}
      {@const id = killed?.id ?? (typeof args.id === 'string' ? args.id : '')}
      <span class="shrink-0">{killed?.stopping ? 'Stopping' : 'Already stopped'}</span>
      {#if id}
        <JobRef ref={id} onOpen={onOpenJob} />
      {/if}
    {:else if call.name === 'send'}
      {@const sent = sendReceipt(call.details)}
      {@const to = sent?.to ?? (typeof args.to === 'string' ? args.to : '')}
      {@const message = typeof args.message === 'string' ? args.message : ''}
      {#if sent?.kind === 'note'}
        <span class="shrink-0">Sent note to parent</span>
      {:else}
        <span class="shrink-0"
          >{sent?.kind === 'steer'
            ? 'Steered'
            : sent?.kind === 'followUp'
              ? 'Queued follow-up for'
              : 'Sent mail to'}</span
        >
        {#if to}
          <JobRef ref={to} member onOpen={onOpenJob} />
        {/if}
      {/if}
      {#if message}
        <span class="min-w-0 truncate">{message}</span>
      {/if}
    {:else if call.name === 'yield'}
      {#if typeof details.mode === 'string'}
        <span class="truncate"
          >{call.status === 'running'
            ? `Awaiting review · ${details.mode}`
            : `${details.mode} result${typeof details.outcome === 'string' ? ` · ${details.outcome}` : ''}`}</span
        >
      {:else}
        <span class="truncate">Yielded result</span>
      {/if}
    {:else if call.name === 'enter_mode'}
      <span class="shrink-0">Entered mode</span>
      <Chip class="font-mono text-xs">{typeof args.mode === 'string' ? args.mode : ''}</Chip>
    {/if}
  </div>
  {#if call.name === 'wait'}
    <Deliverables items={call.result?.deliverables ?? []} {folderPath} {sessionId} {onOpenJob} />
  {/if}
</div>
