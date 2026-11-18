<script lang="ts">
  import ToolLink from './ToolLink.svelte'
  import { formatCount } from '$lib/features/chat/format'
  import type { BlockView } from '$lib/features/chat/view'
  import { platform } from '$lib/platform'
  import MoreButton from './MoreButton.svelte'
  import { type WebSearchData, urlHost } from './webOutput'

  let {
    data,
    running,
    view,
    onExpand
  }: {
    data: WebSearchData
    running: boolean
    view: BlockView
    onExpand: () => void
  } = $props()

  const limit = $derived(view === 'preview' ? 5 : Infinity)
  const visibleHits = $derived(data.hits.slice(0, limit))
  const hidden = $derived(data.hits.length - visibleHits.length)
</script>

<div data-slot="web-search-output" class="flex min-w-0 flex-col gap-2 px-3 py-2 font-mono text-xs text-fg">
  {#if data.notes.length > 0}
    <div class="text-subtle">
      {#each data.notes as note}
        <div>{note}</div>
      {/each}
    </div>
  {/if}
  {#if data.answer}
    <div class="border-l-2 border-edge pl-2 font-sans text-xs whitespace-pre-wrap text-fg">
      {view === 'preview' && data.answer.length > 600 ? `${data.answer.slice(0, 600)}…` : data.answer}
    </div>
  {/if}
  {#if visibleHits.length > 0}
    <div class="flex min-w-0 flex-col gap-1.5">
      {#each visibleHits as hit, idx}
        <div class="flex min-w-0 flex-col gap-0.5">
          <div class="flex min-w-0 items-baseline gap-1.5">
            <span class="shrink-0 text-subtle">[{idx + 1}]</span>
            <ToolLink class="text-left" onclick={() => void platform.links.openExternal(hit.url)}>{hit.title}</ToolLink>
            <span class="shrink-0 text-subtle">{urlHost(hit.url)}</span>
            {#if hit.publishedDate}
              <span class="shrink-0 text-subtle">· {hit.publishedDate}</span>
            {/if}
          </div>
          {#if hit.snippet && view === 'full'}
            <div class="pl-6 font-sans text-xs whitespace-pre-wrap text-muted">
              {hit.snippet}
            </div>
          {/if}
        </div>
      {/each}
    </div>
  {:else if !data.answer}
    <div class="text-subtle">{running ? 'Searching…' : 'No results'}</div>
  {/if}
  {#if hidden > 0}
    <MoreButton label={formatCount(hidden, 'more result')} {onExpand} />
  {/if}
</div>
