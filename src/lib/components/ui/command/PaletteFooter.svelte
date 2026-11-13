<script lang="ts" module>
  export interface PaletteHint {
    keys: readonly string[]
    label: string
  }
</script>

<script lang="ts">
  import { Kbd } from '$lib/components/ui/kbd'
  import { cn } from '$lib/utils/cn'
  import type { HTMLAttributes } from 'svelte/elements'

  let {
    class: className,
    hints = [],
    navigate = true,
    dismiss = 'close',
    children,
    ...rest
  }: HTMLAttributes<HTMLDivElement> & {
    hints?: readonly PaletteHint[]
    navigate?: boolean
    dismiss?: string | null
  } = $props()

  const allHints = $derived<readonly PaletteHint[]>(
    navigate ? [{ keys: ['↑', '↓'], label: 'navigate' }, ...hints] : hints
  )
</script>

<div
  data-slot="palette-footer"
  class={cn(
    'flex shrink-0 items-center justify-between gap-4 border-t border-edge bg-surface/60 px-3 py-2 text-xs text-muted',
    className
  )}
  {...rest}
>
  {#if children}
    {@render children()}
  {:else}
    <div class="flex flex-wrap items-center gap-x-3 gap-y-1.5">
      {#each allHints as hint (hint.label + hint.keys.join('+'))}
        <span class="flex items-center gap-1.5">
          {#each hint.keys as key, idx (idx)}<Kbd>{key}</Kbd>{/each}
          {hint.label}
        </span>
      {/each}
    </div>
    {#if dismiss}
      <span class="flex shrink-0 items-center gap-1.5"><Kbd>Esc</Kbd>{dismiss}</span>
    {/if}
  {/if}
</div>
