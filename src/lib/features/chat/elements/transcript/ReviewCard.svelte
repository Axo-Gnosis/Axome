<script lang="ts">
  import MarkdownRenderer from '$lib/components/markdown/MarkdownRenderer.svelte'
  import { Button } from '$lib/components/ui/button'
  import { Textarea } from '$lib/components/ui/input'
  import type { ChatReview } from '$lib/types/backend'
  import { KeyHint } from '../key-hint'
  import { chatShortcut } from '$lib/features/chat/keymap/bindings.svelte'
  import { getTranscriptNav } from '$lib/features/chat/keymap/nav.svelte'
  import { getChatKeys } from '$lib/features/chat/keymap/router.svelte'

  type Action = () => Promise<string | null>

  let {
    review,
    folderPath,
    sessionId,
    disabled = false,
    onAccept,
    onRevise,
    onDismiss
  }: {
    review: ChatReview
    folderPath: string
    sessionId: string | null
    disabled?: boolean
    /** Each action resolves to its error message, or null once it took effect. */
    onAccept: Action
    onRevise: (feedback: string) => Promise<string | null>
    onDismiss: Action
  } = $props()

  const id = $props.id()
  const nav = getTranscriptNav()
  const keys = getChatKeys()
  let feedbackField = $state<HTMLTextAreaElement | null>(null)
  let feedback = $state('')
  let error = $state<string | null>(null)
  // Set while an action runs; a successful action resolves the review, which unmounts the card.
  let busy = $state(false)
  const locked = $derived(disabled || busy)
  const canRevise = $derived(!locked && feedback.trim().length > 0)

  async function run(action: Action) {
    if (locked) return
    busy = true
    error = null
    try {
      error = await action()
    } finally {
      busy = false
    }
  }

  const accept = () => run(onAccept)
  const revise = () => {
    if (canRevise) void run(() => onRevise(feedback.trim()))
  }
  const dismiss = () => run(onDismiss)
</script>

<!-- One stop: Activate accepts, or requests changes from inside the feedback; typing starts the feedback. -->
<section
  data-slot="review-card"
  class="flex min-w-0 flex-col gap-3 rounded-lg border border-edge p-3 data-[nav-active]:border-accent"
  aria-label={`Review ${review.mode} result`}
  {@attach nav.stop(`review:${review.callId}`, () => ({
    activate: (editing) => (editing ? revise() : void accept()),
    field: () => feedbackField,
    submitOnEnter: true
  }))}
  {@attach keys.scope(() => ({ commands: { dismiss: () => void dismiss() } }))}
  {@attach (element) => {
    // Never steal focus from text the user is typing; they can click into the card instead.
    if (!document.activeElement?.closest('input, textarea, select, [contenteditable="true"]')) nav.focus(element)
  }}
>
  <span class="text-sm text-muted">Review <span class="font-mono text-fg">{review.mode}</span> result</span>
  {#if typeof review.result === 'string'}
    <MarkdownRenderer source={review.result} {folderPath} {sessionId} />
  {:else}
    <pre
      class="m-0 max-h-80 overflow-auto rounded bg-surface p-2 font-mono text-xs wrap-anywhere whitespace-pre-wrap text-fg">{JSON.stringify(
        review.result,
        null,
        2
      )}</pre>
  {/if}
  <label for={`${id}-feedback`} class="sr-only">Requested changes</label>
  <Textarea
    id={`${id}-feedback`}
    bind:ref={feedbackField}
    rows={3}
    placeholder="What should change?"
    bind:value={feedback}
    disabled={locked}
  />
  <div class="flex flex-wrap items-center gap-2">
    <Button type="button" variant="accent" size="xs" disabled={locked} onclick={() => void accept()}>
      Accept <KeyHint keys={chatShortcut('activate')} />
    </Button>
    <Button type="button" size="xs" disabled={!canRevise} onclick={revise}>Request Changes</Button>
    <Button type="button" variant="ghost" size="xs" disabled={locked} onclick={() => void dismiss()}>
      Dismiss <KeyHint keys={chatShortcut('dismiss')} />
    </Button>
  </div>
  {#if error}
    <p role="alert" class="text-sm whitespace-pre-wrap text-danger">{error}</p>
  {/if}
</section>
