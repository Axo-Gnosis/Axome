<script lang="ts" module>
  import { tv } from 'tailwind-variants'

  export const commandGroupHeadingVariants = tv({
    base: 'px-2 py-1.5 text-xs font-medium text-muted'
  })
</script>

<script lang="ts">
  import { Command } from 'bits-ui'
  import { cn } from '$lib/utils/cn'
  import type { Snippet } from 'svelte'

  let {
    class: className,
    heading,
    headingContent,
    children,
    ...rest
  }: {
    class?: string
    heading?: string
    headingContent?: Snippet
    children?: Snippet
    value?: string
    forceMount?: boolean
  } = $props()
</script>

<Command.Group class={cn('overflow-hidden p-1', className)} {...rest}>
  {#if heading || headingContent}
    <Command.GroupHeading class={commandGroupHeadingVariants()}>
      {#if headingContent}{@render headingContent()}{:else}{heading}{/if}
    </Command.GroupHeading>
  {/if}
  <Command.GroupItems>
    {#if children}{@render children()}{/if}
  </Command.GroupItems>
</Command.Group>
