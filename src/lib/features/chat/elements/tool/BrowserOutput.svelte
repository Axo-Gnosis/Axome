<script lang="ts">
  import { Badge } from '$lib/components/ui/badge'
  import { formatCount } from '$lib/features/chat/format'
  import type { ToolStep } from '$lib/features/chat/turns'
  import type { BlockView } from '$lib/features/chat/view'
  import { platform } from '$lib/platform'
  import ToolLink from './ToolLink.svelte'
  import ToolOutput from './ToolOutput.svelte'
  import type { BrowserOutputData } from './browserOutput'
  import { urlDisplay } from './webOutput'

  let {
    data,
    output,
    view,
    onExpand
  }: {
    data: BrowserOutputData
    output: ToolStep['output']
    view: BlockView
    onExpand: () => void
  } = $props()
</script>

<div data-slot="browser-output" class="flex min-w-0 flex-col gap-2 px-3 py-2 font-mono text-xs text-fg">
  {#if data.url || data.tab !== 'main' || data.consoleErrors.length > 0 || data.failedRequests.length > 0}
    <div class="flex min-w-0 flex-wrap items-center gap-x-2 gap-y-1">
      {#if data.url}
        <ToolLink onclick={() => void platform.links.openExternal(data.url!)}>
          {data.title || urlDisplay(data.url)}
        </ToolLink>
      {/if}
      {#if data.tab !== 'main'}
        <Badge variant="default" class="text-[11px]">{data.tab}</Badge>
      {/if}
      {#if data.consoleErrors.length > 0}
        <Badge variant="danger" class="text-[11px]">
          {formatCount(data.consoleErrors.length, 'console error')}
        </Badge>
      {/if}
      {#if data.failedRequests.length > 0}
        <Badge variant="danger" class="text-[11px]">
          {formatCount(data.failedRequests.length, 'failed request')}
        </Badge>
      {/if}
    </div>
  {/if}
  <ToolOutput parts={output} {view} failed={false} {onExpand} />
</div>
