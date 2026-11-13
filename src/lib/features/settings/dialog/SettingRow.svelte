<!--
  @component
  SettingRow — one setting: label and optional description, with its control
  at the right (compact controls: switches, short selects, number fields) or
  below (`stacked`: paths, free text, lists).

  Renders a <label> so clicking the text focuses or toggles the control; pass
  `as="div"` when the row holds more than one control.

  @param label - setting name
  @param description - plain text, or a snippet for inline code/links
  @param stacked - put the control under the label at full width
-->

<script lang="ts">
  import { cn } from '$lib/utils/cn'
  import type { Snippet } from 'svelte'

  let {
    label,
    description,
    stacked = false,
    as = 'label',
    class: className,
    children
  }: {
    label: string
    description?: string | Snippet
    stacked?: boolean
    as?: 'label' | 'div'
    class?: string
    children?: Snippet
  } = $props()
</script>

<svelte:element
  this={as}
  data-slot="setting-row"
  class={cn(stacked ? 'flex flex-col gap-2' : 'flex items-center justify-between gap-6', className)}
>
  <div class="flex min-w-0 flex-col gap-0.5">
    <span class="text-base text-fg">{label}</span>
    {#if typeof description === 'function'}
      <span class="text-sm text-muted">{@render description()}</span>
    {:else if description}
      <span class="text-sm text-muted">{description}</span>
    {/if}
  </div>
  {#if children}
    <div class={stacked ? 'flex flex-col gap-2' : 'flex shrink-0 items-center gap-2'}>{@render children()}</div>
  {/if}
</svelte:element>
