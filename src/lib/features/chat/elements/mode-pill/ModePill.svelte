<!--
  @component
  ModePill — the composer's mode indicator. Shows the selected mode; until the next
  model request picks it up (or, after exiting, drops it) the pill reads as pending.

  @param mode - the selected mode; null when none is selected
  @param effectiveMode - the mode the current model request runs under
  @param onExit - leaves the selected mode
-->
<script lang="ts">
  import { IconButton } from '$lib/components/ui/button'
  import { Chip } from '$lib/components/ui/chip'
  import * as Tooltip from '$lib/components/ui/tooltip'
  import Icon from '$lib/icons/Icon.svelte'
  import { XIcon } from '@hugeicons/core-free-icons'

  let {
    mode,
    effectiveMode,
    onExit
  }: {
    mode: string | null
    effectiveMode: string | null
    onExit: () => void
  } = $props()

  const shown = $derived(mode ?? effectiveMode)
  // Entering or leaving mid-run takes effect at the next model request.
  const pending = $derived(
    mode === null
      ? 'Ends with the next model request'
      : mode !== effectiveMode
        ? 'Starts with the next model request'
        : null
  )
</script>

{#if shown !== null}
  <div data-slot="mode-pill" class="flex min-w-0 items-center gap-0.5">
    {#if pending}
      <Tooltip.Root>
        <Tooltip.Trigger>
          {#snippet child({ props })}
            <Chip
              {...props}
              tabindex={0}
              class="max-w-40 truncate text-sm focus-visible:shadow-focus-ring focus-visible:outline-none">{shown}</Chip
            >
          {/snippet}
        </Tooltip.Trigger>
        <Tooltip.Content side="top">{pending}</Tooltip.Content>
      </Tooltip.Root>
    {:else}
      <Chip tone="accent" class="max-w-40 truncate text-sm">{shown}</Chip>
    {/if}
    {#if mode !== null}
      <IconButton tooltip="Exit Mode" tooltipSide="top" onclick={onExit}>
        <Icon icon={XIcon} size={12} />
      </IconButton>
    {/if}
  </div>
{/if}
