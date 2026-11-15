import { SvelteMap } from 'svelte/reactivity'
import { backend } from '$lib/api/backend'
import type { Unsubscribe } from '$lib/api/transport'
import type { StatusTone } from '$lib/components/ui/status-dot'
import type { ChatTab } from '$lib/features/workbench/model'
import type { ChatLiveSummary } from '$lib/types/backend'

const states = new SvelteMap<string, Record<string, ChatLiveSummary>>()
const revisions = new Map<string, number>()

let subscribers = 0
let subscription: Promise<Unsubscribe> | null = null

function applySummaries(folderPath: string, summaries: ChatLiveSummary[]): void {
  const bySession: Record<string, ChatLiveSummary> = {}
  for (const summary of summaries) {
    bySession[summary.sessionId] = summary
  }
  if (states.has(folderPath)) states.set(folderPath, bySession)
}

function retain(): void {
  if (subscribers++ > 0) return
  subscription = backend.chat.onSummariesChanged((event) => {
    revisions.set(event.folderPath, (revisions.get(event.folderPath) ?? 0) + 1)
    applySummaries(event.folderPath, event.summaries)
  })
}

function unretain(): void {
  if (--subscribers > 0 || !subscription) return
  const pending = subscription
  subscription = null
  void pending.then(
    (unsubscribe) => unsubscribe(),
    () => {}
  )
}

export function activateChatSummariesFolder(folderPath: string): () => void {
  retain()
  if (!states.has(folderPath)) states.set(folderPath, {})
  const revision = (revisions.get(folderPath) ?? 0) + 1
  revisions.set(folderPath, revision)
  void backend.chat
    .liveSummaries(folderPath)
    .then((summaries) => {
      if (revisions.get(folderPath) !== revision) return
      applySummaries(folderPath, summaries)
    })
    .catch(() => {})
  let released = false
  return () => {
    if (released) return
    released = true
    unretain()
  }
}

export function releaseChatSummariesFolder(folderPath: string): void {
  revisions.set(folderPath, (revisions.get(folderPath) ?? 0) + 1)
  states.delete(folderPath)
}

export function getChatLiveSummary(folderPath: string, sessionId: string | null): ChatLiveSummary | undefined {
  return sessionId ? states.get(folderPath)?.[sessionId] : undefined
}

export function isChatBusy(summary: ChatLiveSummary): boolean {
  return summary.running || summary.activeCommands > 0 || summary.activeChildren > 0
}

export function chatSummaryTone(summary: ChatLiveSummary): StatusTone {
  if (summary.waiting || summary.autoPaused) return 'waiting'
  return isChatBusy(summary) ? 'working' : 'idle'
}

export function chatTabTone(tab: ChatTab): StatusTone {
  if (tab.status === 'failed') return 'danger'
  const summary = getChatLiveSummary(tab.folderPath, tab.sessionId)
  if (summary) return chatSummaryTone(summary)
  return tab.status === 'waiting' ? 'waiting' : tab.status === 'running' ? 'working' : 'idle'
}
