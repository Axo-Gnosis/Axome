<script lang="ts">
  import { SvelteSet } from 'svelte/reactivity'
  import { Button, IconButton } from '$lib/components/ui/button'
  import * as ContextMenu from '$lib/components/ui/context-menu'
  import { Spinner } from '$lib/components/ui/progress'
  import * as Resizable from '$lib/components/ui/resizable'
  import { SidebarNote, SidebarRow } from '$lib/components/ui/sidebar-row'
  import SidebarDivision from '$lib/features/app-shell/sidebar/SidebarDivision.svelte'
  import SidebarPanel from '$lib/features/app-shell/sidebar/SidebarPanel.svelte'
  import {
    isShown,
    loadCaptures,
    openBank,
    openModels,
    openNote,
    recentCaptures,
    revertFact,
    revertLabel,
    reviewCapture,
    runTitle,
    sessionCaptures,
    type CaptureRun
  } from '$lib/features/memory/captures.svelte'
  import { formatClock } from '$lib/features/chat/format'
  import { openOwnedPath } from '$lib/features/workbench/links/openLink'
  import Icon from '$lib/icons/Icon.svelte'
  import type { MemoryCapturedFact } from '$lib/types/backend'
  import { cn } from '$lib/utils/cn'
  import { FolderOpenIcon } from '@hugeicons/core-free-icons'
  import type { Turn } from '../turns'
  import { memoryUses } from './memoryUses'

  let {
    owner,
    turns,
    bank,
    onJump
  }: {
    owner: { folderPath: string; sessionId: string | null }
    turns: Turn[]
    /** The folder's active bank; `null` when memory is off for it. */
    bank: string | null
    onJump: (stepId: string) => void
  } = $props()

  const SIGN = {
    new: { sign: '+', tone: 'text-success' },
    updated: { sign: '~', tone: 'text-accent' },
    known: { sign: '=', tone: 'text-muted' }
  } as const

  let allBanks = $state(false)
  let selected = $state<{ runId: number; factId: number } | null>(null)
  let reverting = $state(false)
  const expanded = new SvelteSet<number>()

  const uses = $derived(memoryUses(turns))
  const saved = $derived(sessionCaptures(owner.folderPath, owner.sessionId).toReversed())
  const savedFacts = $derived(saved.flatMap((run) => run.facts.map((fact) => ({ run, fact }))))
  const running = $derived(saved.find((run) => run.endedAt === null) ?? null)
  const failed = $derived(saved[0]?.error ? saved[0] : null)
  const recent = $derived(
    recentCaptures(owner.folderPath, allBanks ? null : bank).filter(
      (run) => run.sessionId !== owner.sessionId && isShown(run)
    )
  )
  const days = $derived.by(() => {
    const groups: { label: string; runs: CaptureRun[] }[] = []
    for (const run of recent) {
      const label = dayLabel(run.startedAt)
      if (groups.at(-1)?.label === label) groups.at(-1)!.runs.push(run)
      else groups.push({ label, runs: [run] })
    }
    return groups
  })
  const detail = $derived.by(() => {
    if (!selected) return null
    const { runId, factId } = selected
    const run = [...saved, ...recent].find((run) => run.id === runId)
    const fact = run?.facts.find((fact) => fact.id === factId)
    return run && fact ? { run, fact } : null
  })

  $effect(() => {
    void loadCaptures(owner.folderPath, { sessionId: null, bank: allBanks ? null : bank, limit: 30 })
  })

  function dayLabel(timestamp: string) {
    const date = new Date(timestamp)
    const today = new Date()
    const yesterday = new Date(today)
    yesterday.setDate(today.getDate() - 1)
    if (date.toDateString() === today.toDateString()) return 'Today'
    if (date.toDateString() === yesterday.toDateString()) return 'Yesterday'
    return date.toLocaleDateString('en-US', { month: 'short', day: 'numeric', year: 'numeric' })
  }

  function leaf(note: string) {
    return note.slice(note.lastIndexOf('/') + 1)
  }

  async function revert(fact: MemoryCapturedFact) {
    reverting = true
    try {
      await revertFact(owner.folderPath, fact.id)
    } finally {
      reverting = false
    }
  }
</script>

