<!--
  @component
  WindowDialog — a modal that reads as a window over the app (Settings):
  fills most of the viewport with a margin, capped at max-w-7xl, rounded,
  `bg-ground`. The caller lays out its own panes inside. The modal boundary is
  the visible window, so backdrop clicks dismiss it like Esc does.

  @param open - whether the window is shown
  @param onOpenChange - Esc, backdrop click
  @param label - accessible name
  @param onModalClose - lets action shortcuts dismiss it first (see modalRegistry)
-->

<script lang="ts">
  import { Dialog as DialogPrimitive } from 'bits-ui'
  import { registerModal } from '$lib/utils/modalRegistry.svelte'
  import type { Snippet } from 'svelte'
  import Overlay from './DialogOverlay.svelte'

  let {
    open,
    onOpenChange,
    label,
    onModalClose,
    children
  }: {
    open: boolean
    onOpenChange: (open: boolean) => void
    label: string
    onModalClose?: () => void
    children?: Snippet
  } = $props()

  // Registered while the content is in the DOM (see DialogContent).
  function registerPresence() {
    return registerModal({ isOpen: () => true, close: onModalClose })
  }
</script>

<DialogPrimitive.Root {open} {onOpenChange}>
  <DialogPrimitive.Portal>
    <Overlay />
    <DialogPrimitive.Content
      data-slot="window-dialog"
      aria-label={label}
      class="fixed inset-x-[4vw] inset-y-[4dvh] z-50 mx-auto flex max-w-7xl overflow-hidden rounded-xl border border-edge bg-ground shadow-popover outline-none"
      {@attach registerPresence}
    >
      {#if children}{@render children()}{/if}
    </DialogPrimitive.Content>
  </DialogPrimitive.Portal>
</DialogPrimitive.Root>
