<script lang="ts">
  import { Button } from '$lib/components/ui/button'
  import ToolLink from './ToolLink.svelte'
  import { formatCount } from '$lib/features/chat/format'
  import type { BlockView } from '$lib/features/chat/view'
  import { openOwnedPath } from '$lib/features/workbench/links/openLink'
  import { platform } from '$lib/platform'
  import MoreButton from './MoreButton.svelte'
  import { type DocsSearchData, urlHost } from './webOutput'

  let {
    data,
    view,
    folderPath,
    sessionId,
    onExpand
  }: {
    data: DocsSearchData
    view: BlockView
    folderPath: string
    sessionId: string | null
    onExpand: () => void
  } = $props()

  const limit = $derived(view === 'preview' ? 6 : Infinity)

  const openTarget = (target: string, line?: number) => {
    void openOwnedPath(target, line, { folderPath, sessionId })
  }
</script>

<div data-slot="docs-search-output" class="flex min-w-0 flex-col gap-2 px-3 py-2 font-mono text-xs text-fg">
  {#if data.mode === 'list'}
    {#if data.sources.length === 0 && data.configured.length === 0}
      <div class="text-subtle">No documentation sources cached yet.</div>
    {:else}
      <div class="flex min-w-0 flex-col gap-1">
        {#each data.sources.slice(0, limit) as src}
          <div class="flex min-w-0 items-baseline justify-between gap-2">
            <ToolLink onclick={() => openTarget(src.docPath)}>{src.key}</ToolLink>
            <span class="shrink-0 text-subtle">{formatCount(src.sections, 'section')}</span>
          </div>
        {/each}
        {#each data.configured.slice(0, Math.max(0, limit - data.sources.length)) as src}
          <div class="flex min-w-0 items-baseline justify-between gap-2 text-subtle">
            <span class="truncate">{src}</span>
            <span class="shrink-0">configured</span>
          </div>
        {/each}
      </div>
      {#if data.sources.length + data.configured.length > limit}
        <MoreButton
          label={formatCount(data.sources.length + data.configured.length - limit, 'more source')}
          {onExpand}
        />
      {/if}
    {/if}
  {:else if data.mode === 'toc'}
    <div class="flex min-w-0 flex-col gap-1">
      {#each data.entries.slice(0, limit) as entry}
        <div class="flex min-w-0 items-baseline justify-between gap-2">
          <ToolLink onclick={() => openTarget(entry.docUrl ?? entry.path, entry.startLine)}>{entry.crumb}</ToolLink>
          <span class="shrink-0 text-subtle">{entry.startLine}-{entry.endLine}</span>
        </div>
      {/each}
    </div>
    {#if data.entries.length > limit}
      <MoreButton label={formatCount(data.entries.length - limit, 'more section')} {onExpand} />
    {/if}
  {:else if data.mode === 'search'}
    {#if data.loose}
      <div class="text-subtle">Matched any query term.</div>
    {/if}
    {#if data.hits.length === 0}
      <div class="text-subtle">No matching sections.</div>
    {:else}
      <div class="flex min-w-0 flex-col gap-2">
        {#each data.hits.slice(0, limit) as hit}
          <div class="flex min-w-0 flex-col gap-0.5">
            <div class="flex min-w-0 flex-wrap items-baseline gap-x-2 gap-y-0.5">
              <ToolLink onclick={() => openTarget(hit.docUrl ?? hit.path, hit.startLine)}>{hit.crumb}</ToolLink>
              <span class="shrink-0 text-subtle">{hit.docUrl ?? hit.path}:{hit.startLine}-{hit.endLine}</span>
              {#if hit.webUrl}
                <Button
                  type="button"
                  variant="text"
                  size="xs"
                  class="shrink-0 p-0 font-mono text-xs text-subtle hover:text-bright hover:underline"
                  onclick={() => void platform.links.openExternal(hit.webUrl!)}
                >
                  {urlHost(hit.webUrl)}
                </Button>
              {/if}
            </div>
            {#if view === 'full' && hit.snippet}
              <pre
                class="m-0 border-l-2 border-edge pl-2 font-mono text-xs wrap-anywhere whitespace-pre-wrap text-muted">{hit.snippet}</pre>
            {/if}
          </div>
        {/each}
      </div>
      {#if data.hits.length > limit}
        <MoreButton label={formatCount(data.hits.length - limit, 'more section')} {onExpand} />
      {/if}
    {/if}
  {/if}
</div>
