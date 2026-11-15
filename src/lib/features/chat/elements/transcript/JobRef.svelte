<script lang="ts">
  import { Chip, chipVariants } from '$lib/components/ui/chip'
  import { cn } from '$lib/utils/cn'

  let {
    ref,
    member = false,
    onOpen
  }: {
    ref: string
    member?: boolean
    onOpen?: (ref?: string) => void
  } = $props()

  function stripRefSuffix(ref: string): string {
    return ref.replace(/\/\d+$/, '')
  }

  const strippedRef = $derived(stripRefSuffix(ref))
  const clickable = $derived(
    strippedRef.startsWith('bg_') ||
      strippedRef.startsWith('agent_') ||
      strippedRef.startsWith('team_') ||
      (member && strippedRef.length > 0)
  )
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
