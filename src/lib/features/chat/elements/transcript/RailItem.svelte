<!--
  @component
  RailItem — one entry on a work block's rail. Its node (icon, or a pip without one)
  sits over the rail line on a `bg-ground` box so the line breaks behind it. Nested
  entries (inside a tool group's sub-list) have no node.
-->

<script lang="ts">
  import type { Snippet } from 'svelte'
  import type { StatusTone } from '$lib/components/ui/status-dot'
  import Icon, { type IconData } from '$lib/icons/Icon.svelte'
  import { cn } from '$lib/utils/cn'

  let {
    icon,
    tone = 'idle',
    nested = false,
    viewId,
    children
  }: {
    icon?: IconData
    tone?: StatusTone
    nested?: boolean
    viewId?: string
    children: Snippet
  } = $props()
</script>

<div data-slot="rail-item" data-view-id={viewId} class={cn('relative min-w-0', nested ? 'pl-0' : 'pl-6')}>
  {#if !nested}
    <span aria-hidden="true" class="absolute top-1 left-0 flex h-4 w-3.75 items-center justify-center bg-ground">
      {#if icon}
        <Icon
          {icon}
          size={12}
          class={tone === 'working' ? 'text-accent' : tone === 'danger' ? 'text-danger' : 'text-muted'}
        />
      {:else}
        <span class="size-1 rounded-full bg-muted"></span>
      {/if}
    </span>
  {/if}
  {@render children()}
</div>
