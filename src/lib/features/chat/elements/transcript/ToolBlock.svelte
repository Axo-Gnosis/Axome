<script lang="ts">
  import { Button } from '$lib/components/ui/button'
  import { NetLineChange } from '$lib/components/ui/net-line-change'
  import type { ChatController } from '$lib/features/chat/controller.svelte'
  import { chatExtensionTarget, findExtension } from '$lib/features/chat/extensions'
  import { openExtensionSettings } from '$lib/features/settings/dialog/state.svelte'
  import { formatDuration } from '$lib/features/chat/format'
  import type { ToolStep } from '$lib/features/chat/turns'
  import type { BlockView } from '$lib/features/chat/view'
  import { openOwnedPath } from '$lib/features/workbench/links/openLink'
  import { CircleXIcon } from '@hugeicons/core-free-icons'
  import Icon from '$lib/icons/Icon.svelte'
  import FileIcon from '$lib/icons/FileIcon.svelte'
  import { AskDocument, AskSummary } from '../ask'
  import BashCommand from '../tool/BashCommand.svelte'
  import BashOutput from '../tool/BashOutput.svelte'
  import BrowserOutput from '../tool/BrowserOutput.svelte'
  import DocsSearchOutput from '../tool/DocsSearchOutput.svelte'
  import MemorySearchOutput from '../tool/MemorySearchOutput.svelte'
  import ReadOutput from '../tool/ReadOutput.svelte'
  import SearchOutput from '../tool/SearchOutput.svelte'
  import ToolDiff from '../tool/ToolDiff.svelte'
  import ToolFields from '../tool/ToolFields.svelte'
  import ToolOutput from '../tool/ToolOutput.svelte'
  import WebFetchOutput from '../tool/WebFetchOutput.svelte'
  import WebSearchOutput from '../tool/WebSearchOutput.svelte'
  import { openReferenceResult } from '../tool/referenceNavigation'
  import { parseMemorySearch } from '../tool/memoryOutput'
  import { toolHeader } from '../tool/toolHeader'
  import { parseBrowserOutput } from '../tool/browserOutput'
  import { fileRecords } from '../tool/payload'
  import { parseSearchDocs, parseWebFetch, parseWebSearch } from '../tool/webOutput'
  import RailRow from './RailRow.svelte'
  let {
    call,
    expansion,
    overrides,
    onView,
    folderPath,
    sessionId,
    controller,
    nested = false,
    now,
    onOpenJob
  }: {
    call: ToolStep
    expansion: BlockView
    overrides: Record<string, BlockView>
    onView: (id: string, view: BlockView) => void
    folderPath: string
    sessionId: string | null
    controller: ChatController
    nested?: boolean
    now: number
    onOpenJob?: (ref?: string) => void
  } = $props()
  const records = $derived(fileRecords(call.result?.details))
  const header = $derived(toolHeader(call, folderPath, records))
  const toolOwner = $derived.by(() => {
    const origins = controller.state?.toolOrigins
    return origins && Object.hasOwn(origins, call.name) ? origins[call.name] : null
  })
  const blockedWhileUnavailable = $derived.by(() => {
    const origin = header.blocked?.extension
    const state = controller.extensions
    if (!origin || !state || state.stale) return null
    const item = findExtension(state.payload, origin)
    return item && item.phase !== 'ready' ? origin : null
  })
  const asking = $derived(
    !header.blocked && call.name === 'ask'
      ? (controller.state?.live.questions.find((questions) => questions.callId === call.callId) ?? null)
      : null
  )
  const failed = $derived(call.status === 'error')
  // Bulk expansion opens calls; only their More control reveals unabridged output.
  const view = $derived(overrides[call.id] ?? (expansion === 'collapsed' ? 'collapsed' : 'preview'))
  const showFields = $derived(
    !['bash', 'read', 'edit', 'write', 'web_search', 'web_fetch', 'search_docs', 'browser', 'memory_search'].includes(
      call.name
    )
  )
  const hasBody = $derived(
    !header.blocked || header.command !== null || showFields || records.length > 0 || !!header.unfinished
  )
  const memory = $derived(!failed && call.name === 'memory_search' ? parseMemorySearch(call) : null)
  const webSearch = $derived(!failed && call.name === 'web_search' ? parseWebSearch(call) : null)
  const webFetch = $derived(!failed && call.name === 'web_fetch' ? parseWebFetch(call) : null)
  const browser = $derived(!failed && call.name === 'browser' ? parseBrowserOutput(call) : null)
  const docs = $derived(!failed && call.name === 'search_docs' ? parseSearchDocs(call) : null)
  const toggle = () => onView(call.id, view === 'collapsed' ? 'preview' : 'collapsed')
  const expand = () => onView(call.id, 'full')
  const canBackground = $derived(call.name === 'bash' && call.status === 'running' && !controller.readOnly)
  let backgrounding = $state(false)
  async function sendToBackground() {
    if (backgrounding) return
    backgrounding = true
    try {
      const ok = await controller.background(call.callId)
      if (ok) onOpenJob?.()
    } finally {
      backgrounding = false
    }
  }
</script>

