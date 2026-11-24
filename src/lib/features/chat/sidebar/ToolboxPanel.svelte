<script lang="ts">
  import { Badge } from '$lib/components/ui/badge'
  import { IconButton } from '$lib/components/ui/button'
  import { Chip } from '$lib/components/ui/chip'
  import * as Resizable from '$lib/components/ui/resizable'
  import { SidebarNote, SidebarRow } from '$lib/components/ui/sidebar-row'
  import { StatusDot } from '$lib/components/ui/status-dot'
  import SidebarHeader from '$lib/features/app-shell/sidebar/SidebarHeader.svelte'
  import SidebarPanel from '$lib/features/app-shell/sidebar/SidebarPanel.svelte'
  import { openOwnedPath } from '$lib/features/workbench/links/openLink'
  import Icon, { type IconData } from '$lib/icons/Icon.svelte'
  import type { ChatSessionTool, ChatToolbox } from '$lib/types/backend'
  import {
    Alert02Icon,
    FlashIcon,
    Layers01Icon,
    SparklesIcon,
    SquareArrowOutUpRightIcon,
    UserGroupIcon
  } from '@hugeicons/core-free-icons'

  let {
    tools,
    toolbox,
    owner
  }: {
    tools: ChatSessionTool[]
    toolbox: ChatToolbox | null
    owner: { folderPath: string; sessionId: string | null }
  } = $props()

  type DefinitionGroup = 'skills' | 'agents' | 'modes' | 'keywords'
  type CatalogTab = DefinitionGroup | 'diagnostics'
  type Fact = { label: string; value: string | string[] | null | undefined }
  type Definition = { name: string; description: string; path: string | null; flags: string[]; facts: Fact[] }
  let selectedTab = $state<CatalogTab | null>(null)
  let expandedItem = $state<string | null>(null)
  const diagnostics = $derived(toolbox?.diagnostics ?? [])
  const catalogTabs = $derived<{ id: CatalogTab; label: string; count: number; icon: IconData }[]>([
    { id: 'skills', label: 'Skills', count: toolbox?.skills.length ?? 0, icon: SparklesIcon },
    { id: 'agents', label: 'Agents', count: toolbox?.agents.length ?? 0, icon: UserGroupIcon },
    { id: 'modes', label: 'Modes', count: toolbox?.modes.length ?? 0, icon: Layers01Icon },
    { id: 'keywords', label: 'Keywords', count: toolbox?.keywords.length ?? 0, icon: FlashIcon },
    ...(diagnostics.length
      ? [{ id: 'diagnostics' as const, label: 'Diagnostics', count: diagnostics.length, icon: Alert02Icon }]
      : [])
  ])
  const activeTab = $derived(
    selectedTab && catalogTabs.some((tab) => tab.id === selectedTab)
      ? selectedTab
      : diagnostics.length
        ? 'diagnostics'
        : 'skills'
  )
  const activeCatalog = $derived(catalogTabs.find((tab) => tab.id === activeTab)!)
  const toolGroups = $derived(
    [
      { heading: 'Direct', items: tools.filter((tool) => tool.exposure === 'direct') },
      { heading: 'Promoted', items: tools.filter((tool) => tool.exposure === 'deferred' && tool.promoted) },
      { heading: 'Deferred', items: tools.filter((tool) => tool.exposure === 'deferred' && !tool.promoted) }
    ].filter((group) => group.items.length > 0)
  )

  const definitions = $derived.by((): Definition[] => {
    if (!toolbox) return []
    switch (activeTab) {
      case 'skills':
        return toolbox.skills.map((item) => ({
          ...item,
          flags: item.modelInvocable ? [] : ['/command only'],
          facts: [
            { label: 'Arguments', value: item.argumentHint },
            { label: 'Invocation', value: item.modelInvocable ? 'Model or /command' : '/command only' }
          ]
        }))
      case 'agents':
        return toolbox.agents.map((item) => ({
          ...item,
          flags: item.team ? ['team'] : item.runAs ? [item.runAs] : [],
          facts: [
            { label: 'Run as', value: item.runAs },
            { label: 'Model', value: item.model },
            { label: 'Tool allowlist', value: item.tools?.length === 0 ? 'None' : item.tools },
            { label: 'Bash', value: item.bashReadOnly ? 'Read-only' : null },
            { label: 'Max turns', value: item.maxTurns?.toString() },
            { label: 'Structured output', value: item.structuredOutput ? 'Yes' : null },
            { label: 'Members', value: item.members }
          ]
        }))
      case 'modes':
        return toolbox.modes.map((item) => ({
          ...item,
          flags: [...(item.resultReviewed ? ['reviewed'] : []), ...(item.bashReadOnly ? ['read-only bash'] : [])],
          facts: [
            { label: 'Arguments', value: item.argumentHint },
            { label: 'Invocation', value: item.modelInvocable ? 'Model or /command' : '/command only' },
            { label: 'Model', value: item.model },
            { label: 'Allowed tools', value: item.toolsAllow?.length === 0 ? 'None' : item.toolsAllow },
            { label: 'Denied tools', value: item.toolsDeny },
            { label: 'Bash', value: item.bashReadOnly ? 'Read-only' : null },
            { label: 'Writes', value: item.writes?.length === 0 ? 'None' : item.writes },
            {
              label: 'Result',
              value: item.resultReviewed === null ? null : item.resultReviewed ? 'Reviewed' : 'Not reviewed'
            }
          ]
        }))
      case 'keywords':
        return toolbox.keywords.map((item) => ({
          ...item,
          flags: item.model ? ['model switch'] : [],
          facts: [
            { label: 'Model', value: item.model },
            { label: 'Required tools', value: item.requiresTools }
          ]
        }))
      default:
        return []
    }
  })

  function extractParameters(
    schema: Record<string, unknown>
  ): { name: string; required: boolean; type: string | null }[] {
    const props = schema.properties
    if (!props || typeof props !== 'object' || Array.isArray(props)) return []
    const required = new Set(
      Array.isArray(schema.required) ? schema.required.filter((item): item is string => typeof item === 'string') : []
    )
    return Object.entries(props as Record<string, unknown>).map(([name, raw]) => {
      const prop = raw && typeof raw === 'object' && !Array.isArray(raw) ? (raw as Record<string, unknown>) : null
      return { name, required: required.has(name), type: prop && typeof prop.type === 'string' ? prop.type : null }
    })
  }
