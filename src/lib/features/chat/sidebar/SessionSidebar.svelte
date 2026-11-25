<script lang="ts" module>
  const WIDTH_STORAGE_KEY = 'axome.chat.sidebarWidth'
  const DEFAULT_WIDTH = 360
  const MIN_WIDTH = 260
  const MAX_WIDTH = 640

  function clampWidth(px: number): number {
    if (!Number.isFinite(px)) return DEFAULT_WIDTH
    return Math.round(Math.max(MIN_WIDTH, Math.min(MAX_WIDTH, px)))
  }

  function loadWidth(): number {
    if (typeof localStorage === 'undefined') return DEFAULT_WIDTH
    try {
      const raw = localStorage.getItem(WIDTH_STORAGE_KEY)
      if (!raw) return DEFAULT_WIDTH
      return clampWidth(Number(raw))
    } catch {
      return DEFAULT_WIDTH
    }
  }

  let sidebarWidth = $state(loadWidth())

  function setSidebarWidth(px: number): void {
    const next = clampWidth(px)
    sidebarWidth = next
    if (typeof localStorage === 'undefined') return
    try {
      localStorage.setItem(WIDTH_STORAGE_KEY, String(next))
    } catch {
      /* ignore storage errors */
    }
  }
</script>

<script lang="ts">
  import { ProgressRing } from '$lib/components/ui/progress'
  import { ResizeDivider } from '$lib/components/ui/resize-divider'
  import type { ToneStatus } from '$lib/components/ui/status-dot'
  import RailButton from '$lib/features/app-shell/sidebar/RailButton.svelte'
  import { RAIL_WIDTH_CLASS } from '$lib/features/app-shell/sidebar/state.svelte'
  import type { ChatController } from '$lib/features/chat/controller.svelte'
  import { contextUsage, formatCost } from '$lib/features/chat/format'
  import { jobsStatus } from '$lib/features/chat/jobs'
  import Icon from '$lib/icons/Icon.svelte'
  import {
    AiFile01Icon,
    BrainIcon,
    TaskEdit01Icon,
    TerminalIcon,
    UserGroupIcon,
    ToolCaseIcon
  } from '@hugeicons/core-free-icons'
  import { backend } from '$lib/api/backend'
  import type { Turn } from '$lib/features/chat/turns'
  import type { ChatAgentInfo } from '$lib/types/backend'
  import { sessionCaptures } from '$lib/features/memory/captures.svelte'
  import AgentsPanel from './AgentsPanel.svelte'
  import FilesPanel from './FilesPanel.svelte'
  import JobsPanel from './JobsPanel.svelte'
  import MemoryPanel from './MemoryPanel.svelte'
  import PlanPanel from './PlanPanel.svelte'
  import SessionPanel from './SessionPanel.svelte'
  import ToolboxPanel from './ToolboxPanel.svelte'
  import type { SessionSidebarState } from './state.svelte'

  let {
    controller,
    sidebar,
    owner,
    now,
    turns,
    personas,
    onJump
  }: {
    controller: ChatController
    sidebar: SessionSidebarState
    owner: { folderPath: string; sessionId: string | null }
    now: number
    turns: Turn[]
    /** Personas the root session can switch to. */
    personas: ChatAgentInfo[]
    /** Reveal a transcript step. */
    onJump: (stepId: string) => void
  } = $props()

  let memoryBank = $state<string | null>(null)
  $effect(() => {
    const folder = owner.folderPath
    let stale = false
    memoryBank = null
    backend.memory.folder(folder).then(
      (bank) => !stale && (memoryBank = bank),
      () => {}
    )
    return () => {
      stale = true
    }
  })
  const memoryStatus = $derived.by((): ToneStatus | null => {
    const runs = sessionCaptures(owner.folderPath, owner.sessionId)
    const running = runs.find((run) => run.endedAt === null)
    if (running)
      return {
        tone: 'working',
        text: running.trigger === 'compaction' ? 'Saving after compaction' : 'Saving after closing'
      }
    return runs.at(-1)?.error ? { tone: 'danger', text: 'Last save failed' } : null
  })

  const chat = $derived(controller.state)
  const jobs = $derived(chat?.live.jobs ?? [])
  const stats = $derived(chat?.live.stats ?? null)
  const context = $derived(stats ? contextUsage(stats) : null)
  const contextStatus = $derived.by((): ToneStatus | null => {
    if (!stats || !context) return null
    const pct = `${Math.round(context.fraction * 100)}%`
    const cost = (stats.tree ?? stats.usage).cost
    const text = cost === null || cost > 0 ? `${pct} · ${formatCost(cost)}` : pct
    return { tone: context.atTrigger ? 'waiting' : 'idle', text }
  })
  const agentsStatus = $derived(jobsStatus(jobs, 'child', 'Failed subagent'))
  const commandsStatus = $derived(jobsStatus(jobs, 'command', 'Failed job'))
  const planReview = $derived(chat?.live.reviews.some((review) => review.plan) ?? false)
  const planMode = $derived(
    !!chat?.live.mode && !!chat.toolbox.modes.find((mode) => mode.name === chat.live.mode)?.plan
  )
  // Kept while open so the panel can always be toggled closed from its own button.
  const showPlan = $derived(planMode || planReview || sidebar.view === 'plan')
  const planStatus = $derived<ToneStatus | null>(planReview ? { tone: 'waiting', text: 'Ready for review' } : null)
  let panelEl = $state<HTMLDivElement | null>(null)
  let resizeAnchor = 0
