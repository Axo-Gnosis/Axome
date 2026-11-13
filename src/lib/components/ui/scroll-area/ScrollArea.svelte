<script lang="ts">
  import { ScrollArea as ScrollAreaPrimitive } from 'bits-ui'
  import { cn } from '$lib/utils/cn'
  import type { Snippet } from 'svelte'

  let {
    class: className,
    viewportRef = $bindable(null),
    children,
    ...rest
  }: {
    class?: string
    /** The scrolling element, for callers that track or drive scroll position. */
    viewportRef?: HTMLElement | null
    children?: Snippet
  } = $props()
</script>

<ScrollAreaPrimitive.Root class={cn('relative overflow-hidden', className)} {...rest}>
  <ScrollAreaPrimitive.Viewport bind:ref={viewportRef} class="h-full w-full">
    {#if children}{@render children()}{/if}
  </ScrollAreaPrimitive.Viewport>
  <ScrollAreaPrimitive.Scrollbar
    orientation="vertical"
    class="flex w-1.5 touch-none p-px transition-opacity select-none hover:w-2"
  >
    <ScrollAreaPrimitive.Thumb class="relative flex-1 rounded-full bg-edge" />
  </ScrollAreaPrimitive.Scrollbar>
</ScrollAreaPrimitive.Root>
