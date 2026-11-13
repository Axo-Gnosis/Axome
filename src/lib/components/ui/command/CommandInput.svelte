<script lang="ts">
  import { Command } from 'bits-ui'
  import { cn } from '$lib/utils/cn'
  import PaletteSearch from './PaletteSearch.svelte'

  let {
    class: className,
    value = $bindable(''),
    ref = $bindable(null),
    ...rest
  }: Omit<Command.InputProps, 'child' | 'children'> = $props()
</script>

<PaletteSearch>
  {#snippet children(inputClass)}
    <Command.Input class={cn(inputClass, className)} bind:value bind:ref {...rest} />
  {/snippet}
</PaletteSearch>
