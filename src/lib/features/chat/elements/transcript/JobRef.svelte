<script lang="ts">
  import { Chip, chipVariants } from '$lib/components/ui/chip'
  import { cn } from '$lib/utils/cn'
  import { isJobId, stripRef } from '../../jobs'

  let {
    ref,
    member = false,
    onOpen
  }: {
    ref: string
    member?: boolean
    onOpen?: (ref?: string) => void
  } = $props()

  const strippedRef = $derived(stripRef(ref))
  const clickable = $derived(isJobId(strippedRef) || (member && strippedRef.length > 0))
</script>

{#if clickable && onOpen}
  <button
    type="button"
    class={cn(
      chipVariants(),
      'cursor-pointer font-mono text-xs hover:text-bright focus-visible:shadow-focus-ring focus-visible:outline-none'
    )}
    onclick={() => onOpen(strippedRef)}>{ref}</button
  >
{:else}
  <Chip class="font-mono text-xs">{ref}</Chip>
{/if}
