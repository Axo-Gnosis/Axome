<script lang="ts">
  import { backend } from '$lib/api/backend'
  import { getErrorMessage } from '$lib/utils/client-error'
  import { Button, IconButton } from '$lib/components/ui/button'
  import SidebarDivision from '$lib/features/app-shell/sidebar/SidebarDivision.svelte'
  import { SidebarNote, SidebarRow } from '$lib/components/ui/sidebar-row'
  import { InfoTooltip } from '$lib/components/ui/tooltip'
  import SidebarPanel from '$lib/features/app-shell/sidebar/SidebarPanel.svelte'
  import { cacheHitPercent, contextUsage, formatCost, formatTokens } from '$lib/features/chat/format'
  import { openOwnedPath } from '$lib/features/workbench/links/openLink'
  import FileIcon from '$lib/icons/FileIcon.svelte'
  import Icon, { type IconData } from '$lib/icons/Icon.svelte'
  import type { ChatContextDetails, ChatContextFile, ChatState } from '$lib/types/backend'
  import { copyToClipboard } from '$lib/utils/clipboard'
  import type { Snippet } from 'svelte'
  import {
    ArrowDown02Icon,
    ArrowUp02Icon,
    Copy01Icon,
    DatabaseExportIcon,
    DatabaseImportIcon,
    DollarCircleIcon,
    SigmaIcon,
    SquareArrowOutUpRightIcon
  } from '@hugeicons/core-free-icons'

  let {
    state: chat,
    owner
  }: {
    state: ChatState | null
    owner: { folderPath: string; sessionId: string | null }
  } = $props()

  let expandedFile = $state<string | null>(null)
  let expandedGroup = $state<string | null>(null)
  let expandedSection = $state<string | null>(null)
  let loaded = $state<{ sessionId: string; details: ChatContextDetails } | null>(null)
  let detailsError = $state<string | null>(null)
  const sessionId = $derived(chat?.sessionId)
  const running = $derived(chat?.live.running ?? false)
  const readOnly = $derived(chat?.readOnly ?? false)
  const details = $derived(loaded && loaded.sessionId === sessionId ? loaded.details : null)

  // Fetched on open and whenever a run starts or settles; refetching per entry would
  // copy the whole prompt and context files on every step of a run.
  $effect(() => {
    const id = sessionId
    const folder = owner.folderPath
    void running
    detailsError = null
    if (!id) return
    if (readOnly) {
      detailsError = 'Context details are available for live sessions only.'
      return
    }
    let stale = false
    backend.chat.contextDetails(folder, id).then(
      (value) => {
        if (!stale) loaded = { sessionId: id, details: value }
      },
      (error) => {
        if (!stale) detailsError = getErrorMessage(error)
      }
    )
    return () => {
      stale = true
    }
  })

  const context = $derived(chat ? contextUsage(chat.live.stats) : null)
  const measuredTokens = $derived(context?.tokens ?? null)
  const windowTokens = $derived(context?.window ?? null)
  const triggerTokens = $derived(context?.trigger ?? null)
  const fraction = $derived(context?.window ? Math.min(1, context.fraction) : null)
  const atTrigger = $derived(context?.atTrigger ?? false)

  const estimate = (text: string) => Math.ceil(text.length / 4)
  // Declared tool schemas ride along with every request (direct tools plus deferred ones the
  // model promoted); without them the measured remainder would be misread as conversation.
  const toolDefs = $derived(
    (chat?.sessionTools ?? [])
      .filter((tool) => tool.exposure === 'direct' || tool.promoted)
      .map((tool) => ({
        name: tool.name,
        tokens: estimate(
          JSON.stringify({ name: tool.name, description: tool.description, parameters: tool.parameters })
        )
      }))
      .sort((a, b) => b.tokens - a.tokens)
  )
  const promptSections = $derived(
    (details?.prompt ?? []).map((section, index) => ({
      ...section,
      key: `prompt:${index}`,
      label: section.name.replace(/[_-]+/g, ' ').replace(/^./, (letter) => letter.toUpperCase()),
      tokens: estimate(section.body)
    }))
  )
  const instructionTokens = $derived(details ? promptSections.reduce((sum, section) => sum + section.tokens, 0) : null)
  const toolTokens = $derived(toolDefs.reduce((sum, tool) => sum + tool.tokens, 0))
  const groups = $derived(
    [
      {
        key: 'conversation',
        label: 'Conversation',
        tokens:
          measuredTokens !== null && instructionTokens !== null
            ? Math.max(0, measuredTokens - instructionTokens - toolTokens)
            : null
      },
      { key: 'instructions', label: 'Instructions', tokens: instructionTokens },
      { key: 'tools', label: 'Tools', tokens: toolTokens }
    ].sort((a, b) => (b.tokens ?? -1) - (a.tokens ?? -1))
  )

  function share(tokens: number): number {
    return measuredTokens !== null && measuredTokens > 0
      ? Math.max(0, Math.min(100, (tokens / measuredTokens) * 100))
      : 0
  }
</script>

