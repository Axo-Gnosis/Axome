<script lang="ts">
  import { Button } from '$lib/components/ui/button'
  import { NetLineChange } from '$lib/components/ui/net-line-change'
  import type { ChatController } from '$lib/features/chat/controller.svelte'
  import { formatDuration } from '$lib/features/chat/format'
  import type { ToolStep } from '$lib/features/chat/turns'
  import type { BlockView } from '$lib/features/chat/view'
  import { openOwnedPath } from '$lib/features/workbench/links/openLink'
  import { CircleXIcon } from '@hugeicons/core-free-icons'
  import Icon from '$lib/icons/Icon.svelte'
  import FileIcon from '$lib/icons/FileIcon.svelte'
  import { AskSummary } from '../ask'
  import { askOutcome, questionLabel } from '../ask/askResult'
  import BashCommand from '../tool/BashCommand.svelte'
  import BashOutput from '../tool/BashOutput.svelte'
  import BrowserOutput from '../tool/BrowserOutput.svelte'
  import DocsSearchOutput from '../tool/DocsSearchOutput.svelte'
  import ReadOutput from '../tool/ReadOutput.svelte'
  import SearchOutput from '../tool/SearchOutput.svelte'
  import ToolDiff from '../tool/ToolDiff.svelte'
  import ToolFields from '../tool/ToolFields.svelte'
  import ToolOutput from '../tool/ToolOutput.svelte'
  import WebFetchOutput from '../tool/WebFetchOutput.svelte'
  import WebSearchOutput from '../tool/WebSearchOutput.svelte'
  import { openReferenceResult } from '../tool/referenceNavigation'
  import { toolHeader, toolIcon } from '../tool/toolHeader'
  import { parseBrowserOutput } from '../tool/browserOutput'
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
    onOpenJob,
    onAnswer
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
    /** Focuses the docked ask panel for a pending question. */
    onAnswer?: () => void
  } = $props()
  const header = $derived(toolHeader(call, folderPath))
  const asking = $derived(
    controller.state?.live.questions.find((questions) => questions.callId === call.callId) ?? null
  )
  const askResult = $derived(call.name === 'ask' && !asking && call.status !== 'running' ? askOutcome(call) : null)
  const askTarget = $derived(
    asking && asking.questions.length > 1
      ? `${asking.questions.length} questions: ${asking.questions.map(questionLabel).join(', ')}`
      : (asking?.questions[0]?.question ?? '')
  )
  const failed = $derived(call.status === 'error')
  // Bulk expansion opens calls; only their More control reveals unabridged output.
  const view = $derived(overrides[call.id] ?? (expansion === 'collapsed' ? 'collapsed' : 'preview'))
  const showFields = $derived(
    !['read', 'edit', 'write', 'web_search', 'web_fetch', 'search_docs', 'browser'].includes(call.name)
  )
  // A dismissed ask is a tool error but reads as muted; its outcome decides the tone.
  const tone = $derived(askResult ? (askResult.tone === 'danger' ? 'danger' : 'idle') : header.tone)
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
    <BashCommand command={header.command} inline />
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
  {#if askResult}
    <span
      class={askResult.tone === 'answered'
        ? 'text-fg'
        : askResult.tone === 'danger'
          ? 'text-danger-bright'
          : 'text-muted'}>{askResult.text}</span
    >
  {:else}
    {#if header.lines}<NetLineChange added={header.lines.added} removed={header.lines.removed} />{/if}
    {#if header.meta}<span>{header.meta}</span>{/if}
  {/if}
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
  {#if header.status && !askResult}
    <span class="inline-flex items-center gap-1 text-danger-bright">
      <Icon icon={CircleXIcon} size={12} aria-hidden="true" />
      {header.status.label}
    </span>
  {/if}
{/snippet}

{#snippet askingTarget()}{askTarget}{/snippet}

{#snippet askingAside()}
  {#if onAnswer}
    <Button variant="text" size="xs" class="p-0 text-accent hover:text-accent-bright" onclick={onAnswer}
      >Answer below</Button
    >
  {/if}
{/snippet}

{#if asking}
  <RailRow
    icon={toolIcon(call.name)}
    tone="working"
    verb="Waiting for your answer"
    {nested}
    viewId={call.id}
    target={askingTarget}
    aside={askingAside}
  />
{:else}
  <RailRow
    icon={toolIcon(call.name)}
    {tone}
    verb={header.verb}
    mono={!header.known}
    {nested}
    viewId={call.id}
    open={view !== 'collapsed'}
    label={`${view === 'collapsed' ? 'Expand' : 'Collapse'} ${header.name} output`}
    onToggle={toggle}
    {target}
    {aside}
  >
    {#if call.name === 'ask'}
      <AskSummary {call} />
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
    {:else if (call.name === 'edit' || call.name === 'write') && !failed}
      <ToolDiff {call} {folderPath} {view} onExpand={expand} />
    {:else}
      {#if showFields}<ToolFields {call} />{/if}
      <ToolOutput parts={call.output} {view} {failed} onExpand={expand} />
    {/if}
  </RailRow>
{/if}