</script>

{#if sidebar.view !== null}
  <ResizeDivider
    direction="col"
    aria-label="Resize session sidebar"
    onResizeStart={(e) => {
      resizeAnchor = (panelEl?.getBoundingClientRect().width ?? sidebarWidth) + e.clientX
    }}
    onResize={(e) => setSidebarWidth(resizeAnchor - e.clientX)}
  />
{/if}
<div
  data-slot="session-rail"
  class="flex {RAIL_WIDTH_CLASS} shrink-0 flex-col items-center gap-0.5 border-l border-edge bg-ground py-2"
>
  <RailButton
    tooltipSide="left"
    label="Session"
    active={sidebar.view === 'session'}
    status={contextStatus}
    onclick={() => sidebar.toggle('session')}
  >
    <ProgressRing value={context?.fraction ?? 0} tone={context?.atTrigger ? 'warning' : 'muted'} size={16} />
  </RailButton>
  <RailButton
    tooltipSide="left"
    label="Toolbox"
    active={sidebar.view === 'toolbox'}
    onclick={() => sidebar.toggle('toolbox')}
  >
    <Icon icon={ToolCaseIcon} size={16} />
  </RailButton>
  <RailButton
    tooltipSide="left"
    label="Files"
    active={sidebar.view === 'files'}
    onclick={() => sidebar.toggle('files')}
  >
    <Icon icon={AiFile01Icon} size={16} />
  </RailButton>
  {#if memoryBank || sidebar.view === 'memory'}
    <RailButton
      tooltipSide="left"
      label="Memory"
      active={sidebar.view === 'memory'}
      status={memoryStatus}
      onclick={() => sidebar.toggle('memory')}
    >
      <Icon icon={BrainIcon} size={16} />
    </RailButton>
  {/if}
  <RailButton
    tooltipSide="left"
    label="Subagents & Teams"
    active={sidebar.view === 'agents'}
    status={agentsStatus}
    onclick={() => sidebar.toggle('agents')}
  >
    <Icon icon={UserGroupIcon} size={16} />
  </RailButton>
  <RailButton
    tooltipSide="left"
    label="Background Jobs"
    active={sidebar.view === 'jobs'}
    status={commandsStatus}
    onclick={() => sidebar.toggle('jobs')}
  >
    <Icon icon={TerminalIcon} size={16} />
  </RailButton>
  {#if showPlan}
    <RailButton
      tooltipSide="left"
      label="Plan"
      active={sidebar.view === 'plan'}
      status={planStatus}
      onclick={() => sidebar.toggle('plan')}
    >
      <Icon icon={TaskEdit01Icon} size={16} />
    </RailButton>
  {/if}
</div>
{#if sidebar.view !== null}
  <div
    bind:this={panelEl}
    style="width: {sidebarWidth}px;"
    class="flex min-h-0 shrink-0 flex-col overflow-hidden border-l border-edge bg-ground"
  >
    {#if sidebar.view === 'jobs'}
      <JobsPanel {controller} {sidebar} folderPath={owner.folderPath} sessionId={owner.sessionId} {now} />
    {:else if sidebar.view === 'agents'}
      <AgentsPanel {controller} {sidebar} folderPath={owner.folderPath} {now} />
    {:else if sidebar.view === 'toolbox'}
      <ToolboxPanel tools={chat?.sessionTools ?? []} toolbox={chat?.toolbox ?? null} {owner} />
    {:else if sidebar.view === 'session'}
      <SessionPanel state={chat ?? null} {controller} {owner} {turns} {personas} />
    {:else if sidebar.view === 'files'}
      <FilesPanel state={chat ?? null} {owner} />
    {:else if sidebar.view === 'plan'}
      <PlanPanel {controller} {owner} />
    {:else if sidebar.view === 'memory'}
      <MemoryPanel {owner} {turns} bank={memoryBank} {onJump} />
    {/if}
  </div>
{/if}