{#snippet target()}
  {#if header.command !== null}
    <span class="flex min-w-0 items-center gap-1.5">
      {#if header.summary}<span class="shrink-0">{header.summary}</span>{/if}
      <BashCommand command={header.command} inline />
    </span>
  {:else}
    <span class="flex min-w-0 items-center gap-1.5">
      {#if header.summary}<span class="min-w-0 truncate">{header.summary}</span>{/if}
      {#if header.path !== null}
        {@const search = header.name === 'grep' || header.name === 'find'}
        {#if search}<span class="shrink-0 text-subtle">in</span>{/if}
        <Button
          variant="text"
          size="xs"
          class="min-w-0 shrink justify-start p-0 font-mono text-xs font-normal text-fg hover:text-bright hover:underline"
          onclick={() =>
            header.path !== null && void openOwnedPath(header.path, header.line, { folderPath, sessionId })}
          >{#if !search}<FileIcon filename={header.path} size={12} />{/if}<span class="truncate"
            >{header.pathLabel || header.path}</span
          ></Button
        >
      {/if}
    </span>
  {/if}
{/snippet}

{#snippet aside()}
  {#if header.lines}<NetLineChange added={header.lines.added} removed={header.lines.removed} split />{/if}
  {#if toolOwner}<span class="font-mono">{toolOwner}</span>{/if}
  {#if header.blocked?.extension}<span>by {header.blocked.extension.id}</span>{/if}
  {#if header.unfinished}<span class="text-warning">unfinished call</span>{/if}
  {#if header.meta}<span>{header.meta}</span>{/if}
  {#if call.status === 'running' && call.timestamp}
    <span class="tabular-nums">{formatDuration(Math.max(0, now - Date.parse(call.timestamp)))}</span>
  {/if}
  {#if canBackground}
    <Button
      variant="ghost"
      size="xs"
      disabled={backgrounding}
      onclick={(e) => {
        e.stopPropagation()
        void sendToBackground()
      }}>Background</Button
    >
  {/if}
  {#if header.status}
    <span class="inline-flex items-center gap-1 text-danger-bright">
      <Icon icon={CircleXIcon} size={12} aria-hidden="true" />
      {header.status.label}
    </span>
  {/if}
{/snippet}

{#snippet fileOutput()}
  {#if header.unfinished}
    <p class="px-3 py-2 font-sans text-sm text-warning">{header.unfinished}</p>
  {/if}
  {#if records.length > 0}
    <ToolDiff {call} {records} {folderPath} {sessionId} {view} onExpand={expand} />
  {/if}
{/snippet}

{#if asking}
  <AskDocument pending={asking} {controller} {folderPath} {sessionId} {now} />
  {@render fileOutput()}
{:else if !header.blocked && call.name === 'ask' && call.status !== 'running'}
  <AskSummary {call} {folderPath} {sessionId} />
  {@render fileOutput()}
{:else}
  <RailRow
    icon={header.icon}
    tone={header.tone}
    verb={header.verb}
    mono={!header.known}
    {nested}
    viewId={call.id}
    open={hasBody && view !== 'collapsed'}
    label={`${view === 'collapsed' ? 'Expand' : 'Collapse'} ${header.name} output`}
    onToggle={hasBody ? toggle : undefined}
    {target}
    {aside}
  >
    {#if header.blocked}
      {#if header.command}<div class="px-3 py-2"><BashCommand command={header.command} /></div>{/if}
      {#if showFields}<ToolFields {call} />{/if}
    {:else if call.name === 'bash'}
      <div class="px-3 py-2"><BashCommand command={header.command ?? ''} /></div>
      <BashOutput {call} {view} failed={header.tone === 'danger'} onExpand={expand} {folderPath} {sessionId} />
    {:else if call.name === 'grep' || call.name === 'find'}
      <SearchOutput
        {call}
        {view}
        onExpand={expand}
        onOpenResult={(path, line) => void openReferenceResult(call, { folderPath, sessionId }, path, line)}
      />
    {:else if memory}
      <MemorySearchOutput data={memory} {view} onExpand={expand} {folderPath} {sessionId} />
    {:else if webSearch}
      <WebSearchOutput data={webSearch} running={call.status === 'running'} {view} onExpand={expand} />
    {:else if webFetch}
      <WebFetchOutput data={webFetch} output={call.output} {view} {folderPath} {sessionId} onExpand={expand} />
    {:else if browser}
      <BrowserOutput data={browser} output={call.output} {view} onExpand={expand} />
    {:else if docs}
      <DocsSearchOutput data={docs} {view} {folderPath} {sessionId} onExpand={expand} />
    {:else if call.name === 'read' && !failed}
      <ReadOutput {call} {folderPath} {view} onExpand={expand} />
    {:else}
      {#if showFields}<ToolFields {call} />{/if}
      {#if call.output.length}
        <ToolOutput parts={call.output} {view} {failed} onExpand={expand} />
      {/if}
    {/if}
    {@render fileOutput()}
  </RailRow>
  {#if header.blocked}
    <div class="flex flex-wrap items-baseline gap-x-2 pb-1 text-sm text-danger-bright" class:pl-6={!nested}>
      <p class="min-w-0 wrap-anywhere whitespace-pre-wrap">
        {#if blockedWhileUnavailable}
          {`${blockedWhileUnavailable.id} checks every ${call.name} call before it runs, and it isn't running right now. Nothing ran.`}
        {:else}
          {header.blocked.reason}
        {/if}
      </p>
      {#if blockedWhileUnavailable}
        <Button
          variant="text"
          size="xs"
          onclick={() => openExtensionSettings(chatExtensionTarget(folderPath, blockedWhileUnavailable))}
          >Open Extensions</Button
        >
      {/if}
    </div>
  {/if}
{/if}
