<script lang="ts">
  import { backend } from '$lib/api/backend'
  import { Button } from '$lib/components/ui/button'
  import { DisclosureChevron } from '$lib/components/ui/disclosure-chevron'
  import { formatCount } from '$lib/features/chat/format'
  import { partsText } from '$lib/features/chat/parts/parts'
  import type { Turn } from '$lib/features/chat/turns'
  import {
    applySquashedChanges,
    summarizeTurnChanges,
    turnFileChanges,
    type TurnChangedFile
  } from '$lib/features/chat/turnChanges'
  import type { TurnDiffFile } from '$lib/features/workbench/model'
  import { openTurnDiff } from '$lib/features/workbench/surfaces/diff/service.svelte'
  import FileIcon from '$lib/icons/FileIcon.svelte'
  import { cn } from '$lib/utils/cn'
  import { disclose } from './disclose'

  let {
    turn,
    folderPath,
    sessionId
  }: {
    turn: Turn
    folderPath: string
    sessionId: string | null
  } = $props()

  const changes = $derived(turnFileChanges(turn.steps, folderPath))
  const label = $derived.by(() => {
    const prompt = turn.user ? partsText(turn.user.body.parts).replace(/\s+/g, ' ').trim() : ''
    if (!prompt) return 'Turn Changes'
    return prompt.length > 28 ? `${prompt.slice(0, 27)}…` : prompt
  })

  let open = $state(false)
  let squashed = $state.raw<{ key: string; files: TurnChangedFile[] } | null>(null)

  const changeKey = $derived(changes.files.map((file) => `${file.key}:${file.afterSha}:${file.mutations}`).join('|'))
  const files = $derived(squashed?.key === changeKey ? squashed.files : changes.files)
  const summary = $derived(summarizeTurnChanges(files))

  $effect(() => {
    const key = changeKey
    const raw = changes.raw
    const folder = folderPath
    const session = sessionId
    if (!changes.needsBlobSquash || !folder || !session) return
    let cancelled = false
    void backend.chat
      .squashChanges(folder, session, raw)
      .then((result) => {
        if (cancelled) return
        squashed = { key, files: applySquashedChanges(result, folder) }
      })
      .catch((cause) => console.warn('Squashing turn changes failed', cause))
    return () => {
      cancelled = true
    }
  })

  function review(initialFile: string | null) {
    if (!sessionId) return
    const entries: TurnDiffFile[] = files.map((file) => ({
      path: file.displayPath,
      created: file.created,
      beforeSha: file.beforeSha,
      afterSha: file.afterSha,
      binary: file.binary,
      omitted: file.omitted,
      additions: file.additions,
      deletions: file.deletions
    }))
    openTurnDiff(folderPath, sessionId, turn.id, label, entries, initialFile)
  }
</script>

{#if changes.files.length > 0}
  <div data-slot="turn-changes-card" class="overflow-hidden rounded-lg border border-edge bg-surface">
    <div class="flex min-w-0 items-center gap-2 px-3 py-1.5">
      <button
        type="button"
        aria-expanded={open}
        onclick={() => (open = !open)}
        class="flex min-w-0 flex-1 cursor-pointer items-center gap-2 rounded text-left text-sm text-fg transition-colors hover:text-bright focus-visible:shadow-focus-ring focus-visible:outline-none"
      >
        <DisclosureChevron {open} size={14} class="text-muted" />
        <span class="truncate font-medium">{formatCount(files.length, 'file')} changed</span>
        <span class="flex shrink-0 items-center gap-1.5 font-mono text-xs tabular-nums">
          {#if summary.additions > 0}<span class="text-success">+{summary.additions}</span>{/if}
          {#if summary.deletions > 0}<span class="text-danger">-{summary.deletions}</span>{/if}
          {#if summary.hasUnknownCounts}<span class="text-subtle">…</span>{/if}
        </span>
      </button>
      {#if sessionId}
        <Button type="button" variant="outline" size="xs" onclick={() => review(null)}>Review</Button>
      {/if}
    </div>
    {#if open}
      <div
        data-disclose
        transition:disclose
        class="flex flex-col divide-y divide-edge/50 border-t border-edge bg-ground"
      >
        {#each files as file (file.key)}
          <button
            type="button"
            disabled={!sessionId}
            onclick={() => review(file.displayPath)}
            class="flex w-full min-w-0 cursor-pointer items-center gap-2 px-3 py-1.5 text-left hover:bg-surface focus-visible:shadow-focus-ring focus-visible:outline-none disabled:cursor-default"
          >
            <span
              class={cn('shrink-0 font-mono text-2xs font-bold', file.created ? 'text-success' : 'text-accent')}
              title={file.created ? 'Added' : 'Modified'}>{file.created ? 'A' : 'M'}</span
            >
            <FileIcon filename={file.displayPath} size={13} />
            <span class="min-w-0 flex-1 truncate font-mono text-xs text-fg" title={file.displayPath}
              >{file.displayPath}</span
            >
            {#if file.discontinuous}
              <span class="shrink-0 text-2xs text-warning" title="Modified outside file tools mid-turn">gapped</span>
            {/if}
            {#if file.binary}
              <span class="shrink-0 text-2xs text-subtle">binary</span>
            {:else if file.omitted}
              <span class="shrink-0 text-2xs text-subtle">&gt;4 MiB</span>
            {/if}
            <span class="flex shrink-0 items-center gap-1 font-mono text-2xs tabular-nums">
              {#if (file.additions ?? 0) > 0}<span class="text-success">+{file.additions}</span>{/if}
              {#if (file.deletions ?? 0) > 0}<span class="text-danger">-{file.deletions}</span>{/if}
            </span>
          </button>
        {/each}
      </div>
    {/if}
  </div>
{/if}
