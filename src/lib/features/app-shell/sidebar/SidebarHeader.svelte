<script lang="ts">
  import type { Snippet } from 'svelte'

  let {
    title,
    actions
  }: {
    title: string
    actions?: Snippet
  } = $props()
</script>

<div class="flex h-8 min-h-8 shrink-0 items-center justify-between border-b border-edge bg-ground pr-1.5 pl-2.5">
  <span class="min-w-0 truncate text-xs font-semibold tracking-wide text-muted uppercase" {title}>{title}</span>
  <div class="flex shrink-0 items-center gap-1">
    {#if actions}<div class="flex items-center gap-0.5">{@render actions()}</div>{/if}
  </div>
</div>
