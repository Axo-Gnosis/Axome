<script lang="ts">
  import { TidalStipple } from '$lib/components/ui/tidal-stipple'
  import type { Snippet } from 'svelte'

  let {
    children,
    footer
  }: {
    children: Snippet
    /** Sits at the stage bottom while content fits, after the content once it scrolls. */
    footer?: Snippet
  } = $props()
</script>

<!-- Content centers while it fits and scrolls once it outgrows the stage. -->
<div class="relative isolate flex flex-1 flex-col overflow-hidden bg-ground">
  <TidalStipple class="-z-10" />

  <div class="flex flex-1 flex-col overflow-y-auto">
    <div class="m-auto w-full max-w-2xl px-8 py-10">
      {@render children()}
    </div>

    {#if footer}
      <div class="flex shrink-0 justify-center px-8 pb-6">{@render footer()}</div>
    {/if}
  </div>
</div>
