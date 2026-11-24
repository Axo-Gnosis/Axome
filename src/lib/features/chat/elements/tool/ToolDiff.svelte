<script lang="ts">
  import { Button } from '$lib/components/ui/button'
  import { DisclosureChevron } from '$lib/components/ui/disclosure-chevron'
  import { NetLineChange } from '$lib/components/ui/net-line-change'
  import { contentLines, previewDiff, sourceRows } from '$lib/features/chat/lineDiff'
  import type { ToolStep } from '$lib/features/chat/turns'
  import type { BlockView } from '$lib/features/chat/view'
  import { openOwnedPath } from '$lib/features/workbench/links/openLink'
  import FileIcon from '$lib/icons/FileIcon.svelte'
  import { wordHighlights } from './diffHighlight'
  import { displayFilePath, editDiff, record, type ToolFileRecord } from './payload'
  import ToolCode from './ToolCode.svelte'

  let {
    call,
    records,
    folderPath,
    sessionId,
    view,
    onExpand
  }: {
    call: ToolStep
    records: ToolFileRecord[]
    folderPath: string
    sessionId: string | null
    view: BlockView
    onExpand: () => void
  } = $props()

  const limit = $derived(view === 'full' ? null : 40)
  const content = $derived(record(call.arguments).content)
</script>

{#key view === 'full'}
  <div data-slot="tool-diff" class="min-w-0 divide-y divide-edge font-mono text-xs">
    {#each records as { ledger, change }, index (index)}
      {@const path = change?.url ?? ledger.path}
      {@const label = change?.url ?? displayFilePath(ledger.path, folderPath)}
      {@const diff = change?.diff}
      {@const line = diff?.firstChangedLine}
      {@const patch = editDiff(change)}
      <details class="group/file" open={index === 0 || view === 'full'}>
        <summary
          class="flex min-w-0 cursor-pointer list-none items-center gap-1.5 px-3 py-2 text-fg hover:text-bright focus-visible:shadow-focus-ring focus-visible:outline-none [&::-webkit-details-marker]:hidden"
        >
          <DisclosureChevron open={false} size={12} class="text-muted group-open/file:rotate-90" />
          <FileIcon filename={path} size={12} />
          <span class="min-w-0 flex-1 truncate" title={path}>{label}{line ? `:${line}` : ''}</span>
          {#if call.result?.isError}<span class="shrink-0 text-warning">unfinished call</span>{/if}
          {#if diff}<NetLineChange added={diff.additions} removed={diff.deletions} split />{/if}
        </summary>
        <div class="min-w-0 border-t border-edge px-3 py-2">
          <Button
            type="button"
            variant="text"
            size="xs"
            class="mb-1.5 p-0"
            onclick={() => void openOwnedPath(path, line, { folderPath, sessionId })}>Open File</Button
          >
          {#if !change}
            <div class="text-muted">{ledger.access === 'read' ? 'Read' : 'Modified'}</div>
          {:else if diff?.patchTruncated}
            <div class="text-muted">Diff too large to preview inline (+{diff.additions} −{diff.deletions})</div>
          {:else if change.before?.binary || change.after.binary}
            <div class="text-muted">Binary file</div>
          {:else if change.before?.omitted || change.after.omitted}
            <div class="text-muted">Snapshot unavailable</div>
          {:else if patch}
            {#if patch.rows.length > 0}
              {@const preview = limit !== null ? previewDiff(patch.rows, limit, 8) : undefined}
              <ToolCode
                rows={patch.rows}
                {path}
                {folderPath}
                {limit}
                {onExpand}
                {preview}
                emphasis={wordHighlights(patch.rows, preview ? new Set(preview.rows) : undefined)}
              />
            {:else}
              <div class="text-muted">No changes</div>
            {/if}
          {:else if change.created && call.name === 'write' && records.length === 1 && typeof content === 'string'}
            <ToolCode rows={sourceRows(contentLines(content), 1)} {path} {folderPath} {limit} {onExpand} neutral />
          {:else}
            <div class="text-muted">No diff recorded</div>
          {/if}
        </div>
      </details>
    {/each}
  </div>
{/key}
