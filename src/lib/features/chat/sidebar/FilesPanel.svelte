<script lang="ts">
  import { backend } from '$lib/api/backend'
  import FileTreeItems from '$lib/components/file-tree/FileTreeItems.svelte'
  import { SidebarNote } from '$lib/components/ui/sidebar-row'
  import { InfoTooltip } from '$lib/components/ui/tooltip'
  import SidebarDivision from '$lib/features/app-shell/sidebar/SidebarDivision.svelte'
  import SidebarPanel from '$lib/features/app-shell/sidebar/SidebarPanel.svelte'
  import { record, pathText } from '$lib/features/chat/elements/tool/payload'
  import GitStatusBadge from '$lib/features/git/GitStatusBadge.svelte'
  import { getGitSummary } from '$lib/features/git/state.svelte'
  import { openOwnedPath } from '$lib/features/workbench/links/openLink'
  import type { ChatLocalFiles, ChatState } from '$lib/types/backend'
  import { getErrorMessage } from '$lib/utils/client-error'
  import { buildFileTree, type FileTreeNode } from '$lib/utils/fileTree'
  import { SvelteSet } from 'svelte/reactivity'

  type SessionFile = { path: string; openPath: string; size?: number; status?: string }

  let {
    state: chat,
    folderPath,
    owner
  }: {
    state: ChatState | null
    folderPath: string
    owner: { folderPath: string; sessionId: string | null }
  } = $props()

  const sessionId = $derived(chat?.sessionId)
  const running = $derived(chat?.live.running ?? false)
  let loaded = $state<{ sessionId: string; folder: string; local: ChatLocalFiles } | null>(null)
  let error = $state<string | null>(null)
  let loading = $state(false)
  const local = $derived(loaded?.sessionId === sessionId && loaded?.folder === folderPath ? loaded.local : null)
  const localFiles = $derived(local?.files ?? [])

  $effect(() => {
    const id = sessionId
    const folder = folderPath
    void running
    error = null
    loading = false
    if (!id) return
    let stale = false
    loading = true
    backend.chat.localFiles(folder, id).then(
      (result) => {
        if (!stale) {
          loaded = { sessionId: id, folder, local: result }
          loading = false
        }
      },
      (reason) => {
        if (!stale) {
          error = getErrorMessage(reason)
          loading = false
        }
      }
    )
    return () => {
      stale = true
    }
  })

  const files = $derived.by(() => {
    // local:// files have their own group; the ledger records them by absolute session path.
    const localPrefix = local?.root ? `${local.root}/` : null
    const paths = new Map<string, boolean>()
    for (const { body } of chat?.entries ?? []) {
      if (body.kind !== 'toolResult' || body.isError) continue
      const ledger = record(record(body.details).ledger)
      if (!pathText(ledger.path) || typeof ledger.sha256 !== 'string') continue
      if (localPrefix && ledger.path.startsWith(localPrefix)) continue
      const edited = paths.get(ledger.path) === true || ledger.access === 'modify'
      paths.set(ledger.path, edited)
    }
    const recorded = [...paths]
    return [
      { name: 'Edited', paths: recorded.filter(([, edited]) => edited).map(([path]) => path) },
      { name: 'Read', paths: recorded.filter(([, edited]) => !edited).map(([path]) => path) }
    ]
  })
  const statuses = $derived.by(() => {
    const summary = getGitSummary(folderPath)
    const result = new Map<string, string>()
    if (summary?.is_repo) {
      for (const change of summary.changes) result.set(change.path, change.status)
      for (const conflict of summary.conflicts) result.set(conflict.path, conflict.kind)
    }
    return result
  })
  const prefix = $derived(`${folderPath.replace(/\/+$/, '')}/`)
  const empty = $derived(!localFiles.length && files.every((group) => !group.paths.length))
  const groups = $derived.by(() => {
    const payloads: { name: string; files: SessionFile[] }[] = [
      {
        name: 'Local',
        files: localFiles.map((file) => ({ path: file.path, openPath: `local://${file.path}`, size: file.size }))
      },
      ...files.map((group) => ({
        name: group.name,
        files: group.paths.map((openPath) => {
          const relative = openPath.startsWith(prefix) ? openPath.slice(prefix.length) : null
          return {
            path: relative ?? openPath,
            openPath,
            status: relative === null ? undefined : statuses.get(relative)
          }
        })
      }))
    ]
    return payloads.map((group) => {
      const nodes = buildFileTree(group.files)
      // Absolute paths have an empty root segment; keep its key distinct from project-relative paths.
      for (const node of nodes) {
        if (node.type === 'directory' && node.name === '') {
          node.name = '/'
          node.lowerName = '/'
        }
      }
      return { name: group.name, count: group.files.length, nodes }
    })
  })
  const collapsedDirs = new SvelteSet<string>()

  function directoryKey(group: string, path: string): string {
    return JSON.stringify([sessionId, folderPath, group, path])
  }

  function toggleDirectory(group: string, path: string): void {
    const key = directoryKey(group, path)
    if (collapsedDirs.has(key)) collapsedDirs.delete(key)
    else collapsedDirs.add(key)
  }

  function sizeLabel(bytes: number): string {
    if (bytes < 1024) return `${bytes} B`
    if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KiB`
    return `${(bytes / (1024 * 1024)).toFixed(1)} MiB`
  }
</script>

<SidebarPanel title="Files">
  {#snippet headerActions()}
    <InfoTooltip ariaLabel="About file status">Git status shows the file's current state.</InfoTooltip>
  {/snippet}
  <div class="h-full overflow-y-auto">
    {#if empty && !loading && !error}
      <SidebarNote>No files yet.</SidebarNote>
    {:else}
      {#each groups as group (group.name)}
        <SidebarDivision title={group.name}>
          {#snippet headerActions()}
            <span class="font-mono text-xs text-subtle tabular-nums">({group.count})</span>
          {/snippet}
          {#if group.name === 'Local' && error}
            <SidebarNote>{error}</SidebarNote>
          {:else if group.name === 'Local' && loading && !group.count}
            <SidebarNote>Loading local files…</SidebarNote>
          {/if}
          {#if group.count}
            <FileTreeItems
              nodes={group.nodes}
              isCollapsed={(node) => collapsedDirs.has(directoryKey(group.name, node.path))}
              onToggleDir={(path) => toggleDirectory(group.name, path)}
              onFileClick={(node) => {
                if (node.change) void openOwnedPath(node.change.openPath, undefined, owner)
              }}
            >
              {#snippet fileTrailing(node: FileTreeNode<SessionFile>)}
                {#if node.change?.size !== undefined}
                  <span class="font-mono text-xs text-muted tabular-nums">{sizeLabel(node.change.size)}</span>
                {/if}
                {#if node.change?.status}<GitStatusBadge status={node.change.status} />{/if}
              {/snippet}
            </FileTreeItems>
          {:else if group.name !== 'Local' || (!loading && !error)}
            <SidebarNote>
              {group.name === 'Local' ? 'No local:// files.' : `No ${group.name.toLowerCase()} files.`}
            </SidebarNote>
          {/if}
        </SidebarDivision>
      {/each}
    {/if}
  </div>
</SidebarPanel>
