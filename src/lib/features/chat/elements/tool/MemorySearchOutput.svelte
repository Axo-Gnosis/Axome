<script lang="ts">
  import { formatCount } from '$lib/features/chat/format'
  import type { BlockView } from '$lib/features/chat/view'
  import { memoryNote } from '$lib/features/memory/notes'
  import { openOwnedPath } from '$lib/features/workbench/links/openLink'
  import type { MemoryHit } from './memoryOutput'
  import MoreButton from './MoreButton.svelte'
  import ToolLink from './ToolLink.svelte'

  let {
    data,
    view,
    onExpand,
    folderPath,
    sessionId
  }: {
    data: { hits: MemoryHit[] }
    view: BlockView
    onExpand: () => void
    folderPath: string
    sessionId: string | null
  } = $props()

  const visibleHits = $derived(view === 'preview' ? data.hits.slice(0, 5) : data.hits)
  const hidden = $derived(data.hits.length - visibleHits.length)
  const linked = $derived.by(() => {
    const notes = new Map<string, string>()
    for (const hit of data.hits) {
      for (const { url } of hit.links) {
        const name = memoryNote(url)?.note ?? url
        if (!notes.has(name)) notes.set(name, url)
        if (notes.size === 6) return notes
      }
    }
    return notes
  })
</script>

<div data-slot="memory-search-output" class="flex min-w-0 flex-col gap-2 px-3 py-2 font-mono text-xs text-muted">
  {#each visibleHits as hit, index}
    <div class="flex min-w-0 items-baseline gap-1.5">
      <span class="shrink-0 text-subtle">{index + 1}</span>
      <div class="min-w-0">
        <div class="font-sans wrap-anywhere whitespace-pre-wrap text-fg">{hit.snippet}</div>
        {hit.category ?? hit.rowType} ·
        <ToolLink onclick={() => void openOwnedPath(hit.url, hit.line ?? undefined, { folderPath, sessionId })}>
          {memoryNote(hit.url)?.note ?? hit.title}
        </ToolLink>
      </div>
    </div>
  {:else}
    <div class="text-subtle">No matching notes</div>
  {/each}
  {#if hidden > 0}
    <MoreButton label={formatCount(hidden, 'more hit')} {onExpand} />
  {/if}
  {#if linked.size}
    <div>
      Linked:
      {#each linked as [name, url], index}{index ? ', ' : ''}<ToolLink
          onclick={() => void openOwnedPath(url, undefined, { folderPath, sessionId })}>{name}</ToolLink
        >{/each}
    </div>
  {/if}
</div>
