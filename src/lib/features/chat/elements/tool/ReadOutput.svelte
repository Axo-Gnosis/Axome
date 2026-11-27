<script lang="ts">
  import type { ToolStep } from '$lib/features/chat/turns'
  import type { BlockView } from '$lib/features/chat/view'
  import ToolCode from './ToolCode.svelte'
  import ToolOutput from './ToolOutput.svelte'
  import { readOutput } from './readOutput'

  let {
    call,
    folderPath,
    view,
    onExpand
  }: {
    call: ToolStep
    folderPath: string
    view: BlockView
    onExpand: () => void
  } = $props()

  const limit = $derived(view === 'preview' ? 40 : null)
  const read = $derived(readOutput(call))
</script>

<div data-slot="read-output" class="min-w-0 px-3 py-2 font-mono text-xs">
  {#if read}
    {#if read.leading.length}
      <ToolOutput parts={read.leading} view="full" {onExpand} />
    {/if}
    {#each read.sources as file, index}
      {#if read.sources.length > 1}
        <div class="text-subtle" class:mt-2={index > 0}>{file.header}</div>
      {/if}
      {#if file.rows.length}
        <ToolCode rows={file.rows} path={file.path} {folderPath} {limit} {onExpand} neutral />
      {:else}
        <div class="text-subtle">{file.directory ? 'Empty directory' : 'No source text recorded'}</div>
      {/if}
      {#if file.notice}
        <pre class="mt-1 font-mono wrap-anywhere whitespace-pre-wrap text-subtle">{file.notice}</pre>
      {/if}
      {#if file.attachments.length}
        <div class="mt-2">
          <ToolOutput parts={file.attachments} view="full" {onExpand} />
        </div>
      {/if}
    {/each}
  {:else}
    <ToolOutput parts={call.output} {view} {onExpand} />
  {/if}
</div>
