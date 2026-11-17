<script lang="ts">
  import type { Turn } from '$lib/features/chat/turns'
  import { cn } from '$lib/utils/cn'
  import { turnPreview } from './turnRail'

  let {
    turns,
    activeTurnId,
    onJump
  }: {
    turns: Turn[]
    activeTurnId: string | null
    onJump: (turnId: string) => void
  } = $props()

  let navEl = $state<HTMLElement>()
  let hovered = $state<{ index: number; centerY: number; navHeight: number } | null>(null)

  const preview = $derived(hovered && turns[hovered.index] ? turnPreview(turns[hovered.index]) : null)
  // Keep the popover aligned with the hovered dot without clipping near the top/bottom edges.
  const clampedTop = $derived(hovered ? Math.max(44, Math.min(hovered.navHeight - 44, hovered.centerY)) : 0)

  function show(index: number, button: HTMLElement) {
    if (!navEl) return
    const navRect = navEl.getBoundingClientRect()
    const btnRect = button.getBoundingClientRect()
    hovered = {
      index,
      centerY: btnRect.top - navRect.top + btnRect.height / 2,
      navHeight: navRect.height
    }
  }
</script>

<nav
  bind:this={navEl}
  aria-label="Turn navigation"
  class="pointer-events-none absolute inset-y-3 left-1.5 z-20 flex items-center"
>
  <div class="pointer-events-auto flex max-h-full scrollbar-none flex-col items-center overflow-y-auto py-1">
    {#each turns as turn, index (turn.id)}
      {@const active = turn.id === activeTurnId}
      {@const isHovered = hovered?.index === index}
      <button
        type="button"
        aria-label={`Jump to turn ${index + 1} of ${turns.length}`}
        aria-current={active ? 'location' : undefined}
        class="group flex h-3.5 w-4 shrink-0 cursor-pointer items-center justify-center rounded-full focus-visible:shadow-focus-ring focus-visible:outline-none"
        onmouseenter={(event) => show(index, event.currentTarget)}
        onmouseleave={() => {
          if (hovered?.index === index) hovered = null
        }}
        onfocus={(event) => show(index, event.currentTarget)}
        onblur={() => {
          if (hovered?.index === index) hovered = null
        }}
        onclick={() => onJump(turn.id)}
      >
        <span
          class={cn(
            'block rounded-full',
            active
              ? 'size-2 bg-bright'
              : isHovered
                ? 'size-2 bg-muted'
                : 'size-1.5 bg-subtle/80 group-hover:size-2 group-hover:bg-muted'
          )}
        ></span>
      </button>
    {/each}
  </div>

  {#if hovered && preview}
    <div
      role="tooltip"
      style:top={`${clampedTop}px`}
      class="pointer-events-none absolute left-6 w-72 max-w-[min(18rem,calc(100vw-6rem))] -translate-y-1/2 rounded-xl border border-edge-strong bg-surface/95 px-3.5 py-2.5 shadow-popover backdrop-blur-sm"
    >
      <p class="line-clamp-2 text-sm leading-snug font-semibold text-bright">{preview.user}</p>
      {#if preview.agent}
        <p class="mt-1 line-clamp-3 text-sm leading-snug text-fg/90">{preview.agent}</p>
      {/if}
      <div class="mt-2 flex items-center justify-between gap-3 text-xs text-muted tabular-nums">
        <span>{hovered.index + 1} of {turns.length}</span>
        {#if preview.time}
          <span>{preview.time}</span>
        {/if}
      </div>
    </div>
  {/if}
</nav>
