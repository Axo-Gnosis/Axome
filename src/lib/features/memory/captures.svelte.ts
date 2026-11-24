import { SvelteMap } from 'svelte/reactivity'
import { backend } from '$lib/api/backend'
import type { Unsubscribe } from '$lib/api/transport'
import { notify } from '$lib/features/notifications/state.svelte'
import { openChatSession } from '$lib/features/sessions/service.svelte'
import { setSettingsOpen, setSettingsPage } from '$lib/features/settings/dialog/state.svelte'
import { openFileLocation } from '$lib/features/workbench/links/openLink'
import { openFolder } from '$lib/features/workbench/state.svelte'
import type { MemoryCaptureQuery, MemoryCaptureRun, MemoryCapturedFact } from '$lib/types/backend'
import { getErrorMessage } from '$lib/utils/client-error'
import { splitRemotePath } from '$lib/utils/paths'

/** A capture run tagged with the host it ran on (`null`: this desktop). */
export type CaptureRun = MemoryCaptureRun & { server: string | null }

// The one store of capture runs in the UI; loaders and host events upsert into it.
const runs = new SvelteMap<string, CaptureRun>()

/** Set by Review; the chat holding the run claims it (see `revealsIn`), opens its band, and clears it. */
export const reveal = $state<{ current: { server: string | null; sessionId: string; captureId: number } | null }>({
  current: null
})

function serverOf(folderPath: string | null): string | null {
  return folderPath ? (splitRemotePath(folderPath)?.server ?? null) : null
}

function hostPath(server: string | null, path: string): string {
  return server ? `axome://${server}${path}` : path
}

/** The UI path of the folder a run belongs to. */
function uiFolder(run: CaptureRun): string {
  return hostPath(run.server, run.folderPath)
}

/** The capture id a pending Review asks this chat to reveal; `null` when it targets another chat. */
export function revealsIn(folderPath: string, sessionId: string | null): number | null {
  const target = reveal.current
  return target && target.sessionId === sessionId && target.server === serverOf(folderPath) ? target.captureId : null
}

function upsert(server: string | null, run: MemoryCaptureRun): CaptureRun {
  const tagged = { ...run, server }
  runs.set(`${server ?? ''}:${run.id}`, tagged)
  return tagged
}

function sorted(filter: (run: CaptureRun) => boolean): CaptureRun[] {
  return [...runs.values()].filter(filter).sort((a, b) => a.id - b.id)
}

/** Reverted facts are excluded. */
export function factCounts(run: MemoryCaptureRun): { added: number; updated: number; known: number } {
  const live = run.facts.filter((fact) => !fact.reverted)
  return {
    added: live.filter((fact) => fact.outcome === 'new').length,
    updated: live.filter((fact) => fact.outcome === 'updated').length,
    known: live.filter((fact) => fact.outcome === 'known').length
  }
}

/** `N new · N updated · N already known`; empty when there is nothing to say. */
export function countsLabel(run: MemoryCaptureRun, known = true): string {
  const counts = factCounts(run)
  return [
    counts.added && `${counts.added} new`,
    counts.updated && `${counts.updated} updated`,
    known && counts.known && `${counts.known} already known`
  ]
    .filter(Boolean)
    .join(' · ')
}

export function runSummary(run: MemoryCaptureRun): string {
  return countsLabel(run) || 'nothing new'
}

export function runTitle(run: MemoryCaptureRun): string {
  return run.title ?? 'Untitled chat'
}

/** Ended runs that found nothing stay in Settings only. */
export function isShown(run: MemoryCaptureRun): boolean {
  return run.endedAt === null || run.facts.length > 0 || run.error !== null
}

/** The action that reverts a fact; `null` when there is nothing to revert. */
export function revertLabel(fact: MemoryCapturedFact): 'Forget' | 'Undo' | null {
  if (fact.reverted || fact.outcome === 'known') return null
  return fact.outcome === 'new' ? 'Forget' : 'Undo'
}

/** Oldest first. */
export function sessionCaptures(folderPath: string, sessionId: string | null): CaptureRun[] {
  if (!sessionId) return []
  return sorted((run) => run.sessionId === sessionId && uiFolder(run) === folderPath)
}

/** Newest first; every folder on the host of `hostFolder`. */
export function recentCaptures(hostFolder: string | null, bank: string | null): CaptureRun[] {
  const server = serverOf(hostFolder)
  return sorted((run) => run.server === server && (bank === null || run.bank === bank)).reverse()
}

export async function loadCaptures(hostFolder: string | null, query: MemoryCaptureQuery): Promise<void> {
  try {
    const loaded = await backend.memory.captures(query, hostFolder)
    for (const run of loaded) upsert(serverOf(hostFolder), run)
  } catch (error) {
    notify.error('Load memory captures failed', getErrorMessage(error))
  }
}

export async function revertFact(hostFolder: string | null, factId: number): Promise<void> {
  try {
    upsert(serverOf(hostFolder), await backend.memory.revertFact(factId, hostFolder))
  } catch (error) {
    notify.error('Memory change failed', getErrorMessage(error))
  }
}

export async function openNote(hostFolder: string | null, fact: MemoryCapturedFact): Promise<void> {
  await openFileLocation(fact.path, null, hostFolder)
}

export async function openBank(hostFolder: string | null, bank: string): Promise<void> {
  try {
    const directory = await backend.memory.ensureBank(bank, hostFolder)
    await openFolder(hostPath(serverOf(hostFolder), directory))
  } catch (error) {
    notify.error('Open memory bank failed', getErrorMessage(error))
  }
}

export function openModels(): void {
  setSettingsPage('models')
  setSettingsOpen(true)
}

/** Reopen the run's chat, opening its folder if needed; the chat then opens the run's band. */
export function reviewCapture(run: CaptureRun): void {
  setSettingsOpen(false)
  reveal.current = { server: run.server, sessionId: run.sessionId, captureId: run.id }
  try {
    // A chat tab opens its folder; `uiFolder` is the path form `sessionCaptures` matches tabs against.
    openChatSession(uiFolder(run), run.sessionId, run.title ?? 'Chat')
  } catch (error) {
    reveal.current = null
    notify.error('Open chat failed', getErrorMessage(error))
  }
}

function toast(run: CaptureRun): void {
  const title = runTitle(run)
  if (run.error) {
    notify.error(
      `Memory couldn’t save from “${title}”`,
      `${run.error} ${run.facts.length ? 'It' : 'Nothing was saved; it'} will try again next time this chat closes or compacts.`,
      { actions: run.noModel ? [{ label: 'Open Models', run: openModels }] : [] }
    )
    return
  }
  const { added, updated, known } = factCounts(run)
  const n = added + updated
  const changed = [added && `${added} new`, updated && `${updated} updated`].filter(Boolean).join(', ')
  const knownText = known === 0 ? '' : known === 1 ? ' One was already known.' : ` ${known} were already known.`
  notify.success(`Memory saved ${n} ${n === 1 ? 'fact' : 'facts'} from “${title}”`, `${changed}.${knownText}`, {
    actions: [{ label: 'Review', run: () => reviewCapture(run) }]
  })
}

export function startMemoryCaptures(): Promise<Unsubscribe> {
  return backend.memory.onCaptureChanged(({ server, capture, notify }) => {
    const run = upsert(server, capture)
    if (notify) toast(run)
  })
}