</script>

<SidebarPanel title="Tools ({tools.length})">
  <Resizable.PaneGroup direction="vertical">
    <Resizable.Pane defaultSize={60} minSize={15}>
      <div class="h-full overflow-y-auto">
        {#each toolGroups as group (group.heading)}
          <div class="px-2.5 pt-2.5 pb-1 text-xs font-semibold tracking-wide text-muted uppercase">
            {group.heading} <span class="font-mono text-subtle">{group.items.length}</span>
          </div>
          {#each group.items as tool (tool.name)}
            {@const key = `tools:${tool.name}`}
            {@const isExpanded = expandedItem === key}
            <SidebarRow
              behavior="disclosure"
              density="compact"
              pressed={isExpanded}
              expanded={isExpanded}
              content={toolContent}
              onclick={() => (expandedItem = isExpanded ? null : key)}
            >
              <span class="shrink-0 font-mono text-fg">{tool.name}</span>
              {#if !tool.allowedInMode}<Badge variant="warning">Blocked</Badge>{/if}
              {#if tool.promoted && tool.exposure === 'deferred'}<Badge variant="accent">Promoted</Badge>{/if}
              <span class="min-w-0 flex-1 truncate text-xs text-muted">{tool.summary}</span>
            </SidebarRow>
            {#snippet toolContent()}
              {@const params = extractParameters(tool.parameters)}
              <div class="border-b border-edge bg-surface px-3 py-2 text-xs">
                <p class="whitespace-pre-wrap text-muted">{tool.description || tool.summary}</p>
                {#if params.length}
                  <ul class="mt-2 flex flex-col gap-1 border-t border-edge/50 pt-2">
                    {#each params as param (param.name)}
                      <li class="flex flex-wrap items-baseline gap-1.5">
                        <span class="font-mono text-fg">{param.name}</span>
                        {#if param.type}<span class="font-mono text-subtle">{param.type}</span>{/if}
                        <span class={param.required ? 'text-accent' : 'text-subtle'}
                          >{param.required ? 'required' : 'optional'}</span
                        >
                      </li>
                    {/each}
                  </ul>
                {/if}
              </div>
            {/snippet}
          {/each}
        {:else}
          <SidebarNote>No tools available.</SidebarNote>
        {/each}
      </div>
    </Resizable.Pane>
    <Resizable.Handle />
    <Resizable.Pane defaultSize={40} minSize={15}>
      <div class="flex h-full min-h-0 flex-col">
        <SidebarHeader title="{activeCatalog.label} ({activeCatalog.count})">
          {#snippet actions()}
            {#each catalogTabs as tab (tab.id)}
              <IconButton
                tooltip={tab.label}
                active={activeTab === tab.id}
                onclick={() => {
                  selectedTab = tab.id
                  expandedItem = null
                }}
              >
                <span class="relative flex items-center">
                  <Icon icon={tab.icon} size={14} />
                  {#if tab.id === 'diagnostics'}<StatusDot tone="waiting" size="sm" corner />{/if}
                </span>
              </IconButton>
            {/each}
          {/snippet}
        </SidebarHeader>
        <div class="min-h-0 flex-1 overflow-y-auto">
          {#if activeTab === 'diagnostics'}
            {#each diagnostics as message, index (index)}
              <div class="border-t border-edge/30 px-2.5 py-1.5 font-mono text-xs break-words text-warning-bright">
                {message}
              </div>
            {/each}
          {:else}
            {#each definitions as item (item.name)}
              {@const key = `${activeTab}:${item.name}`}
              {@const isExpanded = expandedItem === key}
              <SidebarRow
                behavior="disclosure"
                density="compact"
                pressed={isExpanded}
                expanded={isExpanded}
                content={definitionContent}
                actions={item.path ? definitionAction : undefined}
                onclick={() => (expandedItem = isExpanded ? null : key)}
              >
                <span class="shrink-0 font-mono text-fg">{item.name}</span>
                {#each item.flags as flag (flag)}<Badge>{flag}</Badge>{/each}
                <span class="min-w-0 flex-1 truncate text-xs text-muted">{item.description}</span>
              </SidebarRow>
              {#snippet definitionAction()}
                {#if item.path}
                  <IconButton
                    tooltip="Open definition"
                    onclick={() => void openOwnedPath(item.path!, undefined, owner)}
                  >
                    <Icon icon={SquareArrowOutUpRightIcon} size={13} />
                  </IconButton>
                {/if}
              {/snippet}
              {#snippet definitionContent()}
                <div class="border-b border-edge bg-surface px-3 py-2 text-xs">
                  <dl class="flex flex-col gap-2">
                    {#if item.description}
                      <div>
                        <dt class="text-muted">Description</dt>
                        <dd class="mt-1 whitespace-pre-wrap text-fg">{item.description}</dd>
                      </div>
                    {/if}
                    {#each item.facts as fact (fact.label)}
                      {#if fact.value != null && (typeof fact.value === 'string' ? fact.value !== '' : fact.value.length > 0)}
                        <div class="flex flex-wrap items-baseline gap-x-3 gap-y-1">
                          <dt class="text-muted">{fact.label}</dt>
                          <dd class="flex min-w-0 flex-wrap gap-1 break-words text-fg">
                            {#if Array.isArray(fact.value)}
                              {#each fact.value as value, index (index)}<Chip class="font-mono">{value}</Chip>{/each}
                            {:else}<span class="whitespace-pre-wrap">{fact.value}</span>{/if}
                          </dd>
                        </div>
                      {/if}
                    {/each}
                    {#if item.path}
                      <div>
                        <dt class="text-muted">Definition</dt>
                        <dd
                          class="mt-1 font-mono wrap-anywhere text-fg"
                          title={item.path.startsWith('/') ? item.path : `${owner.folderPath}/${item.path}`}
                        >
                          {item.path}
                        </dd>
                      </div>
                    {/if}
                  </dl>
                </div>
              {/snippet}
            {:else}
              <SidebarNote>No {activeCatalog.label.toLowerCase()} loaded.</SidebarNote>
            {/each}
          {/if}
        </div>
      </div>
    </Resizable.Pane>
  </Resizable.PaneGroup>
</SidebarPanel>
