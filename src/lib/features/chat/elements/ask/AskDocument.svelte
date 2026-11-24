<script lang="ts">
  import { tick } from 'svelte'
  import { Button } from '$lib/components/ui/button'
  import { Kbd, KbdGroup } from '$lib/components/ui/kbd'
  import { StatusDot } from '$lib/components/ui/status-dot'
  import * as Message from '$lib/features/chat/elements/message'
  import type { ChatController } from '../../controller.svelte'
  import type { ChatQuestions } from '$lib/types/backend'
  import { askCountdown, askKey, askRefusal, buildAskAnswers, focusAskControl } from './askDocument'
  import AskQuestionField from './AskQuestionField.svelte'

  let {
    pending,
    controller,
    folderPath,
    sessionId,
    now
  }: {
    pending: ChatQuestions
    controller: ChatController
    folderPath: string
    sessionId: string | null
    now: number
  } = $props()

  let section: HTMLElement
  const draft = $derived(controller.askDrafts[askKey(pending.callId)])
  const validation = $derived(draft && buildAskAnswers(draft.blocks, draft.questions))
  const disabled = $derived(!draft || draft.busy || controller.readOnly || controller.connection !== 'live')
  const canSend = $derived(!disabled && !!validation && !validation.error && validation.ready === validation.total)

  async function submit() {
    if (!draft || !canSend || !validation) return
    const submitted = draft
    const key = askKey(pending.callId)
    const owner = controller
    submitted.busy = true
    submitted.error = null
    let refusedId: string | null = null
    try {
      const error = await owner.answer(pending.callId, validation.answers)
      if (!error || owner.askDrafts[key] !== submitted) return
      const refusal = askRefusal(error, submitted.blocks)
      refusedId = refusal.id
      if (refusedId === null) submitted.error = refusal.message
      else submitted.errors[askKey(refusedId)] = refusal.message
    } finally {
      submitted.busy = false
    }
    if (refusedId !== null) {
      await tick()
      if (controller !== owner || draft !== submitted || !section?.isConnected) return
      const field = Array.from(section.querySelectorAll<HTMLElement>('[data-ask-question-id]')).find(
        (node) => node.dataset.askQuestionId === refusedId
      )
      if (field) focusAskControl(field)
    }
  }

  async function dismiss() {
    if (!draft || disabled) return
    const submitted = draft
    submitted.busy = true
    try {
      await controller.dismiss(pending.callId)
    } finally {
      submitted.busy = false
    }
  }

  function handleKey(event: KeyboardEvent) {
    if (event.defaultPrevented || event.isComposing || disabled) return
    if (event.key === 'Escape' && !event.ctrlKey && !event.altKey && !event.metaKey && !event.shiftKey) {
      event.preventDefault()
      event.stopPropagation()
      void dismiss()
    } else if (event.key === 'Enter' && event.ctrlKey && !event.altKey && !event.metaKey && !event.shiftKey) {
      event.preventDefault()
      event.stopPropagation()
      void submit()
    }
  }
</script>

<!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
<section
  bind:this={section}
  data-ask-call-id={pending.callId}
  data-slot="ask-document"
  aria-label="Answer questions"
  tabindex="-1"
  onkeydown={handleKey}
  class="flex min-w-0 flex-col gap-3 rounded-lg focus-visible:shadow-focus-ring focus-visible:outline-none"
>
  {#if draft && validation}
    {#each draft.blocks as block, index (block.type === 'markdown' ? `markdown:${index}` : askKey(block.id))}
      {#if block.type === 'markdown'}
        <Message.Response text={block.text} streaming={false} {folderPath} {sessionId} />
      {:else}
        {@const key = askKey(block.id)}
        <AskQuestionField
          question={block}
          draft={draft.questions[key]}
          {disabled}
          error={draft.errors[key] ?? validation.errors[key] ?? null}
          onChange={() => {
            delete draft.errors[key]
          }}
        />
      {/if}
    {/each}
    {#if draft.error || validation.error}
      <p role="alert" class="text-sm text-danger-bright">{draft.error ?? validation.error}</p>
    {/if}
    {@const missing = validation.total - validation.ready}
    <div
      class="flex min-w-0 flex-wrap items-center gap-x-3 gap-y-2 rounded-lg border border-edge bg-surface py-1.5 ps-3 pe-1.5"
    >
      <div class="flex min-w-0 flex-1 items-center gap-2.5">
        <StatusDot tone="waiting" size="sm" />
        <p class="flex min-w-0 flex-wrap items-center gap-x-1.5 text-sm text-muted">
          <span class="font-medium text-fg">{validation.ready} of {validation.total} ready</span>
          {#if missing === 1}
            <span>· 1 needs an answer</span>
          {:else if missing > 1}
            <span>· {missing} need an answer</span>{/if}
          {#if pending.deadlineMs !== null}
            <span
              >· suggested answers apply in <span class="tabular-nums">{askCountdown(pending.deadlineMs, now)}</span
              ></span
            >{/if}
        </p>
      </div>
      <div class="ms-auto flex items-center gap-2">
        <Button variant="text" size="xs" {disabled} onclick={() => void dismiss()}>Dismiss</Button>
        <Button variant="accent" size="xs" disabled={!canSend} onclick={() => void submit()}>
          Send answers <KbdGroup><Kbd>Ctrl</Kbd><Kbd>Enter</Kbd></KbdGroup>
        </Button>
      </div>
    </div>
  {:else}
    <p role="alert" class="text-sm text-danger-bright">This question document is unavailable.</p>
  {/if}
</section>
