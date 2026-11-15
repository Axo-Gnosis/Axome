<script lang="ts">
  import { Badge } from '$lib/components/ui/badge'
  import { Button } from '$lib/components/ui/button'
  import { Chip } from '$lib/components/ui/chip'
  import * as Resizable from '$lib/components/ui/resizable'
  import { SidebarNote, SidebarRow } from '$lib/components/ui/sidebar-row'
  import { StatusDot } from '$lib/components/ui/status-dot'
  import SidebarPanel from '$lib/features/app-shell/sidebar/SidebarPanel.svelte'
  import { ChatController } from '$lib/features/chat/controller.svelte'
  import { Conversation } from '$lib/features/chat/elements/conversation'
  import { PromptInput } from '$lib/features/chat/elements/prompt-input'
  import { AgentBlock, UserBlock } from '$lib/features/chat/elements/transcript'
  import { isActiveJob, jobTone, pickJob } from '$lib/features/chat/jobs'
  import { getChatModels } from '$lib/features/chat/models.svelte'
  import { transcriptTurns } from '$lib/features/chat/turns'
  import type { BlockView } from '$lib/features/chat/view'
  import { modelName } from '$lib/features/chat/format'
  import { openChatSession } from '$lib/features/sessions/service.svelte'
  import { notify } from '$lib/features/notifications/state.svelte'
  import Icon from '$lib/icons/Icon.svelte'
  import type { ChatJob, ChatJobKind } from '$lib/types/backend'
  import { getErrorMessage } from '$lib/utils/client-error'
  import { cn } from '$lib/utils/cn'
  import { ChevronRightIcon } from '@hugeicons/core-free-icons'
  import { SvelteSet } from 'svelte/reactivity'
  import type { SessionSidebarState } from './state.svelte'
  import JobRow from './JobRow.svelte'

  type ChildJob = ChatJob & { kind: Extract<ChatJobKind, { kind: 'child' }> }

  interface TeamGroup {
    id: string
    name: string
    members: ChildJob[]
    runningCount: number
  }

  let {
    controller,
    sidebar,
    folderPath,
    now
  }: {
    controller: ChatController
    sidebar: SessionSidebarState
    folderPath: string
    now: number
  } = $props()

  const collapsedTeams = new SvelteSet<string>()
  let overrides = $state<Record<string, BlockView>>({})

  const childJobs = $derived((controller.state?.live.jobs ?? []).filter((j): j is ChildJob => j.kind.kind === 'child'))

  const partitioned = $derived.by(() => {
    const teamsById = new Map<string, TeamGroup>()
    const standalone: ChildJob[] = []
    for (const job of childJobs) {
      const team = job.kind.team
      if (!team) {
        standalone.push(job)
        continue
      }
      let group = teamsById.get(team.id)
      if (!group) {
        group = { id: team.id, name: team.name ?? team.id, members: [], runningCount: 0 }
        teamsById.set(team.id, group)
      }
      group.members.push(job)
      if (isActiveJob(job.status)) group.runningCount++
    }
    return { teams: [...teamsById.values()], standalone }
  })

  const selectedChild = $derived(pickJob(childJobs, sidebar.selectedAgent))

  // Key controller identity on primitive sessionId so live progress updates don't thrash the child stream.
  const selectedChildSessionId = $derived(selectedChild?.kind.sessionId ?? null)
  const childController = $derived(
    selectedChildSessionId
      ? new ChatController({ kind: 'session', folderPath, sessionId: selectedChildSessionId })
      : null
  )

  $effect(() => {
    const ctrl = childController
    if (!ctrl) return
    void ctrl.attach().catch(() => {})
    return () => ctrl.release()
  })

  const models = $derived(getChatModels(folderPath))
  const available = $derived(models.catalog?.models ?? [])
  const childState = $derived(childController?.state ?? null)
  const childTurns = $derived(
    childState ? transcriptTurns(childState.entries, childState.streaming, childState.live) : []
  )

  // Consume Esc here even while idle, so a child's composer never stops the root chat.
  function stopChild(): void {
    if (!childState?.live.running || !childController) return
    void childController.stop().catch((error) => notify.error('Stop chat failed', getErrorMessage(error)))
  }

  function toggleTeam(teamId: string): void {
    if (collapsedTeams.has(teamId)) collapsedTeams.delete(teamId)
    else collapsedTeams.add(teamId)
  }

  function onView(stepId: string, view: BlockView): void {
    overrides[stepId] = view
  }
</script>

