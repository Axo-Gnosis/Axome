<script lang="ts">
  import MarkdownRenderer from '$lib/components/markdown/MarkdownRenderer.svelte'
  import { Button } from '$lib/components/ui/button'
  import { Textarea } from '$lib/components/ui/input'
  import type { ChatReview } from '$lib/types/backend'
  import { KeyHint } from '../key-hint'

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

  function keydown(event: KeyboardEvent) {
    if (event.isComposing || event.ctrlKey || event.metaKey || event.altKey) return
    if (event.key === 'Escape') {
      event.preventDefault()
      event.stopPropagation()
      void dismiss()
    } else if (event.key === 'Enter') {
      event.stopPropagation()
      // A focused button keeps native Enter activation; Shift+Enter keeps its newline in the feedback.
      if (!(event.target instanceof HTMLElement) || event.target.closest('button')) return
      if (event.target.closest('textarea')) {
        if (event.shiftKey) return
        event.preventDefault()
        revise()
      } else {
        event.preventDefault()
        void accept()
      }
    }
  }
</script>

<!-- Delegated keys: Enter accepts (Request Changes from the feedback field), Esc dismisses; controls stay native. -->
<!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
<section
  data-slot="review-card"
  class="flex min-w-0 flex-col gap-3 rounded-lg border border-edge p-3 outline-none focus-visible:shadow-focus-ring"
  aria-label={`Review ${review.mode} result`}
  tabindex="-1"
  {@attach (element) => {
    // Never steal focus from text the user is typing; they can click or Tab into the card.
    if (!document.activeElement?.closest('input, textarea, select, [contenteditable="true"]')) element.focus()
  }}
  onkeydown={keydown}
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
  <Textarea id={`${id}-feedback`} rows={3} placeholder="What should change?" bind:value={feedback} disabled={locked} />
  <div class="flex flex-wrap items-center gap-2">
    <Button type="button" variant="accent" size="xs" disabled={locked} onclick={() => void accept()}>
      Accept <KeyHint keys="Enter" />
    </Button>
    <Button type="button" size="xs" disabled={!canRevise} onclick={revise}>Request Changes</Button>
    <Button type="button" variant="ghost" size="xs" disabled={locked} onclick={() => void dismiss()}>
      Dismiss <KeyHint keys="Esc" />
    </Button>
  </div>
  {#if error}
    <p role="alert" class="text-sm whitespace-pre-wrap text-danger">{error}</p>
  {/if}
</section>
