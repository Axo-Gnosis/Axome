<script lang="ts" module>
  import { tv } from 'tailwind-variants'

  export const paletteSurfaceVariants = tv({
    base: 'flex flex-col overflow-hidden rounded-xl border border-edge bg-raised shadow-popover'
  })
</script>

<script lang="ts">
  import { Dialog as DialogPrimitive } from 'bits-ui'
  import type { Snippet } from 'svelte'
  import * as Dialog from '../dialog'

  let {
    open,
    onOpenChange,
    label,
    children,
    onkeydown,
    onOpenAutoFocus
  }: {
    open: boolean
    onOpenChange: (open: boolean) => void
    label: string
    children?: Snippet
    onkeydown?: DialogPrimitive.ContentProps['onkeydown']
    onOpenAutoFocus?: DialogPrimitive.ContentProps['onOpenAutoFocus']
  } = $props()
</script>

<DialogPrimitive.Root {open} {onOpenChange}>
  <DialogPrimitive.Portal>
    <Dialog.Overlay />
    <!-- Keep the modal boundary on the palette, not the viewport, so backdrop clicks dismiss it. -->
    <DialogPrimitive.Content
      data-slot="palette-dialog"
      class={paletteSurfaceVariants({ class: 'fixed inset-x-3 top-[15vh] z-50 mx-auto max-h-[80dvh] max-w-3xl' })}
      aria-label={label}
      {onkeydown}
      {onOpenAutoFocus}
    >
      {#if children}{@render children()}{/if}
    </DialogPrimitive.Content>
  </DialogPrimitive.Portal>
</DialogPrimitive.Root>