{#snippet sign(fact: MemoryCapturedFact)}
  <span class={cn('w-3 shrink-0 font-mono', fact.reverted ? 'text-muted' : SIGN[fact.outcome].tone)}
    >{fact.reverted ? '−' : SIGN[fact.outcome].sign}</span
  >
{/snippet}

{#snippet factRow(run: CaptureRun, fact: MemoryCapturedFact, depth: number)}
  <SidebarRow
    density="compact"
    {depth}
    pressed={selected?.factId === fact.id}
    onclick={() => (selected = { runId: run.id, factId: fact.id })}
  >
    {@render sign(fact)}
    <span class={cn('min-w-0 flex-1 truncate', fact.reverted ? 'text-muted line-through' : 'text-fg')}>{fact.text}</span
    >
    <span class="shrink-0 text-xs text-subtle">{leaf(fact.note)}</span>
  </SidebarRow>
{/snippet}

{#snippet lists()}
  <div class="h-full overflow-y-auto">
    <SidebarDivision title="Used here">
      {#snippet headerActions()}
        <span class="font-mono text-xs text-subtle tabular-nums">({uses.length})</span>
      {/snippet}
      {#each uses as use (use.stepId)}
        {#if use.kind === 'search'}
          <SidebarRow density="compact" onclick={() => onJump(use.stepId)}>
            <span class="min-w-0 flex-1 truncate text-fg">“{use.query}”</span>
            {#if use.notes !== null}
              <span class="shrink-0 text-xs text-subtle">{use.notes} {use.notes === 1 ? 'note' : 'notes'}</span>
            {/if}
          </SidebarRow>
        {:else}
          <SidebarRow density="compact" onclick={() => void openOwnedPath(use.url, undefined, owner)}>
            <span class="min-w-0 flex-1 truncate font-mono text-fg">{use.note}</span>
            <span class="shrink-0 text-xs text-subtle">{use.access}</span>
          </SidebarRow>
        {/if}
      {:else}
        <SidebarNote>Nothing yet in this chat.</SidebarNote>
      {/each}
    </SidebarDivision>

    <SidebarDivision title="Saved from here">
      {#snippet headerActions()}
        <span class="font-mono text-xs text-subtle tabular-nums">({savedFacts.length})</span>
      {/snippet}
      {#if running}
        <SidebarRow behavior="display" density="compact" tone="muted">
          <Spinner size={12} class="shrink-0 text-accent" />
          <span>Saving after {running.trigger === 'compaction' ? 'compaction' : 'closing'}…</span>
        </SidebarRow>
      {/if}
      {#if failed}
        <div class="border-t border-edge/30 px-2.5 py-1.5 text-xs text-danger">
          Last save failed: {failed.error}
          {#if failed.noModel}
            <Button variant="text" size="xs" onclick={openModels}>Open Models</Button>
          {/if}
        </div>
      {/if}
      {#each savedFacts as { run, fact } (fact.id)}
        {@render factRow(run, fact, 0)}
      {:else}
        {#if !running}<SidebarNote>Nothing yet in this chat.</SidebarNote>{/if}
      {/each}
      {#if savedFacts.length}
        <SidebarNote>Next save when this chat compacts or closes.</SidebarNote>
      {/if}
    </SidebarDivision>

    <SidebarDivision title="Recent">
      {#snippet headerActions()}
        {#if bank}
          <Button variant="text" size="xs" aria-pressed={allBanks} onclick={() => (allBanks = !allBanks)}
            >{allBanks ? 'All banks' : bank}</Button
          >
        {/if}
      {/snippet}
      {#each days as day (day.label)}
        <div class="px-2.5 pt-2 pb-1 text-xs text-subtle">{day.label}</div>
        {#each day.runs as run (run.id)}
          {@const open = expanded.has(run.id)}
          <div>
            <ContextMenu.Root>
              <ContextMenu.Trigger class="contents">
                <SidebarRow
                  behavior="disclosure"
                  density="compact"
                  expanded={open}
                  onclick={() => (open ? expanded.delete(run.id) : expanded.add(run.id))}
                >
                  {#if run.endedAt === null}<Spinner size={12} class="shrink-0 text-accent" />{/if}
                  <span class={cn('min-w-0 flex-1 truncate', run.error ? 'text-danger' : 'text-fg')}
                    >{runTitle(run)}</span
                  >
                  <span class={cn('shrink-0 font-mono text-xs', run.error ? 'text-danger' : 'text-subtle')}
                    >{run.trigger === 'compaction' ? 'compaction' : 'closed'} · {formatClock(run.startedAt)}</span
                  >
                </SidebarRow>
              </ContextMenu.Trigger>
              <ContextMenu.Content>
                <ContextMenu.Item onSelect={() => reviewCapture(run)}>Open Chat</ContextMenu.Item>
              </ContextMenu.Content>
            </ContextMenu.Root>
            {#if open}
              {#if run.error}
                <div class="px-2.5 py-1 ps-6 text-xs text-danger">
                  {run.error}
                  {#if run.noModel}
                    <Button variant="text" size="xs" onclick={openModels}>Open Models</Button>
                  {/if}
                </div>
              {/if}
              {#each run.facts as fact (fact.id)}
                {@render factRow(run, fact, 1)}
              {/each}
            {/if}
          </div>
        {/each}
      {:else}
        <SidebarNote>No recent captures.</SidebarNote>
      {/each}
    </SidebarDivision>
  </div>
{/snippet}

<SidebarPanel title="Memory">
  {#snippet headerActions()}
    {#if bank}
      <span class="font-mono text-xs text-subtle">{bank}</span>
      <IconButton tooltip="Open Bank Folder" onclick={() => void openBank(owner.folderPath, bank)}>
        <Icon icon={FolderOpenIcon} size={14} />
      </IconButton>
    {/if}
  {/snippet}
  {#if detail}
    {@const { run, fact } = detail}
    {@const revertAction = revertLabel(fact)}
    <Resizable.PaneGroup direction="vertical">
      <Resizable.Pane defaultSize={60} minSize={15}>{@render lists()}</Resizable.Pane>
      <Resizable.Handle />
      <Resizable.Pane defaultSize={40} minSize={15}>
        <div class="flex h-full flex-col gap-3 overflow-y-auto p-3 text-sm">
          <div class="flex min-w-0 items-center gap-2">
            {@render sign(fact)}
            <span class="min-w-0 flex-1 truncate font-mono text-fg">{fact.note}</span>
            <Button variant="text" size="xs" onclick={() => void openNote(owner.folderPath, fact)}>Open Note</Button>
          </div>
          <div class="overflow-hidden rounded-md border border-edge bg-surface font-mono text-xs leading-normal">
            {#if fact.outcome === 'updated' && fact.previous}
              <div class="grid grid-cols-[2ch_minmax(0,1fr)] px-2 py-1 text-danger">
                <span aria-hidden="true">−</span><span class="wrap-anywhere">{fact.previous}</span>
              </div>
            {/if}
            <div
              class={cn(
                'grid grid-cols-[2ch_minmax(0,1fr)] px-2 py-1',
                fact.outcome === 'known' ? 'text-fg' : 'text-success',
                fact.reverted && 'text-muted line-through'
              )}
            >
              <span aria-hidden="true">{fact.outcome === 'known' ? ' ' : '+'}</span><span class="wrap-anywhere"
                >{fact.text}</span
              >
            </div>
          </div>
          <dl class="grid grid-cols-[auto_minmax(0,1fr)] gap-x-3 gap-y-1 text-xs">
            <dt class="text-muted">Category</dt>
            <dd class="text-fg">{fact.category}</dd>
            <dt class="text-muted">From</dt>
            <dd class="text-fg">
              {run.sessionId === owner.sessionId ? 'this chat' : runTitle(run)} · after {run.trigger === 'compaction'
                ? 'compaction'
                : 'closing'} · {formatClock(run.startedAt)}
            </dd>
            <dt class="text-muted">Bank</dt>
            <dd class="font-mono text-fg">{run.bank}</dd>
          </dl>
          {#if revertAction}
            <div>
              <Button variant="outline" size="xs" disabled={reverting} onclick={() => void revert(fact)}
                >{revertAction}</Button
              >
            </div>
          {/if}
        </div>
      </Resizable.Pane>
    </Resizable.PaneGroup>
  {:else}
    {@render lists()}
  {/if}
</SidebarPanel>
