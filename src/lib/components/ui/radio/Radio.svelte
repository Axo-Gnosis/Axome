<!--
  @component
  Radio — native radio input with Axome design tokens.

  A 14×14 circle that matches `Checkbox`: accent border and dot when checked.
  Native, so same-`name` radios keep the browser's arrow-key group behavior.
-->

<script lang="ts">
  import { cn } from '$lib/utils/cn'
  import type { HTMLInputAttributes } from 'svelte/elements'

  let {
    class: className,
    checked = false,
    ref = $bindable<HTMLInputElement | null>(null),
    ...rest
  }: Omit<HTMLInputAttributes, 'type' | 'checked'> & {
    class?: string
    checked?: boolean
    ref?: HTMLInputElement | null
  } = $props()
</script>

<span data-slot="radio" class={cn('relative inline-flex size-3.5 shrink-0', className)}>
  <input
    type="radio"
    {checked}
    bind:this={ref}
    class="peer size-3.5 cursor-pointer appearance-none rounded-full border border-edge bg-surface transition-colors checked:border-accent hover:border-accent focus-visible:shadow-focus-ring focus-visible:outline-none disabled:pointer-events-none disabled:opacity-50"
    {...rest}
  />
  <span
    aria-hidden="true"
    class="pointer-events-none absolute inset-1 rounded-full bg-accent opacity-0 peer-checked:opacity-100"
  ></span>
</span>
