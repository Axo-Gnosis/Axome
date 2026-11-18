<!--
  @component
  RailRow — one-line rail entry: verb · target · aside. Opened content sits under
  the row in a single bordered box (or unboxed with `bare`). The verb is the
  keyboard toggle; an unselected click anywhere in the row or its content toggles too.
-->

<script lang="ts">
  import type { Snippet } from 'svelte'
  import { Button } from '$lib/components/ui/button'
  import type { IconData } from '$lib/icons/Icon.svelte'
  import { cn } from '$lib/utils/cn'
  import { disclose } from './disclose'
  import type { StatusTone } from '$lib/components/ui/status-dot'
  import RailItem from './RailItem.svelte'

  let {
    icon,
    tone = 'idle',
    nested = false,
    viewId,
    verb,
    mono = false,
    open = false,
    label,
    onToggle,
    bare = false,
    target,
    aside,
    children
  }: {
    icon?: IconData
    tone?: StatusTone
    nested?: boolean
    viewId?: string
    verb: string
    mono?: boolean
    open?: boolean
    label?: string
    onToggle?: () => void
    /** Children render unboxed (a group's sub-list). */
    bare?: boolean
    target?: Snippet
    aside?: Snippet
    children?: Snippet
  } = $props()

  const verbClass = $derived(
    cn(
      'shrink-0 text-sm',
      tone === 'working' ? 'text-accent' : tone === 'danger' ? 'text-danger-bright' : 'text-fg',
      mono && 'font-mono'
    )
  )

  function toggleFromContent(event: MouseEvent) {
    const target = event.target
    if (
      window.getSelection()?.toString() ||
      !(target instanceof Element) ||
      target.closest('a, button, input, textarea, select, label, form, [contenteditable], [role="button"]') ||
      // A nested call owns its own clicks; its group only toggles from its row or the gaps between calls.
      target.closest('[data-slot="rail-row"]') !== event.currentTarget
    ) {
      return
    }
    onToggle?.()
  }
</script>

<RailItem {icon} {tone} {nested} {viewId}>
  <!-- The verb button provides keyboard access; the row and its content also accept an unselected click. -->
  <!-- svelte-ignore a11y_no_static_element_interactions, a11y_click_events_have_key_events -->
  <div data-slot="rail-row" class="min-w-0" onclick={onToggle ? toggleFromContent : undefined}>
    <div class={cn('flex min-h-6 min-w-0 items-center gap-2 text-sm text-muted', onToggle && 'cursor-pointer')}>
      {#if onToggle}
        <Button
          variant="text"
          size="xs"
          class={cn(verbClass, 'p-0 font-normal transition-none')}
          aria-expanded={open}
          aria-label={label}
          onclick={onToggle}>{verb}</Button
        >
      {:else}
        <span class={verbClass}>{verb}</span>
      {/if}
      {#if target}
        <div class={cn('min-w-0 flex-1 truncate font-mono text-xs', nested && 'text-fg')}>{@render target()}</div>
      {/if}
      {#if aside}
        <div class="ms-auto flex shrink-0 items-center gap-2 font-mono text-xs text-muted">{@render aside()}</div>
      {/if}
    </div>
    {#if open && children}
      {#if bare}
        <div data-disclose transition:disclose class="min-w-0">{@render children()}</div>
      {:else}
        <div
          data-disclose
          transition:disclose
          class={cn(
            'mt-1 mb-1.5 min-w-0 divide-y divide-edge overflow-hidden rounded-md border border-edge bg-surface font-mono text-xs',
            tone === 'danger' && 'border-danger-border'
          )}
        >
          {@render children()}
        </div>
      {/if}
    {/if}
  </div>
</RailItem>
