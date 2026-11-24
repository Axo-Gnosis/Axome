<!--
  @component
  Slider — bits-ui single-value slider with Axome tokens: a pill track in
  `surface`, the filled part in `accent-dim`, a `max` thumb. `ticks` dots
  every step, for discrete scales (e.g. effort levels).
-->

<script lang="ts">
  import { Slider as SliderPrimitive } from 'bits-ui'
  import { cn } from '$lib/utils/cn'

  let {
    value = $bindable(0),
    min = 0,
    max = 100,
    step = 1,
    ticks = false,
    disabled = false,
    onValueChange,
    class: className,
    'aria-label': ariaLabel
  }: {
    value?: number
    min?: number
    max?: number
    step?: number
    ticks?: boolean
    disabled?: boolean
    onValueChange?: (value: number) => void
    class?: string
    'aria-label'?: string
  } = $props()
</script>

<SliderPrimitive.Root
  type="single"
  data-slot="slider"
  bind:value
  {min}
  {max}
  {step}
  {onValueChange}
  {disabled}
  class={cn(
    'relative flex h-5 w-full cursor-pointer touch-none items-center select-none data-disabled:pointer-events-none data-disabled:opacity-50',
    className
  )}
>
  {#snippet children({ thumbItems, tickItems })}
    <span class="relative h-full w-full grow overflow-hidden rounded-full bg-surface">
      <SliderPrimitive.Range class="absolute h-full bg-accent-dim" />
    </span>
    {#if ticks}
      {#each tickItems as { index } (index)}
        <SliderPrimitive.Tick
          {index}
          class="pointer-events-none size-1 rounded-full bg-subtle data-bounded:bg-ground/40"
        />
      {/each}
    {/if}
    {#each thumbItems as { index } (index)}
      <SliderPrimitive.Thumb
        {index}
        aria-label={ariaLabel}
        class="block size-5 rounded-full border border-edge bg-max transition-colors focus-visible:shadow-focus-ring focus-visible:outline-none"
      />
    {/each}
  {/snippet}
</SliderPrimitive.Root>