<SidebarPanel title="Agents">
  {#if childJobs.length === 0}
    <SidebarNote>No subagents spawned</SidebarNote>
  {:else}
    <Resizable.PaneGroup direction="vertical">
      <Resizable.Pane defaultSize={40} minSize={20}>
        <div class="h-full overflow-y-auto">
          {#each partitioned.teams as group (group.id)}
            {@const collapsed = collapsedTeams.has(group.id)}
            <SidebarRow
              behavior="disclosure"
              density="compact"
              on="surface"
              divider={false}
              expanded={!collapsed}
              class="border-b border-edge/30 bg-surface"
              onclick={() => toggleTeam(group.id)}
            >
              <Icon
                icon={ChevronRightIcon}
                size={12}
                class={cn('duration-fast shrink-0 text-muted transition-transform', !collapsed && 'rotate-90')}
              />
              <span class="min-w-0 flex-1 truncate text-xs font-medium text-bright">{group.name}</span>
              <span class="shrink-0 font-mono text-2xs text-muted tabular-nums"
                >{group.runningCount}/{group.members.length}</span
              >
            </SidebarRow>
            {#if !collapsed}
              {#each group.members as job (job.id)}
                {@render childRow(job, true)}
              {/each}
            {/if}
          {/each}
          {#each partitioned.standalone as job (job.id)}
            {@render childRow(job, false)}
          {/each}
        </div>
      </Resizable.Pane>
      <Resizable.Handle />
      <Resizable.Pane defaultSize={60} minSize={25} class="flex h-full min-h-0 flex-col bg-ground">
        {#if selectedChild && childController}
          <div class="flex shrink-0 items-center gap-2 border-b border-edge bg-surface px-2.5 py-1.5 text-xs">
            <span class="font-mono text-bright">{selectedChild.id}</span>
            {#if selectedChild.kind.agent}
              <Chip class="font-mono text-2xs">{selectedChild.kind.agent}</Chip>
            {/if}
            <div class="ms-auto flex items-center gap-1">
              <Button
                variant="ghost"
                size="xs"
                onclick={() =>
                  openChatSession(
                    folderPath,
                    selectedChild.kind.sessionId,
                    selectedChild.kind.agent ? `${selectedChild.kind.agent} (${selectedChild.id})` : selectedChild.id
                  )}>Open in Tab</Button
              >
            </div>
          </div>
          <Conversation>
            {#if !childState}
              <div class="py-4 text-center text-xs text-muted">
                {childController.connection === 'closed'
                  ? (childController.error ?? 'Failed to load subagent transcript.')
                  : 'Loading subagent transcript…'}
              </div>
            {/if}
            {#each childTurns as turn (turn.id)}
              {#if turn.user}
                <UserBlock
                  entry={turn.user}
                  controller={childController}
                  canBranch={false}
                  owner={{ folderPath, sessionId: selectedChild.kind.sessionId }}
                  commands={childState?.commands ?? []}
                  models={available}
                />
              {/if}
              {#if turn.steps.length || turn.running || turn.end}
                <AgentBlock
                  {turn}
                  controller={childController}
                  {folderPath}
                  sessionId={selectedChild.kind.sessionId}
                  canBranch={false}
                  now={turn.running ? now : 0}
                  expansion="collapsed"
                  {overrides}
                  {onView}
                />
              {/if}
            {/each}
            {#snippet footer()}
              {#if !controller.readOnly && !childController.readOnly}
                <PromptInput
                  bind:parts={childController.draft}
                  owner={{ folderPath, sessionId: selectedChild.kind.sessionId }}
                  models={available}
                  running={childState?.live.running ?? false}
                  disabled={childController.connection !== 'live'}
                  placeholder={childState?.live.running ? 'Steer subagent…' : 'Follow up with subagent…'}
                  commands={childState?.commands ?? []}
                  onCommand={(n, a) => childController.runCommand(n, a)}
                  onSubmit={() => void childController.send()}
                  onAltSubmit={() => void childController.followUp()}
                  onCancel={stopChild}
                />
              {/if}
            {/snippet}
          </Conversation>
        {/if}
      </Resizable.Pane>
    </Resizable.PaneGroup>
  {/if}
</SidebarPanel>

{#snippet childRow(job: ChildJob, indented: boolean)}
  {@const selected = selectedChild?.id === job.id}
  <JobRow
    {job}
    {selected}
    class={cn('flex-col gap-0.5', indented ? 'ps-5 pe-2.5' : 'px-2.5')}
    onSelect={() => (sidebar.selectedAgent = job.id)}
    onStop={() => void controller.killJob(job.id)}
  >
    <div class="flex min-w-0 items-center gap-1.5">
      <StatusDot size="sm" tone={jobTone(job.status)} class="shrink-0" />
      <span class={cn('shrink-0 font-mono text-xs', selected ? 'text-bright' : 'text-fg')}
        >{job.kind.team?.member ?? job.id}</span
      >
      <Chip class="shrink-0 font-mono text-2xs">{job.kind.agent ?? 'subagent'}</Chip>
      <span class="min-w-0 flex-1 truncate text-2xs text-subtle">{modelName(job.kind.model)}</span>
      {#if job.kind.unreadMail > 0}
        <Badge variant="accent" class="shrink-0 px-1.5 py-0 text-2xs normal-case">{job.kind.unreadMail} mail</Badge>
      {/if}
    </div>
    {#if job.kind.progress}
      <div class="flex min-w-0 items-center gap-1.5 ps-3">
        <span class="shrink-0 font-mono text-2xs text-muted tabular-nums"
          >{job.kind.progress.turns}/{job.kind.progress.maxTurns}t</span
        >
        {#if job.kind.progress.tools.length}
          <span class="min-w-0 flex-1 truncate font-mono text-2xs text-accent"
            >{job.kind.progress.tools.join(', ')}</span
          >
        {/if}
      </div>
    {/if}
  </JobRow>
{/snippet}
