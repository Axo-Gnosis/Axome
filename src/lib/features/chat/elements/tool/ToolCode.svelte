<script lang="ts">
  import { formatCount } from '$lib/features/chat/format'
  import type { DiffRow } from '$lib/features/chat/lineDiff'
  import { cn } from '$lib/utils/cn'
  import { codeParts, contextHighlights } from './diffHighlight'
  import type { DiffToken } from './diffHighlight'
  import MoreButton from './MoreButton.svelte'

  let {
    rows,
    path,
    folderPath,
    limit,
    onExpand,
    neutral = false,
    preview,
    emphasis
  }: {
    rows: DiffRow[]
    path: string
    folderPath: string
    limit: number | null
    onExpand: () => void
    neutral?: boolean
    preview?: { rows: DiffRow[]; hiddenLines: number; hiddenHunks: number }
    emphasis?: Map<DiffRow, DiffToken[]>
  } = $props()

  const shown = $derived(preview?.rows ?? (limit === null ? rows : rows.slice(0, limit)))
  const gutterWidth = $derived(
    rows.reduce((width, row) => Math.max(width, row.kind === 'gap' ? 0 : String(row.lineNumber ?? '').length), 3)
  )
  const gutters = $derived.by(() => {
    let previous: number | null = null
    return shown.map((row) => {
      if (row.kind === 'gap') {
        previous = null
        return ''
      }
      const number = row.lineNumber === previous ? '' : String(row.lineNumber ?? '')
      previous = row.lineNumber
      return `${row.kind === 'add' ? '+' : row.kind === 'del' ? '-' : ' '}${number}`
    })
  })
  let highlighted = $state.raw<{ rows: DiffRow[]; tokens: Map<number, DiffToken[]> } | null>(null)
  let content = $state<HTMLDivElement>()
  let measured = $state({ height: 0, hiddenLines: 0, hiddenHunks: 0 })
  const hiddenLines = $derived(
    (preview?.hiddenLines ?? rows.length - shown.length) + (limit === null ? 0 : measured.hiddenLines)
  )
  const hiddenHunks = $derived((preview?.hiddenHunks ?? 0) + (limit === null ? 0 : measured.hiddenHunks))

  $effect(() => {
    const source = shown
    const file = path
    const folder = folderPath
    const element = content
    if (!element) return
    let cancelled = false
    let timer: ReturnType<typeof setTimeout> | undefined
    // Paint plaintext first, and do not tokenize off-screen output during bulk expansion.
    const observer = new IntersectionObserver(
      (entries) => {
        if (!entries.some((entry) => entry.isIntersecting)) return
        observer.disconnect()
        timer = setTimeout(() => {
          if (cancelled) return
          void contextHighlights(source, file, folder).then(
            (tokens) => {
              if (!cancelled) highlighted = { rows: source, tokens }
            },
            (cause) => console.warn('Code syntax highlighting failed', cause)
          )
        }, 0)
      },
      { root: element.closest('[data-slot="conversation"]'), rootMargin: '400px' }
    )
    observer.observe(element)
    return () => {
      cancelled = true
      clearTimeout(timer)
      observer.disconnect()
    }
  })

  $effect(() => {
    const element = content
    const visibleRows = shown
    const maxRows = limit
    if (!element || maxRows === null) return
    const sourceRows = new Set(rows)
    // Preserve OMP's visual-row cap after logical hunk budgeting. Browser wrapping
    // is measured, not estimated; full views stay in the transcript's scroll flow.
    const measure = () => {
      const lineHeight = Number.parseFloat(getComputedStyle(element).lineHeight)
      if (!Number.isFinite(lineHeight) || element.clientWidth === 0) return
      const height = lineHeight * maxRows
      const top = element.getBoundingClientRect().top
      let hiddenLines = 0
      let hiddenHunks = 0
      let inChange = false
      for (let index = 0; index < visibleRows.length; index += 1) {
        const row = visibleRows[index]
        const rect = element.children[index].getBoundingClientRect()
        const isChange = row.kind === 'add' || row.kind === 'del'
        if (rect.bottom - top > height + 0.5 && sourceRows.has(row)) hiddenLines += 1
        if (isChange && !inChange && rect.top - top >= height - 0.5) hiddenHunks += 1
        inChange = isChange
      }
      measured = { height, hiddenLines, hiddenHunks }
    }
    measure()
    const observer = new ResizeObserver(measure)
    observer.observe(element)
    return () => observer.disconnect()
  })
</script>

<div data-slot="tool-code" class="min-w-0 font-mono text-xs">
  <div
    class="overflow-hidden"
    style:max-height={limit !== null && measured.height ? `${measured.height}px` : undefined}
  >
    <div bind:this={content} class="leading-normal" style:--gutter-width={`${gutterWidth + 1}ch`}>
      {#each shown as row, index}
        <div
          data-slot="diff-row"
          data-kind={row.kind}
          class={cn(
            'group grid min-h-[1lh] grid-cols-[calc(var(--gutter-width)_+_1ch)_minmax(0,1fr)] data-[kind=add]:text-success data-[kind=del]:text-danger',
            neutral ? 'text-fg' : 'text-muted'
          )}
        >
          <span class="border-r border-edge pr-[1ch] text-right whitespace-pre select-none" aria-hidden="true"
            >{gutters[index]}</span
          >
          {#if row.kind === 'gap'}
            <span
              class="block min-w-0 pl-[1ch] wrap-anywhere whitespace-pre-wrap text-subtle"
              aria-label="Unchanged lines omitted">…</span
            >
          {:else}
            {@const parts = codeParts(
              row.text,
              emphasis?.get(row) ?? (highlighted?.rows === shown ? highlighted.tokens.get(index) : undefined)
            )}
            <span data-slot="diff-code" class="block min-w-0 pl-[1ch] wrap-anywhere whitespace-pre-wrap [tab-size:3]"
              >{#each parts.indent.match(/ +|\t/g) ?? [] as whitespace}<span
                  class="relative inline-block text-subtle opacity-65"
                  style:width={whitespace === '\t' ? '3ch' : `${whitespace.length}ch`}
                  style:background-image={whitespace === '\t'
                    ? undefined
                    : 'radial-gradient(circle, currentColor 0.6px, transparent 0.8px)'}
                  style:background-size="1ch 1lh"
                  >{whitespace}{#if whitespace === '\t'}<svg
                      class="pointer-events-none absolute inset-0 h-[1lh] w-full select-none"
                      aria-hidden="true"
                      viewBox="0 0 12 16"
                      preserveAspectRatio="none"
                      ><path fill="none" stroke="currentColor" stroke-width="1" d="M3 8h6m-2-2 2 2-2 2" /></svg
                    >{/if}</span
                >{/each}{#each parts.tokens as token}<span
                  class={token.changed
                    ? 'text-surface group-data-[kind=add]:bg-success group-data-[kind=del]:bg-danger'
                    : undefined}
                  style:color={token.color}>{token.text}</span
                >{/each}</span
            >
          {/if}
        </div>
      {/each}
    </div>
  </div>
  {#if limit !== null && (hiddenLines > 0 || hiddenHunks > 0)}
    <div class="mt-1">
      <MoreButton
        label={hiddenHunks > 0 ? formatCount(hiddenHunks, 'more hunk') : formatCount(hiddenLines, 'more line')}
        {onExpand}
      />
    </div>
  {/if}
</div>
