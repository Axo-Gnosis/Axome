<!--
  @component
  ReceiptRow — a coordination receipt that names no known job (running, failed, a note to
  the parent, a wait on several jobs, a mode change). Not expandable.
-->

<script lang="ts">
  import { Chip } from '$lib/components/ui/chip'
  import { firstLine, formatDuration } from '$lib/features/chat/format'
  import type { ToolStep } from '$lib/features/chat/turns'
  import type { ChatContent } from '$lib/types/backend'
  import { Clock01Icon, InfoIcon, Navigation03Icon, UserGroupIcon } from '@hugeicons/core-free-icons'
  import { record } from '../tool/payload'
  import { killReceipt, sendReceipt, waitReceipt } from '../tool/receipt'
  import JobRef from './JobRef.svelte'
  import type { StatusTone } from '$lib/components/ui/status-dot'
  import RailRow from './RailRow.svelte'

  let { call, onOpenJob }: { call: ToolStep; onOpenJob?: (ref?: string) => void } = $props()

  const args = $derived(record(call.arguments))
  const failed = $derived(call.status === 'error')
  const running = $derived(call.status === 'running')
  const outputText = $derived(
    call.output
      .flatMap((part: ChatContent) => (part.type === 'text' ? [part.text] : []))
      .join(' ')
      .trim()
  )
  const icon = $derived(
    call.name === 'wait'
      ? Clock01Icon
      : call.name === 'send'
        ? Navigation03Icon
        : call.name === 'enter_mode'
          ? InfoIcon
          : UserGroupIcon
  )
  const waited = $derived(call.name === 'wait' ? waitReceipt(call.details) : null)
  const sent = $derived(call.name === 'send' ? sendReceipt(call.details) : null)
  const killed = $derived(call.name === 'kill' ? killReceipt(call.details) : null)
  const message = $derived(typeof args.message === 'string' ? args.message : '')

  const row = $derived.by((): { verb: string; tone: StatusTone } => {
    switch (call.name) {
      case 'spawn':
        return failed ? { verb: 'Spawn failed', tone: 'danger' } : { verb: 'Spawning', tone: 'working' }
      case 'send':
        if (failed) return { verb: 'Send failed', tone: 'danger' }
        if (!sent) return { verb: running ? 'Sending' : 'Sent', tone: running ? 'working' : 'idle' }
        return {
          verb: {
            note: 'Sent note to parent',
            steer: 'Steered',
            followUp: 'Queued follow-up for',
            mail: 'Sent mail to'
          }[sent.kind],
          tone: 'idle'
        }
      case 'wait':
        if (running) return { verb: 'Waiting', tone: 'working' }
        if (failed) return { verb: 'Wait failed', tone: 'danger' }
        switch (waited?.reason) {
          case 'results':
            return { verb: 'Waited for', tone: 'idle' }
          case 'mail':
            return { verb: 'Wait ended', tone: 'idle' }
          case 'interrupted':
            return { verb: 'Wait interrupted', tone: 'idle' }
          case 'idle':
            return { verb: 'Nothing was running', tone: 'idle' }
          default:
            return { verb: 'Waited', tone: 'idle' }
        }
      case 'kill':
        if (failed) return { verb: 'Stop failed', tone: 'danger' }
        return { verb: killed?.stopping === false ? 'Already stopped' : 'Stopping', tone: 'idle' }
      case 'enter_mode':
        return { verb: 'Entered mode', tone: 'idle' }
      default:
        return { verb: call.name, tone: failed ? 'danger' : 'idle' }
    }
  })
</script>

{#snippet target()}
  <span class="flex min-w-0 items-center gap-1.5">
    {#if failed}
      <span class="min-w-0 truncate">{outputText}</span>
    {:else if call.name === 'spawn'}
      <span class="min-w-0 truncate">{firstLine(args.task)}</span>
    {:else if call.name === 'send'}
      {@const to = sent?.to ?? (typeof args.to === 'string' ? args.to : '')}
      {#if sent?.kind !== 'note' && to}<JobRef ref={to} member onOpen={onOpenJob} />{/if}
      {#if message}<span class="min-w-0 truncate">{message}</span>{/if}
    {:else if call.name === 'wait'}
      {#if running}
        <span class="truncate">for background work</span>
      {:else if waited?.reason === 'results'}
        {#each waited.jobs as ref, idx (idx)}<JobRef {ref} onOpen={onOpenJob} />{/each}
      {:else if waited?.reason === 'mail'}
        <span class="truncate">mail arrived</span>
      {:else if waited?.reason === 'interrupted'}
        <span class="truncate">by a new message</span>
      {:else if !waited}
        <span class="min-w-0 truncate">{outputText}</span>
      {/if}
    {:else if call.name === 'kill'}
      {@const id = killed?.id ?? (typeof args.id === 'string' ? args.id : '')}
      {#if id}<JobRef ref={id} onOpen={onOpenJob} />{/if}
    {:else if call.name === 'enter_mode'}
      <Chip class="font-mono text-xs">{typeof args.mode === 'string' ? args.mode : ''}</Chip>
    {/if}
  </span>
{/snippet}

{#snippet aside()}
  {#if waited?.reason === 'results' && call.result?.durationMs != null}
    <span class="tabular-nums">{formatDuration(call.result.durationMs)}</span>
  {/if}
{/snippet}

<RailRow {icon} tone={row.tone} viewId={call.id} verb={row.verb} {target} {aside} />
