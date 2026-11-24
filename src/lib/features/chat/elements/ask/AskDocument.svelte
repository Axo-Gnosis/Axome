<script lang="ts">
  import { tick } from 'svelte'
  import { Button } from '$lib/components/ui/button'
  import { StatusDot } from '$lib/components/ui/status-dot'
  import * as Message from '$lib/features/chat/elements/message'
  import { Kbd, KbdGroup } from '$lib/components/ui/kbd'
  import { splitShortcut } from '$lib/features/command-palette/shortcuts/spec'
  import { chatBindings } from '$lib/features/chat/keymap/bindings.svelte'
  import { getTranscriptNav, NAV_ACTIVE } from '$lib/features/chat/keymap/nav.svelte'
  import { getChatKeys } from '$lib/features/chat/keymap/router.svelte'
  import type { ChatController } from '../../controller.svelte'
  import type { ChatQuestions } from '$lib/types/backend'
  import { askCountdown, askKey, askRefusal, askSections, buildAskAnswers } from './askDocument'
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

  const nav = getTranscriptNav()
  const keys = getChatKeys()
  let section = $state<HTMLElement | null>(null)
  const draft = $derived(controller.askDrafts[askKey(pending.callId)])
  const validation = $derived(draft && buildAskAnswers(draft.blocks, draft.questions))
  const disabled = $derived(!draft || draft.busy || controller.readOnly || controller.connection !== 'live')
  const canSend = $derived(!disabled && !!validation && validation.ready === validation.total)
  const stopBase = (id: string) => `ask:${pending.callId}:${askKey(id)}`
  const sendKeys = $derived(splitShortcut(chatBindings('sendAnswers')[0]))

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
      submitted.questions[askKey(refusedId)].collapsed = false
      await tick()
      if (controller !== owner || draft !== submitted) return
      const stop = field(refusedId)?.querySelector<HTMLElement>('[data-nav-stop]')
      if (stop) nav.focus(stop)
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

  const fields = () => Array.from(section?.querySelectorAll<HTMLElement>('[data-ask-question-id]') ?? [])
  const field = (id: string) => fields().find((node) => node.dataset.askQuestionId === id)

  /**
   * Collapses a question whose answer is complete. With `advance` the highlight moves to the next
   * open question (wrapping), else to Send; without it, it rests quietly on the collapsed row.
   */
  async function settle(id: string, advance: boolean) {
    const key = askKey(id)
    const state = draft?.questions[key]
    if (!state || validation?.errors[key]) return
    state.collapsed = true
    await tick()
    const all = fields()
    const at = all.findIndex((node) => node.dataset.askQuestionId === id)
    const row = all[at]?.querySelector<HTMLElement>('[data-nav-stop]')
    if (!advance) {
      if (row) nav.focus(row, { quiet: true })
      return
    }
    const next = [...all.slice(at + 1), ...all.slice(0, at)].find((node) => !('collapsed' in node.dataset))
    const stop =
      next?.querySelector<HTMLElement>('[data-nav-stop]') ??
      section?.querySelector<HTMLElement>(`[data-nav-stop="${CSS.escape(`ask:${pending.callId}:send`)}"]`)
    if (stop) nav.focus(stop)
  }
</script>

<section
  bind:this={section}
  data-ask-call-id={pending.callId}
  data-slot="ask-document"
  aria-label="Answer questions"
  {@attach keys.scope(() => ({
    commands: {
      sendAnswers: () => {
        if (!canSend) return false
        void submit()
      },
      dismiss: () => {
        if (disabled) return false
        void dismiss()
      }
    }
  }))}
  class="flex min-w-0 flex-col gap-3"
>
  {#if draft && validation}
    {#each askSections(draft.blocks) as part, index (index)}
      {#if part.type === 'markdown'}
        <Message.Response text={part.text} streaming={false} {folderPath} {sessionId} />
      {:else}
        <div
          class="flex min-w-0 flex-col divide-y divide-edge overflow-hidden rounded-lg border border-edge bg-surface"
        >
          {#each part.questions as question (askKey(question.id))}
            {@const key = askKey(question.id)}
            <AskQuestionField
              {question}
              draft={draft.questions[key]}
              {disabled}
              base={stopBase(question.id)}
              error={draft.errors[key] ?? validation.errors[key] ?? null}
              onChange={() => {
                delete draft.errors[key]
              }}
              onSettle={(advance) => void settle(question.id, advance)}
            />
          {/each}
        </div>
      {/if}
    {/each}
    {#if draft.error}
      <p role="alert" class="text-sm text-danger-bright">{draft.error}</p>
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
        <Button
          variant="accent"
          size="xs"
          disabled={!canSend}
          onclick={() => void submit()}
          {@attach nav.stop(`ask:${pending.callId}:send`, () => ({ activate: () => void submit() }))}
          class={NAV_ACTIVE}
        >
          Send answers
          {#if sendKeys.length}
            <KbdGroup
              >{#each sendKeys as key (key)}<Kbd>{key}</Kbd>{/each}</KbdGroup
            >
          {/if}
        </Button>
      </div>
    </div>
  {:else}
    <p role="alert" class="text-sm text-danger-bright">This question document is unavailable.</p>
  {/if}
</section>
