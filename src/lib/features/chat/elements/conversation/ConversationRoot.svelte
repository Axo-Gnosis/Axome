<script lang="ts">
  import { tick, type Snippet } from 'svelte'
  import {
    captureScrollAnchor,
    ExpansionAnchor,
    captureToggleAnchor,
    restoreScrollAnchor,
    type ScrollAnchor
  } from './scrollAnchor'
  import { IconButton } from '$lib/components/ui/button'
  import { ArrowDown02Icon } from '@hugeicons/core-free-icons'
  import Icon from '$lib/icons/Icon.svelte'
  import { cn } from '$lib/utils/cn'
  import { logicalKey } from '$lib/utils/keyboardEvent'

  let { children, footer }: { children?: Snippet; footer?: Snippet } = $props()

  let viewport: HTMLDivElement | undefined
  let transcript: HTMLDivElement | undefined
  let atBottom = $state(true)
  let anchor: ScrollAnchor | null = null
  const expansionAnchor = new ExpansionAnchor()
  let changing = false
  let revision = 0
  let userScrolling = false
  let dragging = false
  let scrollIdle: ReturnType<typeof setTimeout> | undefined
  // Where our own correction (or browser clamping it absorbed) left scrollTop; scroll events landing elsewhere are navigation.
  let settledTop = Number.NaN

  function nearBottom(scroller: HTMLElement): boolean {
    return scroller.scrollHeight - scroller.scrollTop - scroller.clientHeight <= 32
  }

  function endUserScroll() {
    clearTimeout(scrollIdle)
    userScrolling = false
    dragging = false
  }

  function beginUserScroll() {
    userScrolling = true
    clearTimeout(scrollIdle)
    if (!dragging) scrollIdle = setTimeout(endUserScroll, 250)
  }

  function updateAtBottom() {
    if (!viewport?.isConnected || changing) return
    // Our corrections and the clamps they absorbed land exactly on settledTop; anything else is navigation
    // (wheel, scrollbar, keys, in-page links, focus scrolling).
    if (!userScrolling && Math.abs(viewport.scrollTop - settledTop) < 1) return
    atBottom = nearBottom(viewport)
    anchor = atBottom || !transcript ? null : captureScrollAnchor(viewport, transcript)
    expansionAnchor.reset()
    beginUserScroll()
  }

  // Layout changes are not user scrolling. Ignore the scroll event caused by our correction.
  function restorePosition() {
    if (!viewport?.isConnected || userScrolling) return
    if (anchor) restoreScrollAnchor(viewport, anchor)
    else if (atBottom) viewport.scrollTop = viewport.scrollHeight
    settledTop = viewport.scrollTop
  }

  /** Toggling changes layout, not reading position. Reuse the bulk anchor until the user scrolls. */
  export async function preserveScroll(change: () => void, stepId?: string): Promise<void> {
    if (!viewport?.isConnected || !transcript?.isConnected) {
      change()
      return
    }
    const scroller = viewport
    const content = transcript
    const target = stepId ? transcript.querySelector<HTMLElement>(`[data-view-id="${CSS.escape(stepId)}"]`) : null
    endUserScroll()
    if (target) {
      anchor = captureToggleAnchor(viewport, target)
      expansionAnchor.reset()
    } else {
      anchor = expansionAnchor.capture(viewport, transcript)
    }
    atBottom = false
    changing = true
    const current = ++revision
    try {
      change()
      await tick()
      if (current === revision && viewport === scroller && transcript === content) {
        restorePosition()
        // The toggle held its header; if that left the view resting at the bottom, keep following new output.
        if (nearBottom(scroller)) {
          anchor = null
          atBottom = true
          expansionAnchor.reset()
        }
      }
    } finally {
      if (current === revision) changing = false
    }
  }

  export function scrollToBottom(): void {
    endUserScroll()
    expansionAnchor.reset()
    anchor = null
    atBottom = true
    if (viewport) restorePosition()
  }

  export function scrollPage(direction: 1 | -1): void {
    if (!viewport?.isConnected) return
    beginUserScroll()
    viewport.scrollBy({ top: direction * viewport.clientHeight * 0.9 })
    updateAtBottom()
  }

  // Watches the whole column, so a growing transcript or composer keeps a following view pinned.
  function pinToBottom(column: HTMLDivElement) {
    // Child attachments can run before the parent's bind:this assignment.
    const scroller = column.parentElement
    if (!scroller) return
    let active = true
    const events = new AbortController()
    const options = { passive: true, capture: true, signal: events.signal }
    scroller.addEventListener('wheel', beginUserScroll, options)
    scroller.addEventListener(
      'pointerdown',
      (event) => {
        // Touch panning and scrollbar dragging, not clicks on expansion buttons.
        if (event.pointerType !== 'touch' && event.target !== scroller) return
        dragging = true
        beginUserScroll()
      },
      options
    )
    const release = () => {
      if (!dragging) return
      dragging = false
      beginUserScroll()
    }
    window.addEventListener('pointerup', release, options)
    window.addEventListener('pointercancel', release, options)
    scroller.addEventListener(
      'keydown',
      (event) => {
        if (event.isComposing || event.altKey || event.ctrlKey || event.metaKey) return
        const key = logicalKey(event)
        if (!['ArrowUp', 'ArrowDown', 'PageUp', 'PageDown', 'Home', 'End', ' '].includes(key)) return
        const target = event.target instanceof Element ? event.target : null
        if (target?.closest('input, textarea, select, [contenteditable="true"]')) return
        if (key === ' ' && target?.closest('button, a, [role="button"]')) return
        beginUserScroll()
      },
      options
    )
    scrollToBottom()
    const observer = new ResizeObserver(() => {
      if (active && viewport === scroller) {
        restorePosition()
      }
    })
    observer.observe(column)
    observer.observe(scroller)
    return () => {
      active = false
      observer.disconnect()
      events.abort()
      endUserScroll()
      expansionAnchor.reset()
      revision++
      changing = false
      anchor = null
      settledTop = Number.NaN
    }
  }
