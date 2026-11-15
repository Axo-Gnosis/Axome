<script lang="ts">
  import { Button } from '$lib/components/ui/button'
  import * as Resizable from '$lib/components/ui/resizable'
  import { SidebarNote } from '$lib/components/ui/sidebar-row'
  import { StatusDot } from '$lib/components/ui/status-dot'
  import SidebarPanel from '$lib/features/app-shell/sidebar/SidebarPanel.svelte'
  import type { ChatController } from '$lib/features/chat/controller.svelte'
  import { formatCommandExit } from '$lib/features/chat/elements/tool/bashOutput'
  import { formatDuration } from '$lib/features/chat/format'
  import { jobTone, pickJob } from '$lib/features/chat/jobs'
  import { openLink } from '$lib/features/workbench/links/openLink'
  import type { ChatJob, ChatJobKind, ChatPromotion } from '$lib/types/backend'
  import { cn } from '$lib/utils/cn'
  import JobRow from './JobRow.svelte'
  import type { SessionSidebarState } from './state.svelte'

  type CommandJob = ChatJob & { kind: Extract<ChatJobKind, { kind: 'command' }> }

  let {
    controller,
    sidebar,
    folderPath,
    sessionId,
    now
  }: {
    controller: ChatController
    sidebar: SessionSidebarState
    folderPath: string
    sessionId: string | null
    now: number
  } = $props()

  const commandJobs = $derived(
    (controller.state?.live.jobs ?? []).filter((j): j is CommandJob => j.kind.kind === 'command')
  )

  const selectedJob = $derived(pickJob(commandJobs, sidebar.selectedJob))

  // Key the watch on the primitive id so live updates don't restart it.
  const selectedJobId = $derived(selectedJob?.id ?? null)

  $effect(() => {
    const jobId = selectedJobId
    if (controller.connection !== 'live' || !controller.attachId) return
    void controller.watchJob(jobId)
    return () => {
      void controller.watchJob(null)
    }
  })

  function promotionLabel(promotion: ChatPromotion | null): string {
    if (promotion === 'host') return 'user'
    return promotion ?? 'helpers'
  }

  function statusSummary(job: CommandJob): { text: string; toneClass: string } {
    if (job.status === 'running') return { text: 'Running', toneClass: 'text-muted' }
    if (job.status === 'queued') return { text: 'Queued', toneClass: 'text-muted' }
    if (job.status === 'helpersRunning') return { text: 'Helpers running', toneClass: 'text-muted' }
    if (job.status === 'interrupted') return { text: 'Interrupted', toneClass: 'text-warning' }
    if (job.kind.exit) {
      return {
        text: formatCommandExit(job.kind.exit).label,
        toneClass: job.status === 'failed' ? 'text-danger' : 'text-muted'
      }
    }
    if (job.status === 'failed') return { text: 'Failed', toneClass: 'text-danger' }
    return { text: 'Finished', toneClass: 'text-muted' }
  }
</script>

<SidebarPanel title="Jobs">
  {#if commandJobs.length === 0}
    <SidebarNote>No background jobs</SidebarNote>
  {:else}
    <Resizable.PaneGroup direction="vertical">
      <Resizable.Pane defaultSize={45} minSize={20}>
        <div class="h-full overflow-y-auto">
          {#each commandJobs as job (job.id)}
            {@const selected = selectedJob?.id === job.id}
            {@const elapsed = formatDuration(Math.max(0, (job.kind.endedAt ?? now) - job.startedAt))}
            <JobRow
              {job}
              {selected}
              class="items-center gap-1.5 px-2.5"
              onSelect={() => (sidebar.selectedJob = job.id)}
              onStop={() => void controller.killJob(job.id)}
            >
              <StatusDot size="sm" tone={jobTone(job.status)} />
              <span class="shrink-0 font-mono text-xs text-muted">{job.id}</span>
              <span class={cn('min-w-0 flex-1 truncate font-mono text-xs', selected ? 'text-bright' : 'text-fg')}
                >{job.kind.command}</span
              >
              <span class="shrink-0 font-mono text-2xs text-subtle tabular-nums"
                >{promotionLabel(job.kind.promotion)} · {elapsed}</span
              >
            </JobRow>
          {/each}
        </div>
      </Resizable.Pane>
      <Resizable.Handle />
      <Resizable.Pane defaultSize={55} minSize={20} class="flex h-full min-h-0 flex-col">
        {#if selectedJob}
          {@const output = controller.jobOutputs[selectedJob.id] ?? null}
          {@const summary = statusSummary(selectedJob)}
          <div class="flex shrink-0 flex-wrap items-center gap-2 border-b border-edge bg-surface px-2.5 py-1.5 text-xs">
            <span class="font-mono text-bright">{selectedJob.id}</span>
            <span class={summary.toneClass}>{summary.text}</span>
            {#if output?.truncated}
              <span class="text-subtle">Showing tail (128 KB)</span>
            {/if}
            {#if output?.captureFailed}
              <span class="text-muted">Output capture incomplete</span>
            {/if}
            <Button
              type="button"
              variant="text"
              size="xs"
              class="ms-auto px-0"
              onclick={() => void openLink(`artifact://${selectedJob.kind.artifact}`, { folderPath, sessionId })}
              >Open Full Output</Button
            >
          </div>
          <div class="min-h-0 flex-1 overflow-y-auto bg-ground p-2.5">
            {#if output?.chunks.length}
              <pre
                class="m-0 font-mono text-xs wrap-anywhere whitespace-pre-wrap text-fg">{#each output.chunks as chunk}<span
                    class:text-danger={chunk.stream === 'stderr'}>{chunk.text}</span
                  >{/each}</pre>
            {:else}
              <div class="font-mono text-xs text-subtle">
                {selectedJob.status === 'running' || selectedJob.status === 'helpersRunning'
                  ? 'No output yet'
                  : output?.captureFailed
                    ? 'Output unavailable'
                    : 'No output'}
              </div>
            {/if}
          </div>
        {/if}
      </Resizable.Pane>
    </Resizable.PaneGroup>
  {/if}
</SidebarPanel>
