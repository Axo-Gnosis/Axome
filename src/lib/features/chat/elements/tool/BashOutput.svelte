<script lang="ts">
  import { cn } from '$lib/utils/cn'
  import { Button } from '$lib/components/ui/button'
  import { formatCount } from '$lib/features/chat/format'
  import type { ToolStep } from '$lib/features/chat/turns'
  import type { BlockView } from '$lib/features/chat/view'
  import { openLink } from '$lib/features/workbench/links/openLink'
  import { bashOutput } from './bashOutput'
  import MoreButton from './MoreButton.svelte'
  import TerminalOutput from './TerminalOutput.svelte'
  import ToolImage from './ToolImage.svelte'

  let {
    call,
    view,
    failed = false,
    onExpand,
    folderPath,
    sessionId
  }: {
    call: ToolStep
    view: BlockView
    failed?: boolean
    onExpand: () => void
    folderPath: string
    sessionId: string | null
  } = $props()

  const output = $derived(bashOutput(call, view === 'preview' ? 12 : null))
  const hasContent = $derived(output.parts.some((part) => part.type === 'image' || part.text.length > 0))
  const running = $derived(call.status === 'running')
</script>

<TerminalOutput output={call.output}>
  <div data-slot="bash-output" class="contents">
    {#if output.hidden > 0}
      <MoreButton label={formatCount(output.hidden, 'earlier line')} {onExpand} />
    {/if}
    {#if hasContent}
      <div class={cn('min-w-0 font-mono text-xs', failed ? 'text-danger-bright' : 'text-fg')}>
        {#each output.parts as part}
          {#if part.type === 'text' && part.text}
            <pre class="m-0 font-mono text-xs wrap-anywhere whitespace-pre-wrap">{part.text}</pre>
          {:else if part.type === 'image'}
            <ToolImage {part} />
          {/if}
        {/each}
      </div>
    {:else}
      <div class="font-mono text-xs text-subtle">{running ? 'Running · No output yet' : 'No output'}</div>
    {/if}
    {#if output.fullOutput}
      <div class="flex flex-wrap items-center gap-x-3 gap-y-1 text-xs text-subtle">
        <span>Recorded output is truncated.</span>
        <Button
          type="button"
          variant="text"
          size="xs"
          class="px-0 font-normal"
          title={output.fullOutput}
          onclick={() => void openLink(output.fullOutput!, { folderPath, sessionId })}>Open Full Output</Button
        >
      </div>
    {/if}
  </div>
</TerminalOutput>