<SidebarPanel title="Context">
  {#snippet headerActions()}
    {#if sessionId}
      <Button
        variant="text"
        size="xs"
        class="gap-1.5 px-1.5 font-mono text-xs"
        aria-label="Copy Session ID"
        title={sessionId}
        onclick={() => sessionId && void copyToClipboard(sessionId)}
      >
        {sessionId.slice(0, 8)}
        <Icon icon={Copy01Icon} size={11} />
      </Button>
    {/if}
  {/snippet}
  {#if !chat}
    <SidebarNote>No usage reported yet</SidebarNote>
  {:else}
    {@const stats = chat.live.stats}
    {@const usage = stats.tree ?? stats.usage}
    {@const hitPercent = cacheHitPercent(usage)}
    {@const totalTokens = usage.input + usage.output + usage.cacheRead + usage.cacheWrite}
    <div class="h-full overflow-y-auto">
      <div class="space-y-2 p-3">
        {#if measuredTokens !== null}
          {#if fraction !== null}
            <div class="flex flex-wrap items-baseline gap-x-2 gap-y-1">
              <span
                class={atTrigger
                  ? 'font-mono text-3xl text-warning tabular-nums'
                  : 'font-mono text-3xl text-bright tabular-nums'}
              >
                {Math.round(fraction * 100)}%
              </span>
              <span class="text-xs text-muted">of context used</span>
            </div>
          {/if}
          <p class="font-mono text-sm text-fg tabular-nums">
            {formatTokens(measuredTokens)}{#if windowTokens !== null}
              <span class="text-muted"> / {formatTokens(windowTokens)}</span>
            {/if}
            <span class="text-xs text-muted">tokens{windowTokens === null ? ' used' : ''}</span>
          </p>
          {#if windowTokens !== null && fraction !== null}
            <div
              role="meter"
              aria-label="Context usage"
              aria-valuemin={0}
              aria-valuemax={windowTokens}
              aria-valuenow={Math.min(windowTokens, measuredTokens)}
              aria-valuetext={`${formatTokens(measuredTokens)} of ${formatTokens(windowTokens)} tokens used`}
              class="py-1"
            >
              <div aria-hidden="true" class="relative h-1.5 bg-raised">
                <span
                  class={atTrigger ? 'absolute inset-y-0 left-0 bg-warning' : 'absolute inset-y-0 left-0 bg-accent'}
                  style:width="{fraction * 100}%"
                ></span>
                {#if triggerTokens !== null}
                  <span
                    class="absolute -top-0.5 h-2.5 w-px bg-muted"
                    style:left="{Math.min(100, (triggerTokens / windowTokens) * 100)}%"
                  ></span>
                {/if}
              </div>
            </div>
          {/if}
        {:else}
          <p class="text-xs text-muted">No usage reported yet</p>
        {/if}
        {#if triggerTokens !== null}
          <p class="text-xs text-muted">Auto-compacts at {formatTokens(triggerTokens)} tokens.</p>
        {/if}
      </div>

      <SidebarDivision title="In this context">
        {#snippet headerActions()}
          <InfoTooltip ariaLabel="About context estimates">
            Sizes are approximate, using 4 characters per token. Bars show each category's share of the current context.
            Conversation is the remainder after instructions and tools, including request framing and estimate drift.
          </InfoTooltip>
        {/snippet}
        {#if detailsError}
          <p class="px-2.5 py-1.5 text-xs text-muted">{detailsError}</p>
        {:else if !details}
          <SidebarNote>Loading context details…</SidebarNote>
        {/if}
        {#each groups as group (group.key)}
          <SidebarRow
            behavior="disclosure"
            density="compact"
            pressed={expandedGroup === group.key}
            expanded={expandedGroup === group.key}
            class="relative isolate first:border-t-0"
            onclick={() => {
              expandedGroup = expandedGroup === group.key ? null : group.key
              expandedSection = null
              expandedFile = null
            }}
          >
            <span
              aria-hidden="true"
              class="pointer-events-none absolute inset-y-0 left-0 -z-10 bg-accent-bg"
              style:width="{group.tokens === null ? 0 : share(group.tokens)}%"
            ></span>
            <span class="min-w-0 flex-1 text-fg">{group.label}</span>
            <span class="shrink-0 font-mono text-xs text-muted tabular-nums">
              {group.tokens === null ? 'Unavailable' : `≈${formatTokens(group.tokens)}`}
            </span>
            {#snippet content()}
              <div class="bg-surface">
                {#if group.key === 'conversation'}
                  <p class="px-3 py-1 text-xs text-muted">
                    Messages and tool results since the last compaction, plus request framing and estimate drift.
                  </p>
                  {#each details?.contextFiles.filter((file) => file.nested) ?? [] as file (file.path)}
                    {@render contextFile(file)}
                  {/each}
                {:else if group.key === 'instructions'}
                  {#each promptSections as section (section.key)}
                    {@render detailRow(
                      section.label,
                      section.tokens,
                      expandedSection === section.key,
                      () => {
                        expandedSection = expandedSection === section.key ? null : section.key
                        expandedFile = null
                      },
                      undefined,
                      sectionContent
                    )}
                    {#snippet sectionContent()}
                      {@const files =
                        section.name === 'project_context'
                          ? (details?.contextFiles.filter(
                              (file) => !file.nested && section.body.includes(file.content)
                            ) ?? [])
                          : []}
                      {#if files.length}
                        {#each files as file (file.path)}
                          {@render contextFile(file)}
                        {/each}
                      {:else}
                        <pre
                          class="m-0 max-h-80 overflow-y-auto px-3 py-2 font-mono text-xs wrap-anywhere whitespace-pre-wrap text-muted">{section.body}</pre>
                      {/if}
                    {/snippet}
                  {:else}
                    <p class="px-3 py-1 text-xs text-muted">
                      {details ? 'No prompt sections recorded.' : 'Instructions unavailable.'}
                    </p>
                  {/each}
                {:else}
                  {#each toolDefs as tool (tool.name)}
                    {@render detailRow(tool.name, tool.tokens)}
                  {:else}
                    <SidebarNote>No tool definitions recorded.</SidebarNote>
                  {/each}
                {/if}
              </div>
            {/snippet}
          </SidebarRow>
        {/each}
      </SidebarDivision>

      <SidebarDivision title="Session usage">
        {#snippet headerActions()}
          <InfoTooltip ariaLabel="About session usage">
            Cumulative usage across all requests, not the current context window.
            {#if stats.tree}Includes the root session and its subagents.{/if}
          </InfoTooltip>
        {/snippet}
        <dl class="py-1">
          {@render usageMetric('Input', formatTokens(usage.input), ArrowUp02Icon)}
          {@render usageMetric('Output', formatTokens(usage.output), ArrowDown02Icon)}
          {@render usageMetric(
            'Cache read',
            `${formatTokens(usage.cacheRead)}${hitPercent === null ? '' : ` · ${hitPercent}% hit`}`,
            DatabaseExportIcon
          )}
          {@render usageMetric('Cache write', formatTokens(usage.cacheWrite), DatabaseImportIcon)}
          {@render usageMetric('Total tokens', formatTokens(totalTokens), SigmaIcon)}
          {@render usageMetric('Cost', formatCost(usage.cost), DollarCircleIcon)}
          {#if stats.tree}
            {@const subagentCost =
              stats.tree.cost === null || stats.usage.cost === null
                ? null
                : Math.max(0, stats.tree.cost - stats.usage.cost)}
            {@render usageMetric('Root', formatCost(stats.usage.cost))}
            {@render usageMetric('Subagents', formatCost(subagentCost))}
          {/if}
        </dl>
      </SidebarDivision>
    </div>
  {/if}
</SidebarPanel>

{#snippet contextFile(file: ChatContextFile)}
  {@render detailRow(
    file.path,
    estimate(file.content),
    expandedFile === file.path,
    () => (expandedFile = expandedFile === file.path ? null : file.path),
    file.path,
    fileContent
  )}
  {#snippet fileContent()}
    <pre
      class="m-0 max-h-80 overflow-y-auto px-3 py-2 font-mono text-xs wrap-anywhere whitespace-pre-wrap text-muted">{file.content}</pre>
  {/snippet}
{/snippet}

{#snippet detailRow(
  name: string,
  tokens: number,
  expanded = false,
  onclick?: () => void,
  path?: string,
  content?: Snippet
)}
  {#snippet fileAction()}
    <IconButton tooltip="Open file" onclick={() => path && void openOwnedPath(path, undefined, owner)}>
      <Icon icon={SquareArrowOutUpRightIcon} size={13} />
    </IconButton>
  {/snippet}
  <SidebarRow
    behavior={onclick ? 'disclosure' : 'display'}
    density="compact"
    pressed={expanded}
    class="first:border-t-0"
    on="surface"
    {expanded}
    {onclick}
    {content}
    actions={path ? fileAction : undefined}
  >
    {#if path}<FileIcon filename={path} size={13} />{/if}
    <span class="min-w-0 flex-1 truncate text-fg" class:font-mono={!!path} title={path ?? name}>{name}</span>
    <span class="shrink-0 font-mono text-xs text-muted tabular-nums">≈{formatTokens(tokens)}</span>
  </SidebarRow>
{/snippet}

{#snippet usageMetric(label: string, value: string, icon?: IconData)}
  <!-- Keep the icon slot even when empty so subtotals align with Cost's label. -->
  <div
    data-slot="session-usage-row"
    class="grid min-h-6 grid-cols-[minmax(0,1fr)_auto] items-center gap-3 px-2.5 text-sm text-fg"
  >
    <dt class="flex min-w-0 items-center gap-2 text-muted">
      <span aria-hidden="true" class="inline-flex size-3.5 shrink-0 items-center justify-center">
        {#if icon}<Icon {icon} size={14} />{/if}
      </span>
      <span class="truncate" title={label}>{label}</span>
    </dt>
    <dd class="min-w-0 text-right font-mono text-xs tabular-nums">{value}</dd>
  </div>
{/snippet}