</script>

<!-- Scroller sits strictly above the composer so messages clip cleanly at its top pixel with no opaque backdrop. -->
<div data-slot="conversation" role="region" aria-label="Conversation" class="flex min-h-0 flex-1 flex-col">
  <div
    bind:this={viewport}
    class="min-h-0 flex-1 scrollbar-none overflow-y-auto px-4 [overflow-anchor:none]"
    onscroll={updateAtBottom}
  >
    <div class="mx-auto flex min-h-full w-full max-w-5xl flex-col" {@attach pinToBottom}>
      <div bind:this={transcript} class="flex flex-1 flex-col justify-end gap-4 pt-4 pb-2">
        {#if children}{@render children()}{/if}
      </div>
    </div>
  </div>

  <div class="shrink-0 px-4">
    <!-- Isolate negative layers so the scroll control's shadow stays behind the composer, not the page. -->
    <div
      class={cn(
        'before:duration-fast relative isolate mx-auto w-full max-w-5xl pb-4 before:pointer-events-none before:absolute before:inset-x-0 before:bottom-full before:-z-10 before:h-8 before:bg-linear-to-t before:from-ground before:from-20% before:to-transparent before:transition-opacity',
        atBottom && 'before:opacity-0'
      )}
    >
      {#if !atBottom}
        <IconButton
          size="md"
          tooltip="Scroll to Bottom"
          tooltipSide="top"
          onclick={scrollToBottom}
          class="absolute bottom-full left-1/2 -z-10 mb-2 -translate-x-1/2 border border-edge bg-raised shadow-popover"
        >
          <Icon icon={ArrowDown02Icon} size={14} />
        </IconButton>
      {/if}
      {#if footer}{@render footer()}{/if}
    </div>
  </div>
</div>
