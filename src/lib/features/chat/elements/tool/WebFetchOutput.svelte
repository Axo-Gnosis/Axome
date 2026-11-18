<script lang="ts">
  import { Button } from '$lib/components/ui/button'
  import ToolLink from './ToolLink.svelte'
  import type { ToolStep } from '$lib/features/chat/turns'
  import type { BlockView } from '$lib/features/chat/view'
  import { openOwnedPath } from '$lib/features/workbench/links/openLink'
  import { platform } from '$lib/platform'
  import ToolOutput from './ToolOutput.svelte'
  import { type WebFetchData, urlDisplay } from './webOutput'

  let {
    data,
    output,
    view,
    folderPath,
    sessionId,
    onExpand
  }: {
    data: WebFetchData
    output: ToolStep['output']
    view: BlockView
    folderPath: string
    sessionId: string | null
    onExpand: () => void
  } = $props()

  const targetUrl = $derived(data.finalUrl ?? data.url)
</script>

<div data-slot="web-fetch-output" class="flex min-w-0 flex-col gap-2 px-3 py-2 font-mono text-xs text-fg">
  <div class="flex min-w-0 flex-wrap items-center gap-x-2 gap-y-1">
    {#if targetUrl}
      <ToolLink onclick={() => void platform.links.openExternal(targetUrl)}
        >{data.title || urlDisplay(targetUrl)}</ToolLink
      >
    {/if}
    {#if data.savedLocalUrl}
      <Button
        type="button"
        variant="ghost"
        size="xs"
        class="h-5 px-1.5 font-mono text-[11px] text-accent"
        onclick={() => void openOwnedPath(data.savedLocalUrl!, undefined, { folderPath, sessionId })}
      >
        Open {data.savedLocalUrl}
      </Button>
    {/if}
  </div>
  <ToolOutput parts={output} {view} failed={false} {onExpand} />
</div>
