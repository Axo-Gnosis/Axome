<script lang="ts">
  import { DisclosureChevron } from '$lib/components/ui/disclosure-chevron'
  import { StatusDot } from '$lib/components/ui/status-dot'
  import * as Message from '$lib/features/chat/elements/message'
  import { formatCount, formatDuration, formatTime } from '../../format'
  import type { ToolStep } from '../../turns'
  import AskAnswerRow from './AskAnswerRow.svelte'
  import { askKey, askSections } from './askDocument'
  import { parseAskResult } from './askResult'

  let {
    call,
    folderPath,
    sessionId
  }: {
    call: ToolStep
    folderPath: string
    sessionId: string | null
  } = $props()

  let open = $state(false)
  const result = $derived(parseAskResult(call))
  const count = $derived(result.blocks?.filter((block) => block.type !== 'markdown').length ?? 0)
  // One line of the first Markdown block, as plain text, to say what was asked.
  const context = $derived.by(() => {
    const markdown = result.blocks?.find((block) => block.type === 'markdown' && block.text.trim())
    if (markdown?.type !== 'markdown') return null
    const line = markdown.text.trim().split('\n')[0]
    return line.replace(/[#*_`>]/g, '').trim() || null
  })
  const terminal = $derived(
    result.outcome === 'dismissed'
      ? 'Dismissed. axo stopped without answers.'
      : result.outcome === 'cancelled'
        ? 'Stopped before it was answered.'
        : result.outcome === 'steered'
          ? 'You replied instead of answering.'
          : null
  )
  const answeredAt = $derived(formatTime(call.endedAt))
  const timeoutText = $derived.by(() => {
    const duration = call.result?.durationMs
    return duration != null && Number.isFinite(duration) && duration >= 0
      ? `No answer after ${formatDuration(duration)}. axo used the suggested answers.`
      : 'No answer in time. axo used the suggested answers.'
  })
</script>

<section data-slot="ask-summary" aria-label="Ask" class="flex min-w-0 flex-col gap-3">
  {#if !result.blocks || result.outcome === 'unavailable'}
    <p class="text-sm text-muted">
      <span class="font-medium text-fg">Ask</span> · Question details unavailable. {terminal ??
        (call.status === 'error' ? 'Failed' : 'Completed')}
    </p>
    {#if result.outcome === 'failed' && result.message}
      <p class="text-sm wrap-anywhere whitespace-pre-wrap text-danger-bright">{result.message}</p>
    {/if}
  {:else if result.outcome === 'answered' || result.outcome === 'timedOut'}
    {#each askSections(result.blocks) as section, index (index)}
      {#if section.type === 'markdown'}
        <Message.Response text={section.text} streaming={false} {folderPath} {sessionId} />
      {:else}
        <div class="flex min-w-0 flex-col divide-y divide-edge rounded-lg border border-edge">
          {#each section.questions as question (question.id)}
            {@const row = result.rows[askKey(question.id)]}
            <div class="flex min-w-0 items-start gap-2.5 px-3 py-2">
              <AskAnswerRow
                question={question.question}
                answered={row.answered}
                text={row.text}
                note={row.note}
                suggested={row.suggested}
              />
            </div>
          {/each}
        </div>
      {/if}
    {/each}
    {#if result.outcome === 'timedOut'}
      <p class="flex items-center gap-2 text-sm text-muted">
        <StatusDot tone="waiting" size="sm" />{timeoutText}
      </p>
    {:else}
      <p class="text-sm text-muted">
        Answered by you{#if answeredAt}{' · '}<span class="font-mono">{answeredAt}</span>{/if}
      </p>
    {/if}
  {:else}
    <details bind:open class="min-w-0">
      <summary
        class="flex min-w-0 cursor-pointer list-none items-center gap-2 rounded text-sm focus-visible:shadow-focus-ring focus-visible:outline-none [&::-webkit-details-marker]:hidden"
      >
        <DisclosureChevron {open} size={14} class="text-muted" />
        <span class="shrink-0 font-medium text-fg">Asked {formatCount(count, 'question')}</span>
        {#if context}<span class="min-w-0 truncate text-muted">{context}</span>{/if}
      </summary>
      <div class="flex min-w-0 flex-col gap-2 ps-5.5 pt-2">
        {#each result.blocks as block, index (index)}
          {#if block.type === 'markdown'}
            <Message.Response text={block.text} streaming={false} {folderPath} {sessionId} />
          {:else}
            <p class="text-sm wrap-anywhere text-muted">{block.question}</p>
          {/if}
        {/each}
      </div>
    </details>
    {#if result.outcome === 'failed'}
      <p class="text-sm text-danger-bright">Ask failed.</p>
      {#if result.message}<p class="text-sm wrap-anywhere whitespace-pre-wrap text-muted">{result.message}</p>{/if}
    {:else if result.outcome === 'steered'}
      <p class="text-sm text-muted">{terminal}</p>
    {:else}
      <p class="flex items-center gap-2 text-sm text-muted">
        <span class="size-2 shrink-0 rounded-xs bg-muted"></span>{terminal}
      </p>
    {/if}
  {/if}
</section>
