<script lang="ts">
  import { backend } from '$lib/api/backend'
  import MarkdownRenderer from '$lib/components/markdown/MarkdownRenderer.svelte'
  import { Button } from '$lib/components/ui/button'
  import { SidebarNote } from '$lib/components/ui/sidebar-row'
  import SidebarPanel from '$lib/features/app-shell/sidebar/SidebarPanel.svelte'
  import type { ChatController } from '$lib/features/chat/controller.svelte'
  import { record } from '$lib/features/chat/elements/tool/payload'
  import { openOwnedPath, resolveFileLocationPath } from '$lib/features/workbench/links/openLink'
  import Icon from '$lib/icons/Icon.svelte'
  import { getErrorMessage } from '$lib/utils/client-error'
  import { PencilEdit02Icon } from '@hugeicons/core-free-icons'
  import { ledgerFiles } from './ledger'
  import PlanReview from './PlanReview.svelte'

  let {
    controller,
    owner
  }: {
    controller: ChatController
    owner: { folderPath: string; sessionId: string | null }
  } = $props()

  const LOCAL = 'local://'
  // ponytail: 2s stat poll while visible; switch to a watcher if drafts get large
  const POLL_MS = 2000

  const chat = $derived(controller.state)
  const review = $derived(chat?.live.reviews.find((candidate) => candidate.plan) ?? null)
  const result = $derived(record(review?.result))

  // The local:// root, for recognising drafts among ledger paths while no review names one.
  let root = $state<{ key: string; path: string } | null>(null)
  let rootError = $state<{ key: string; message: string } | null>(null)
  const ownerKey = $derived(JSON.stringify([owner.folderPath, owner.sessionId]))
  const localRoot = $derived(root?.key === ownerKey ? root.path : null)

  $effect(() => {
    const key = ownerKey
    const folder = owner.folderPath
    const sessionId = owner.sessionId
    if (!sessionId) return
    let stale = false
    backend.chat.localFiles(folder, sessionId).then(
      (local) => {
        if (!stale) root = { key, path: local.root }
      },
      (reason) => {
        if (!stale) rootError = { key, message: getErrorMessage(reason) }
      }
    )
    return () => {
      stale = true
    }
  })

  /** The review's draft, else the latest markdown file written under local:// since the last mode change. */
  const draftUrl = $derived.by((): string | null => {
    if (review) return typeof result.file === 'string' && result.file ? result.file : null
    if (!chat || !localRoot) return null
    const prefix = `${localRoot}/`
    const since = chat.entries.findLastIndex((entry) => entry.body.kind === 'modeChange')
    const written = ledgerFiles(chat.entries).findLast(
      (file) =>
        file.index > since && file.access === 'modify' && file.path.startsWith(prefix) && file.path.endsWith('.md')
    )
    return written ? `${LOCAL}${written.path.slice(prefix.length)}` : null
  })

  type Draft = { url: string; source: string | null; missing: boolean; error: string | null }
  let draft = $state<Draft | null>(null)
  const shown = $derived(draft?.url === draftUrl ? draft : null)

  $effect(() => {
    const url = draftUrl
    const folder = owner.folderPath
    const sessionId = owner.sessionId
    if (!url || !sessionId) return
    let stale = false
    let timer: ReturnType<typeof setTimeout> | undefined
    let file: { dir: string; name: string } | null = null
    // The stat identity of the bytes on screen; a new one triggers a re-read.
    let version: string | null = null
    // The failure last probed against the local:// listing, and its verdict.
    let failure: string | null = null
    let missing = false
    const show = (patch: Partial<Draft>) => {
      const base = draft?.url === url ? draft : { url, source: null, missing: false, error: null }
      draft = { ...base, ...patch }
    }

    const tick = async (): Promise<void> => {
      try {
        // A hidden window skips the stat; the next visible tick catches up.
        if (document.visibilityState !== 'hidden') {
          file ??= await locate(folder, sessionId, url)
          if (stale) return
          const stat = await backend.files.stat(file.dir, file.name)
          if (stale) return
          if (!stat.regular) throw new Error(`Not a regular file: ${url}`)
          failure = null
          if (stat.version !== version) {
            const read = await backend.files.read(file.dir, file.name)
            if (stale) return
            version = stat.version
            show({ source: read.content, missing: false, error: null })
          }
        }
      } catch (reason) {
        if (stale) return
        version = null
        const message = getErrorMessage(reason)
        if (message !== failure) {
          missing = await isMissing(folder, sessionId, url)
          if (stale) return
          failure = message
        }
        show(missing ? { source: null, missing: true, error: null } : { error: readError(reason) })
      }
      if (!stale) timer = setTimeout(tick, POLL_MS)
    }

    void tick()
    return () => {
      stale = true
      clearTimeout(timer)
    }
  })

  /** Resolves the draft URL through the session's schemes, as opening it in a tab does. */
  async function locate(folder: string, sessionId: string, url: string): Promise<{ dir: string; name: string }> {
    const target = await backend.chat.resolveLink(folder, sessionId, url)
    if (target.kind !== 'file' || target.isDir) throw new Error(`Not a file: ${url}`)
    const path = await resolveFileLocationPath(target.path, folder)
    const slash = path.lastIndexOf('/')
    return { dir: path.slice(0, slash) || '/', name: path.slice(slash + 1) }
  }

  /** True only when the local:// listing proves the draft is absent; any other failure stays an error. */
  async function isMissing(folder: string, sessionId: string, url: string): Promise<boolean> {
    if (!url.startsWith(LOCAL)) return false
    const path = url.slice(LOCAL.length).replace(/^\/+/, '')
    try {
      const local = await backend.chat.localFiles(folder, sessionId)
      return !local.files.some((file) => file.path === path)
    } catch {
      return false
    }
  }

  function readError(reason: unknown): string {
    if ((reason as { kind?: unknown } | null)?.kind === 'tooLarge') return 'The plan draft is too large to preview.'
    return getErrorMessage(reason)
  }
</script>

<SidebarPanel title="Plan">
  {#snippet headerActions()}
    {#if draftUrl}
      <Button
        variant="text"
        size="xs"
        class="gap-1.5 px-1.5"
        onclick={() => draftUrl && void openOwnedPath(draftUrl, undefined, owner)}
      >
        <Icon icon={PencilEdit02Icon} size={12} />
        Edit
      </Button>
    {/if}
  {/snippet}
  <div class="flex h-full min-h-0 flex-col">
    <div class="min-h-0 flex-1 overflow-y-auto">
      {#if !draftUrl}
        {#if !review && rootError?.key === ownerKey}
          <SidebarNote>{rootError.message}</SidebarNote>
        {:else}
          <SidebarNote>No plan draft yet.</SidebarNote>
        {/if}
      {:else if !shown}
        <SidebarNote>Loading plan draft…</SidebarNote>
      {:else}
        {#if shown.missing}
          <SidebarNote>No plan draft yet.</SidebarNote>
        {:else if shown.error}
          <SidebarNote>{shown.error}</SidebarNote>
        {/if}
        {#if shown.source !== null}
          <div class="flex flex-col gap-2 px-3 py-2">
            <span class="truncate font-mono text-xs text-subtle" title={draftUrl}>{draftUrl}</span>
            <MarkdownRenderer source={shown.source} folderPath={owner.folderPath} sessionId={owner.sessionId} />
          </div>
        {/if}
      {/if}
    </div>
    {#if review}
      {#key review.callId}
        <PlanReview {review} {controller} {owner} />
      {/key}
    {/if}
  </div>
</SidebarPanel>
