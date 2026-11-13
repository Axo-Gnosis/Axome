<script lang="ts" module>
  import { tv, type VariantProps } from 'tailwind-variants'

  // Sits in text like one wide character: inline, baseline-aligned, wrapping as a unit.
  export const chipVariants = tv({
    base: 'inline-block whitespace-nowrap rounded-sm px-1 align-baseline',
    variants: {
      tone: {
        default: 'bg-raised text-fg',
        accent: 'bg-accent-bg text-accent'
      }
    },
    defaultVariants: { tone: 'default' }
  })

  export type ChipTone = VariantProps<typeof chipVariants>['tone']
</script>

<script lang="ts">
  import { cn } from '$lib/utils/cn'
  import type { Snippet } from 'svelte'
  import type { HTMLAttributes } from 'svelte/elements'

  let {
    tone = 'default',
    class: className,
    children,
    ...rest
  }: HTMLAttributes<HTMLSpanElement> & { tone?: ChipTone; class?: string; children?: Snippet } = $props()
</script>

<span data-slot="chip" class={cn(chipVariants({ tone }), className)} {...rest}
  >{#if children}{@render children()}{/if}</span
>
