<script lang="ts">
  import { NetLineChange } from '$lib/components/ui/net-line-change'
  import type { ChatController } from '$lib/features/chat/controller.svelte'
  import { firstLine } from '$lib/features/chat/format'
  import type { BlockView } from '$lib/features/chat/view'
  import { groupVerb, toolHeader, toolIcon } from '../tool/toolHeader'
  import type { ToolGroupStep } from './groupSteps'
  import RailRow from './RailRow.svelte'
  import ToolBlock from './ToolBlock.svelte'

  let {
    group,
    expansion,
    overrides,
    onView,
    folderPath,
    sessionId,
    controller,
    now,
    onOpenJob
  }: {
    group: ToolGroupStep
    expansion: BlockView
    overrides: Record<string, BlockView>
    onView: (id: string, view: BlockView) => void
    folderPath: string
    sessionId: string | null
    controller: ChatController
    now: number
    onOpenJob?: (ref?: string) => void
  } = $props()

  // Group folding must not overwrite the first call's individual view.
  const viewId = $derived(`tool-group:${group.id}`)
  const name = $derived(group.calls[0].name)
  const headers = $derived(group.calls.map((call) => toolHeader(call, folderPath)))
  const failed = $derived(group.calls.filter((call) => call.status === 'error').length)
  const running = $derived(group.calls.filter((call) => call.status === 'running').length)
  const targets = $derived(
    headers
      .map((header) => header.pathLabel || header.summary || firstLine(header.command))
      .filter(Boolean)
      .join(', ')
  )
  const lines = $derived.by(() => {
    if (name !== 'edit' && name !== 'write') return null
    let added = 0
    let removed = 0
    for (const header of headers) {
      added += header.lines?.added ?? 0
      removed += header.lines?.removed ?? 0
    }
    return added || removed ? { added, removed } : null
  })
  const view = $derived(overrides[viewId] ?? expansion)
</script>

{#snippet target()}{targets}{/snippet}

{#snippet aside()}
  {#if lines}<NetLineChange added={lines.added} removed={lines.removed} />{/if}
  {#if failed}<span class="text-danger-bright">{failed} failed</span>{/if}
  {#if running}<span class="text-accent">{running} running</span>{/if}
{/snippet}

<!-- The group never takes the danger tone, so a failed nested call keeps its own visible state. -->
<RailRow
  icon={toolIcon(name)}
  tone={running ? 'working' : 'idle'}
  verb={groupVerb(name, group.calls.length)}
  mono={!headers[0].known}
  {viewId}
  open={view !== 'collapsed'}
  label={`${view === 'collapsed' ? 'Expand' : 'Collapse'} ${name} calls`}
  onToggle={() => onView(viewId, view === 'collapsed' ? 'preview' : 'collapsed')}
  bare
  {target}
  {aside}
>
  <div class="ml-1 flex min-w-0 flex-col border-l border-edge pl-3.5">
    {#each group.calls as call (call.id)}
      <ToolBlock
        {call}
        {expansion}
        {overrides}
        {onView}
        {folderPath}
        {sessionId}
        {controller}
        nested
        {now}
        {onOpenJob}
      />
    {/each}
  </div>
</RailRow>
