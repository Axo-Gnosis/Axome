<!-- Global title-bar tab button with active styling, beam, and close affordance. -->
<script lang="ts">
  import TabBeam, { POSITION_CLASS, type BeamPosition } from '$lib/components/ui/TabBeam.svelte'
  import { cn } from '$lib/utils/cn'
  import { XIcon } from '@hugeicons/core-free-icons'
  import Icon from '$lib/icons/Icon.svelte'
  import type { Snippet } from 'svelte'
  import type { HTMLButtonAttributes } from 'svelte/elements'

  type CloseEvent = MouseEvent | KeyboardEvent

  let {
    active = false,
    position = 'top',
    color,
    leading,
    trailing,
    onClose,
    onauxclick,
    class: className,
    children,
    ...rest
  }: HTMLButtonAttributes & {
    active?: boolean
    position?: BeamPosition
    color?: string
    leading?: Snippet
    /** Status shown in the close button's slot; hover swaps it for the close control. */
    trailing?: Snippet
    onClose?: (event: CloseEvent) => void | Promise<void>
    class?: string
    children?: Snippet
  } = $props()

  function handleClose(event: CloseEvent) {
    event.stopPropagation()
    if (onClose) void onClose(event)
  }

  // Keep the native listener bound when locking removes the consumer callback:
  // optional spread handlers can leave Svelte's cached listener calling undefined.
</script>

<button
  class={cn(
    'group relative flex h-full shrink-0 cursor-pointer items-center gap-1.5 border-none px-3 text-sm focus-visible:shadow-focus-ring focus-visible:outline-none',
    active ? 'bg-raised text-bright' : 'bg-transparent text-muted hover:bg-raised/50 hover:text-bright',
    className
  )}
  role="tab"
  aria-selected={active}
  onauxclick={(event) => onauxclick?.(event)}
  {...rest}
>
  {#if active}
    <TabBeam {position} {color} />
  {:else if color}
    <span
      class={cn(
        'pointer-events-none absolute overflow-hidden transition-opacity duration-150',
        POSITION_CLASS[position],
        'opacity-40 group-hover:opacity-75'
      )}
      style="background-color: {color};"
      aria-hidden="true"
    ></span>
  {/if}
  {#if leading}{@render leading()}{/if}
  {#if children}{@render children()}{/if}
  {#if onClose || trailing}
    <span class="relative -mr-1 flex size-4 shrink-0 items-center justify-center">
      {#if trailing}
        <span class={cn('flex items-center justify-center', onClose && 'group-hover:opacity-0')}
          >{@render trailing()}</span
        >
      {/if}
      {#if onClose}
        <span
          class="absolute inset-0 flex items-center justify-center text-xs leading-none text-muted opacity-0 transition-all group-hover:opacity-100 hover:text-danger"
          role="button"
          tabindex="0"
          onclick={handleClose}
          onkeydown={(event) => event.key === 'Enter' && handleClose(event)}
        >
          <Icon icon={XIcon} size={12} />
        </span>
      {/if}
    </span>
  {/if}
</button>
