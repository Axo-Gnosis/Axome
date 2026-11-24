<script lang="ts">
  import { tick, untrack, type Snippet } from 'svelte'
  import {
    captureScrollAnchor,
    ExpansionAnchor,
    captureToggleAnchor,
    restoreScrollAnchor,
    type ScrollAnchor
  } from './scrollAnchor'
  import { IconButton } from '$lib/components/ui/button'
  import type { Turn } from '$lib/features/chat/turns'
  import { focusAskControl } from '$lib/features/chat/elements/ask/askDocument'
  import { ArrowDown02Icon } from '@hugeicons/core-free-icons'
  import Icon from '$lib/icons/Icon.svelte'
  import { cn } from '$lib/utils/cn'
  import { logicalKey } from '$lib/utils/keyboardEvent'
  import TurnRail from './TurnRail.svelte'

  let {
    turns = [],
    children,
    footer,
    onAskVisibility
  }: {
    turns?: Turn[]
    children?: Snippet
    footer?: Snippet
    onAskVisibility?: (visibleCallIds: string[]) => void
  } = $props()

  let viewport: HTMLDivElement | undefined
  let transcript: HTMLDivElement | undefined
  let atBottom = $state(true)
  let activeTurnId = $state<string | null>(null)
  let viewportWidth = $state(0)
  let viewportHeight = $state(0)
  const showRail = $derived(turns.length > 0 && viewportWidth >= 560 && viewportHeight >= 160)
  // Holds justify-end's top offset during disclosures on short chats so expanding pushes down, not up.
  let lockedSpacer = $state<number | null>(null)
  // Overscroll headroom so Turn 1's end can always rest at the bottom of the viewport like an only message.
  let overscrollTop = $state(16)
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
    if (viewport?.isConnected) settledTop = viewport.scrollTop
  }

  function beginUserScroll() {
    userScrolling = true
    revision++
    clearTimeout(scrollIdle)
    if (!dragging) scrollIdle = setTimeout(endUserScroll, 250)
  }

  function syncOverscroll(): boolean {
    if (!viewport?.isConnected || !transcript?.isConnected) return false
    const firstTurn = turns.length ? transcript.querySelector<HTMLElement>('[data-turn-id]') : null
    const next = firstTurn ? Math.max(16, Math.ceil(viewport.clientHeight - 8 - firstTurn.offsetHeight)) : 16
    if (next === overscrollTop) return false
    overscrollTop = next
    return true
  }

  function syncActiveTurn() {
    if (!turns.length || !viewport?.isConnected || !transcript?.isConnected) {
      activeTurnId = null
      return
    }
    if (atBottom || nearBottom(viewport)) {
      activeTurnId = turns.at(-1)!.id
      return
    }
    // View is bottom-up oriented: the active turn is the last one entering above the bottom edge.
    const threshold = viewport.getBoundingClientRect().bottom - 24
    let next = turns[0].id
    for (const el of transcript.querySelectorAll<HTMLElement>('[data-turn-id]')) {
      const id = el.dataset.turnId
      if (!id) continue
      if (el.getBoundingClientRect().top <= threshold) next = id
      else break
    }
    if (next !== activeTurnId) activeTurnId = next
  }

  function updateAtBottom() {
    if (!viewport?.isConnected || changing) return
    const maxTop = Math.max(0, viewport.scrollHeight - viewport.clientHeight)
    // Our corrections and the clamps they absorbed land on settledTop (or the new maxTop when content shrinks);
    // anything else is navigation (wheel, scrollbar, keys, in-page links, focus scrolling).
    if (
      !userScrolling &&
      (Math.abs(viewport.scrollTop - settledTop) < 1 ||
        (settledTop > maxTop && Math.abs(viewport.scrollTop - maxTop) < 1))
    ) {
      settledTop = viewport.scrollTop
      syncActiveTurn()
      return
    }
    atBottom = nearBottom(viewport)
    anchor = atBottom || !transcript ? null : captureScrollAnchor(viewport, transcript)
    expansionAnchor.reset()
    beginUserScroll()
    syncActiveTurn()
  }

  // Layout changes are not user scrolling. Ignore the scroll event caused by our correction.
  function restorePosition() {
    if (!viewport?.isConnected || userScrolling) return
    if (anchor) restoreScrollAnchor(viewport, anchor)
    else if (atBottom) viewport.scrollTop = viewport.scrollHeight
    settledTop = viewport.scrollTop
  }

  function finishPreserveScroll(scroller: HTMLElement, content: HTMLElement, current: number) {
    if (current !== revision || viewport !== scroller || transcript !== content || userScrolling) return
    restorePosition()
    // The toggle held its header; if that left the view resting at the bottom, keep following new output.
    if (lockedSpacer !== null) {
      if (scroller.scrollHeight <= scroller.clientHeight + 0.5) {
        lockedSpacer = null
        anchor = null
        atBottom = true
        expansionAnchor.reset()
      }
      return
    }
    if (nearBottom(scroller)) {
      anchor = null
      atBottom = true
      expansionAnchor.reset()
    }
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
    if (lockedSpacer === null) {
      const first = transcript.firstElementChild as HTMLElement | null
      if (first) {
        const basePad = parseFloat(getComputedStyle(transcript).paddingTop) || 0
        const marginTop = parseFloat(getComputedStyle(first).marginTop) || 0
        const offset = first.getBoundingClientRect().top - transcript.getBoundingClientRect().top - marginTop
        if (offset > basePad + 0.5) lockedSpacer = offset
      }
    }
    if (target) {
      anchor = captureToggleAnchor(viewport, target)
      expansionAnchor.reset()
    } else {
      anchor = expansionAnchor.capture(viewport, transcript)
    }
    atBottom = false
    changing = true
    const current = ++revision
    let animations: Animation[] = []
    try {
      change()
      await tick()
      if (current === revision && viewport === scroller && transcript === content) {
        restorePosition()
        animations = Array.from(scroller.querySelectorAll('[data-disclose]'))
          .flatMap((el) => el.getAnimations?.() ?? [])
          .filter((animation) => Number.isFinite(animation.effect?.getComputedTiming().endTime))
        if (!animations.length) {
          finishPreserveScroll(scroller, content, current)
        }
      }
    } finally {
      if (current === revision) changing = false
    }
    if (animations.length) {
      await Promise.allSettled(animations.map((animation) => animation.finished))
      finishPreserveScroll(scroller, content, current)
    }
  }

  export function scrollToBottom(): void {
    endUserScroll()
    expansionAnchor.reset()
    lockedSpacer = null
    anchor = null
    atBottom = true
    if (viewport) restorePosition()
    syncActiveTurn()
  }

  function scrollToTurn(turnId: string): void {
    if (!viewport?.isConnected || !transcript?.isConnected) return
    const target = transcript.querySelector<HTMLElement>(`[data-turn-id="${CSS.escape(turnId)}"]`)
    if (!target) return
    const delta = target.getBoundingClientRect().bottom - (viewport.getBoundingClientRect().bottom - 8)
    const maxTop = Math.max(0, viewport.scrollHeight - viewport.clientHeight)
    const top = Math.max(0, Math.min(maxTop, viewport.scrollTop + delta))
    expansionAnchor.reset()
    lockedSpacer = null
    anchor = null
    atBottom = false
    beginUserScroll()
    activeTurnId = turnId
    viewport.scrollTo({ top, behavior: 'smooth' })
  }

  export function jumpToAsk(callId: string): void {
    const scroller = viewport
    const content = transcript
    if (!scroller?.isConnected || !content?.isConnected) return
    const target = Array.from(content.querySelectorAll<HTMLElement>('[data-ask-call-id]')).find(
      (node) => node.dataset.askCallId === callId
    )
    if (!target) return
    expansionAnchor.reset()
    lockedSpacer = null
    anchor = null
    atBottom = false
    changing = false
    beginUserScroll()
    const current = revision
    void tick().then(() => {
      if (
        current !== revision ||
        viewport !== scroller ||
        !content.contains(target) ||
        target.dataset.askCallId !== callId
      )
        return
      const delta = target.getBoundingClientRect().top - scroller.getBoundingClientRect().top
      const reducedMotion = window.matchMedia('(prefers-reduced-motion: reduce)').matches
      scroller.scrollTo({ top: scroller.scrollTop + delta, behavior: reducedMotion ? 'instant' : 'smooth' })
      if (!focusAskControl(target)) target.focus({ preventScroll: true })
      updateAtBottom()
    })
  }

  function observeAsks(content: HTMLDivElement) {
    if (!onAskVisibility) return
    const scroller = content.parentElement?.parentElement
    if (!scroller) return
    let rows: HTMLElement[] = []
    let reported: string[] = []
    const visible = new Set<Element>()
    function report() {
      const next = rows.filter((node) => visible.has(node)).map((node) => node.dataset.askCallId!)
      if (next.length === reported.length && next.every((id, index) => id === reported[index])) return
      reported = next
      onAskVisibility?.(next)
    }
    const observer = new IntersectionObserver(
      (entries) => {
        for (const entry of entries) {
          if (entry.isIntersecting && content.contains(entry.target)) visible.add(entry.target)
          else visible.delete(entry.target)
        }
        report()
      },
      { root: scroller }
    )
    function reconcile() {
      const next = Array.from(content.querySelectorAll<HTMLElement>('[data-ask-call-id]'))
      const seen = new Set(next)
      const previous = new Set(rows)
      for (const node of rows) {
        if (!seen.has(node)) {
          visible.delete(node)
          observer.unobserve(node)
        }
      }
      for (const node of next) {
        if (!previous.has(node)) observer.observe(node)
      }
      rows = next
      report()
    }
    const mutations = new MutationObserver(reconcile)
    mutations.observe(content, {
      childList: true,
      subtree: true,
      attributes: true,
      attributeFilter: ['data-ask-call-id']
    })
    reconcile()
    return () => {
      mutations.disconnect()
      observer.disconnect()
    }
  }

  $effect(() => {
    void turns
    void tick().then(() => {
      const resized = syncOverscroll()
      if (resized) void tick().then(restorePosition)
      syncActiveTurn()
    })
  })

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
    untrack(() => {
      viewportWidth = scroller.clientWidth
      viewportHeight = scroller.clientHeight
      syncOverscroll()
      scrollToBottom()
    })
    const observer = new ResizeObserver(() => {
      if (active && viewport === scroller) {
        viewportWidth = scroller.clientWidth
        viewportHeight = scroller.clientHeight
        const resized = syncOverscroll()
        if (resized) {
          void tick().then(() => {
            if (active && viewport === scroller) {
              restorePosition()
              syncActiveTurn()
            }
          })
        } else {
          restorePosition()
          syncActiveTurn()
        }
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
      lockedSpacer = null
      anchor = null
      settledTop = Number.NaN
    }
  }
</script>

<!-- Scroller sits strictly above the composer so messages clip cleanly at its top pixel with no opaque backdrop. -->
<div data-slot="conversation" role="region" aria-label="Conversation" class="flex min-h-0 flex-1 flex-col">
  <div class="relative flex min-h-0 flex-1 flex-col">
    <div
      bind:this={viewport}
      class={cn(
        'min-h-0 flex-1 scrollbar-none overflow-y-auto px-4 [overflow-anchor:none]',
        showRail && viewportWidth < 1080 && 'pl-7'
      )}
      onscroll={updateAtBottom}
    >
      <div class="mx-auto flex min-h-full w-full max-w-5xl flex-col" {@attach pinToBottom}>
        <div
          bind:this={transcript}
          {@attach observeAsks}
          class={cn('flex flex-1 flex-col gap-4 pt-4 pb-2', lockedSpacer === null && 'justify-end')}
          style:padding-top={`${lockedSpacer ?? overscrollTop}px`}
        >
          {#if children}{@render children()}{/if}
        </div>
      </div>
    </div>
    {#if showRail}
      <TurnRail {turns} {activeTurnId} onJump={scrollToTurn} />
    {/if}
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
