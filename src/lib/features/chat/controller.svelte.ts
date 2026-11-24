import { backend } from '$lib/api/backend'
import type { StreamHandle } from '$lib/api/transport'
import { notify } from '$lib/features/notifications/state.svelte'
import { createExtensionsState, type ExtensionsState } from '$lib/features/settings/state/extensions.svelte'
import type { ChatTab, TabId } from '$lib/features/workbench/model'
import {
  addChatTab,
  getTabs,
  requestFocusTab,
  setChatTabSessionId,
  setChatTabStatus,
  setChatTabTitle
} from '$lib/features/workbench/state.svelte'
import type {
  ChatAnswer,
  ChatBlock,
  ChatEntry,
  ChatEvent,
  ChatJobOutput,
  ChatPart,
  ChatPlanThen,
  ChatQueued,
  ChatState
} from '$lib/types/backend'
import { getErrorMessage } from '$lib/utils/client-error'
import { hostRoot } from '$lib/utils/paths'
import { getOrCreate } from './registry'
import { askKey, createAskDraft, validAskBlocks, type AskDraft } from './elements/ask/askDocument'
import { getChatModels, loadChatModels, MODELS_HINT } from './models.svelte'
import { CONTENT_LIMIT, CONTENT_LIMIT_MESSAGE, contentBytes, hasContent, prependQueuedParts } from './parts/parts'

export type ChatTarget = { kind: 'tab'; tabId: TabId } | { kind: 'session'; folderPath: string; sessionId: string }

type JobOutputEvent = Extract<ChatEvent, { type: 'jobOutput' }>

function mergeJobOutput(prev: ChatJobOutput | undefined, event: JobOutputEvent): ChatJobOutput {
  const chunks = event.reset || !prev ? [] : prev.chunks.map((chunk) => ({ ...chunk }))
  for (const chunk of event.output.chunks) {
    const last = chunks.at(-1)
    if (last?.stream === chunk.stream) last.text += chunk.text
    else chunks.push({ ...chunk })
  }
  const MAX_CHARS = 131_072
  let excess = chunks.reduce((sum, chunk) => sum + chunk.text.length, 0) - MAX_CHARS
  const clamped = excess > 0
  while (excess > 0 && chunks.length) {
    const first = chunks[0]
    if (first.text.length <= excess) {
      excess -= first.text.length
      chunks.shift()
    } else {
      // Don't leave a low surrogate at the capped head.
      const code = first.text.charCodeAt(excess)
      first.text = first.text.slice(excess + (code >= 0xdc00 && code <= 0xdfff ? 1 : 0))
      break
    }
  }
  return {
    chunks,
    captureFailed: event.output.captureFailed,
    truncated: event.reset ? event.output.truncated : prev?.truncated || event.output.truncated || clamped,
    done: event.output.done
  }
}

export class ChatController {
  readonly #target: ChatTarget
  state = $state<ChatState | null>(null)
  connection = $state<'detached' | 'attaching' | 'live' | 'reconnecting' | 'closed'>('detached')
  error = $state<string | null>(null)
  draft = $state<ChatPart[]>([])
  editing = $state<{ entryId: string; anchor: string | null; parts: ChatPart[] } | null>(null)
  jobOutputs = $state<Record<string, ChatJobOutput>>({})
  askDrafts = $state<Record<string, AskDraft>>({})
  #extensions = $state.raw<ExtensionsState | null>(null)
  readonly #extensionsFolder = $derived(this.state && !this.readOnly ? this.folderPath : null)
  #dismissedDiagnostics = $state.raw<string[]>([])
  #askSessionId: string | null = null
  /** Parts sent as the first message once the first attach succeeds; `newChat` sets it. */
  seed: ChatPart[] | null = null
  #handle: StreamHandle<void> | null = null
  #generation = 0
  #closing = false
  #attachId = $state<string | null>(null)
  #watchSeq = 0
  #watchChain: Promise<void> = Promise.resolve()
  // The job this attachment watches; `pending` buffers deltas until the tail response lands.
  #watch: { jobId: string; pending: JobOutputEvent[] | null } | null = null

  constructor(target: ChatTarget) {
    this.#target = target
  }

  get #tab(): ChatTab | undefined {
    const target = this.#target
    if (target.kind !== 'tab') return undefined
    const tab = getTabs().find((candidate) => candidate.id === target.tabId)
    return tab?.kind === 'chat' ? tab : undefined
  }

  get folderPath(): string | null {
    return this.#target.kind === 'tab' ? (this.#tab?.folderPath ?? null) : this.#target.folderPath
  }

  get sessionId(): string | null {
    return this.#target.kind === 'tab' ? (this.#tab?.sessionId ?? null) : this.#target.sessionId
  }

  get busy(): boolean {
    return !!this.state && (this.state.live.running || this.state.live.maintenance !== null)
  }

  get attachId(): string | null {
    return this.#attachId
  }

  get readOnly(): boolean {
    return this.state?.readOnly ?? false
  }

  get extensions(): ExtensionsState | null {
    return this.#extensions
  }

  holdExtensions(): (() => void) | undefined {
    const folderPath = this.#extensionsFolder
    if (!folderPath) return
    const state = createExtensionsState(hostRoot(folderPath), folderPath)
    this.#extensions = state
    return () => {
      if (this.#extensions === state) this.#extensions = null
      state.release()
    }
  }

  get extensionDiagnostics(): string[] {
    return [...new Set(this.state?.extensionDiagnostics)].filter((line) => !this.#dismissedDiagnostics.includes(line))
  }

  dismissExtensionDiagnostic(diagnostic: string): void {
    this.#dismissedDiagnostics = [...this.#dismissedDiagnostics, diagnostic]
  }

  #reconcileExtensionDiagnostics(): void {
    if (this.#dismissedDiagnostics.length) {
      this.#dismissedDiagnostics = this.#dismissedDiagnostics.filter((line) =>
        this.state?.extensionDiagnostics.includes(line)
      )
    }
  }

  async attach(): Promise<void> {
    this.release()
    const folderPath = this.folderPath
    if (!folderPath) return
    let sessionId = this.sessionId
    if (this.#target.kind === 'session' && !sessionId) return
    this.error = null
    // A reconnect keeps the transcript it shows until the new snapshot replaces it.
    if (this.state?.sessionId !== sessionId) this.state = null
    this.connection = 'attaching'
    const generation = this.#generation
    try {
      if (sessionId) void loadChatModels(folderPath)
      else {
        await loadChatModels(folderPath)
        if (generation !== this.#generation) return
        const { catalog, error } = getChatModels(folderPath)
        const model = catalog?.defaultModel ?? catalog?.models[0]?.selector
        if (!catalog || !model) throw new Error(error ?? `No axo models are available. ${MODELS_HINT}`)
        sessionId = await backend.chat.create(folderPath, model, catalog.defaultThinking, catalog.defaultServiceTier)
        // Kept even when superseded: the next attach reuses it, and the backend reclaims an unattached one.
        if (this.#target.kind === 'tab') setChatTabSessionId(this.#target.tabId, sessionId)
        if (generation !== this.#generation) return
      }
      const attachId = crypto.randomUUID()
      this.#attachId = attachId
      const handle = backend.chat.attach(
        { attachId, folderPath, sessionId },
        {
          onEvent: (event) => {
            if (generation === this.#generation) this.apply(event)
          }
        }
      )
      this.#handle = handle
      await handle.ready
      const seed = this.seed
      if (seed && generation === this.#generation) {
        // Taken before sending so a reattach never repeats it; `send` keeps the draft when it fails.
        this.seed = null
        this.draft = seed
        await this.send()
      }
    } catch (error) {
      if (generation !== this.#generation) return
      this.#attachId = null
      this.connection = 'closed'
      this.error = getErrorMessage(error)
      if (this.#target.kind === 'tab') setChatTabStatus(this.#target.tabId, 'failed')
      throw error
    }
  }

  apply(event: ChatEvent): void {
    switch (event.type) {
      case 'snapshot':
        if (this.state?.sessionId !== event.state.sessionId) this.#dismissedDiagnostics = []
        this.state = event.state
        this.#reconcileAsks()
        this.#reconcileExtensionDiagnostics()
        this.connection = 'live'
        this.error = null
        if (this.#target.kind === 'tab') setChatTabTitle(this.#target.tabId, event.state.live.title ?? 'New Chat')
        break
      case 'entry':
        if (this.state) {
          this.state.entries.push(event.entry)
          if (event.entry.body.kind === 'assistant') this.state.streaming = null
        }
        break
      case 'live':
        if (this.state) {
          this.state.live = event.live
          this.#reconcileAsks()
        }
        if (this.#target.kind === 'tab') setChatTabTitle(this.#target.tabId, event.live.title ?? 'New Chat')
        break
      case 'sessionTools':
        if (this.state) this.state.sessionTools = event.tools
        break
      case 'extensionCatalog':
        if (this.state) {
          this.state.commands = event.commands
          this.state.toolOrigins = event.toolOrigins
          this.state.extensionDiagnostics = event.diagnostics
          this.state.toolbox = event.toolbox
          this.#reconcileExtensionDiagnostics()
        }
        break
      case 'delta': {
        if (!this.state) break
        const delta = event.delta
        if (delta.type === 'start') {
          this.state.streaming = []
        } else if (delta.type === 'toolOutput') {
          const tool = this.state.live.tools.find((candidate) => candidate.callId === delta.callId)
          if (tool) {
            tool.output = delta.content
            tool.details = delta.details
          }
        } else {
          const blocks = this.state.streaming
          if (!blocks || delta.index > blocks.length) break
          if (delta.type === 'toolCall') {
            const block: ChatBlock = {
              type: 'toolCall',
              id: delta.id,
              name: delta.name,
              arguments: delta.arguments
            }
            if (delta.index === blocks.length) blocks.push(block)
            else blocks[delta.index] = block
          } else if (delta.index === blocks.length) {
            blocks.push(
              delta.type === 'text'
                ? { type: 'text', text: delta.text }
                : { type: 'thinking', text: delta.text, redacted: false }
            )
          } else {
            const block = blocks[delta.index]
            if (block.type === delta.type && (block.type === 'text' || block.type === 'thinking')) {
              block.text += delta.text
            }
          }
        }
        break
      }
      case 'jobOutput': {
        const watch = this.#watch
        if (watch?.jobId !== event.jobId) break
        if (watch.pending) watch.pending.push(event)
        else this.jobOutputs = { [event.jobId]: mergeJobOutput(this.jobOutputs[event.jobId], event) }
        break
      }
      case 'error':
        notify.error('axo', event.message)
        break
      case 'closed':
        if (this.#closing) return
        if (event.missing) {
          if (this.#target.kind === 'tab') {
            setChatTabSessionId(this.#target.tabId, null)
            this.state = null
            this.#reconcileAsks()
            this.editing = null
            this.error = null
            notify.info('Started a new chat', 'The previous axo session no longer exists.')
            void this.attach().catch(() => {})
          } else {
            this.connection = 'closed'
            this.error = event.message || 'The axo session no longer exists.'
          }
        } else {
          this.connection = 'closed'
          this.error = event.message
        }
        break
      case 'reconnecting':
        this.connection = 'reconnecting'
        break
    }
    if (this.#target.kind === 'tab') {
      setChatTabStatus(
        this.#target.tabId,
        this.connection === 'closed'
          ? 'failed'
          : this.state?.live.questions.length
            ? 'waiting'
            : this.busy
              ? 'running'
              : 'idle'
      )
    }
  }

  #reconcileAsks(): void {
    const sessionId = this.state?.sessionId ?? null
    if (this.#askSessionId !== sessionId) {
      this.#askSessionId = sessionId
      this.askDrafts = {}
    }
    const pending = new Set<string>()
    for (const ask of this.state?.live.questions ?? []) {
      const key = askKey(ask.callId)
      if (!validAskBlocks(ask.blocks)) continue
      pending.add(key)
      const draft = this.askDrafts[key]
      if (!draft || JSON.stringify(draft.blocks) !== JSON.stringify(ask.blocks))
        this.askDrafts[key] = createAskDraft(ask.blocks)
    }
    for (const key of Object.keys(this.askDrafts)) {
      if (!pending.has(key)) delete this.askDrafts[key]
    }
  }

  async close(): Promise<void> {
    const folderPath = this.folderPath
    const sessionId = this.sessionId
    if (!folderPath || !sessionId) return
    this.#closing = true
    try {
      await backend.chat.close(folderPath, sessionId)
    } catch (error) {
      this.#closing = false
      throw error
    }
  }

  async send(): Promise<void> {
    return this.#submitDraft((folder, id, parts) =>
      this.state?.live.running
        ? backend.chat.steer(folder, id, parts)
        : backend.chat.prompt(folder, id, crypto.randomUUID(), parts)
    )
  }

  async #submitDraft(operation: (folder: string, id: string, parts: ChatPart[]) => Promise<unknown>): Promise<void> {
    const parts = this.draft
    if (!this.#validateParts(parts)) return
    const generation = this.#generation
    const succeeded = await this.#withSession((folder, id) => operation(folder, id, parts))
    if (succeeded && generation === this.#generation && this.draft === parts) this.draft = []
  }

  #validateParts(parts: ChatPart[]): boolean {
    if (!hasContent(parts)) return false
    if (contentBytes(parts) > CONTENT_LIMIT) {
      notify.error('Attachment too large', CONTENT_LIMIT_MESSAGE)
      return false
    }
    return true
  }

  /** Runs a catalog command; resolves to its error message, or null once it ran or began. */
  runCommand(name: string, args: Record<string, unknown>): Promise<string | null> {
    return this.#attempt((folder, id) => backend.chat.runCommand(folder, id, name, args))
  }

  /** Switches the persona; null selects Default. Resolves to the error message, or null. */
  setAgent(agent: string | null): Promise<string | null> {
    return this.#attempt((folder, id) => backend.chat.setAgent(folder, id, agent))
  }

  /** Enters a mode (null exits) and submits `parts` when non-empty. Resolves to the error message, or null. */
  setMode(mode: string | null, parts: ChatPart[] = []): Promise<string | null> {
    return this.#attempt((folder, id) => backend.chat.setMode(folder, id, mode, parts))
  }

  /** Accepts a reviewed mode result. Resolves to the error message, or null. */
  acceptResult(callId: string): Promise<string | null> {
    return this.#attempt((folder, id) => backend.chat.acceptResult(folder, id, callId))
  }

  /** Sends a reviewed mode result back with feedback. Resolves to the error message, or null. */
  reviseResult(callId: string, feedback: string): Promise<string | null> {
    return this.#attempt((folder, id) => backend.chat.reviseResult(folder, id, callId, feedback))
  }

  /** Saves a reviewed plan under the plans directory and accepts it; resolves to the saved path or the error message. */
  async acceptPlan(callId: string, saveAs: string, then: ChatPlanThen): Promise<{ path: string } | { error: string }> {
    let path = ''
    const error = await this.#attempt(async (folder, id) => {
      path = await backend.chat.acceptPlan(folder, id, callId, saveAs, then)
    })
    return error === null ? { path } : { error }
  }

  /** Like `#withSession`, but requires a live connection and hands the error message to the caller. */
  async #attempt(operation: (folderPath: string, sessionId: string) => Promise<unknown>): Promise<string | null> {
    const folderPath = this.folderPath
    const sessionId = this.sessionId
    if (!folderPath || !sessionId || this.connection !== 'live') return 'The chat is not connected.'
    try {
      await operation(folderPath, sessionId)
      return null
    } catch (error) {
      return getErrorMessage(error)
    }
  }

  async #withSession(operation: (folderPath: string, sessionId: string) => Promise<unknown>): Promise<boolean> {
    const folderPath = this.folderPath
    const sessionId = this.sessionId
    if (!folderPath || !sessionId) return false
    try {
      await operation(folderPath, sessionId)
      return true
    } catch (error) {
      notify.error('axo', getErrorMessage(error))
      return false
    }
  }

  async followUp(): Promise<void> {
    return this.#submitDraft((folder, id, parts) => backend.chat.followUp(folder, id, parts))
  }

  /** Cancels one queued steer or follow-up; returns its parts, or null if already delivered or failed. */
  async withdraw(submissionId: string): Promise<ChatPart[] | null> {
    const folderPath = this.folderPath
    const sessionId = this.sessionId
    if (!folderPath || !sessionId) return null
    try {
      return await backend.chat.withdraw(folderPath, sessionId, submissionId)
    } catch (error) {
      if ((error as { kind?: unknown } | null)?.kind === 'not_found') return null
      notify.error('axo', getErrorMessage(error))
      return null
    }
  }

  /** Withdraws a queued message and prepends it above the current composer draft. */
  async editQueued(item: ChatQueued): Promise<boolean> {
    const generation = this.#generation
    const parts = await this.withdraw(item.submissionId)
    if (!parts || generation !== this.#generation) return false
    this.draft = prependQueuedParts(parts, this.draft)
    return true
  }

  /** Promotes a follow-up to a steer, interrupts to send a steer now, or resumes held items. */
  async sendQueuedNow(item: ChatQueued): Promise<void> {
    if (item.kind === 'followUp') {
      const parts = await this.withdraw(item.submissionId)
      if (parts) await this.#withSession((folder, id) => backend.chat.steer(folder, id, parts))
      return
    }
    const live = this.state?.live
    if (live?.autoPaused && !live.running) return this.resumeWakes()
    await this.#withSession((folder, id) => backend.chat.sendNow(folder, id))
  }

  async stop(): Promise<void> {
    const folderPath = this.folderPath
    const sessionId = this.sessionId
    // Closing a tab must observe abort failures and keep the tab open.
    if (folderPath && sessionId) await backend.chat.abort(folderPath, sessionId)
  }

  async answer(callId: string, answers: ChatAnswer[]): Promise<string | null> {
    return this.#attempt((folder, id) => backend.chat.answer(folder, id, callId, answers))
  }

  async dismiss(callId: string): Promise<void> {
    await this.#withSession((folder, id) => backend.chat.dismiss(folder, id, callId))
  }

  async background(callId: string): Promise<boolean> {
    const folderPath = this.folderPath
    const sessionId = this.sessionId
    if (!folderPath || !sessionId) return false
    try {
      return await backend.chat.background(folderPath, sessionId, callId)
    } catch (error) {
      notify.error('axo', getErrorMessage(error))
      return false
    }
  }

  async killJob(jobId: string): Promise<boolean> {
    const folderPath = this.folderPath
    const sessionId = this.sessionId
    if (!folderPath || !sessionId) return false
    try {
      return await backend.chat.killJob(folderPath, sessionId, jobId)
    } catch (error) {
      notify.error('axo', getErrorMessage(error))
      return false
    }
  }

  async resumeWakes(): Promise<void> {
    await this.#withSession((folder, id) => backend.chat.resumeWakes(folder, id))
  }

  async resume(): Promise<void> {
    await this.#withSession((folder, id) => backend.chat.resume(folder, id))
  }

  watchJob(jobId: string | null): Promise<void> {
    const seq = ++this.#watchSeq
    this.#watchChain = this.#watchChain.then(() => this.#runWatch(seq, jobId))
    return this.#watchChain
  }

  async #runWatch(seq: number, jobId: string | null): Promise<void> {
    // Superseded before it was sent: the newer call replaces the server-side watcher itself.
    if (seq !== this.#watchSeq) return
    const attachId = this.#attachId
    const folderPath = this.folderPath
    const sessionId = this.sessionId
    if (!attachId || !folderPath || !sessionId) return
    const watch = jobId ? { jobId, pending: [] as JobOutputEvent[] | null } : null
    this.#watch = watch
    const kept = jobId ? this.jobOutputs[jobId] : undefined
    this.jobOutputs = jobId && kept ? { [jobId]: kept } : {}
    try {
      const out = await backend.chat.watchJob(folderPath, sessionId, attachId, jobId)
      if (this.#attachId !== attachId || this.#watch !== watch || !watch) return
      let output = out ?? undefined
      for (const event of watch.pending ?? []) output = mergeJobOutput(output, event)
      watch.pending = null
      if (output) this.jobOutputs = { [watch.jobId]: output }
    } catch (error) {
      if (watch) watch.pending = null
      if (this.#attachId !== attachId) return
      notify.error('axo', getErrorMessage(error))
    }
  }

  startEdit(entry: ChatEntry): void {
    if (entry.body.kind !== 'user') return
    this.editing = {
      entryId: entry.id,
      anchor: entry.body.anchor,
      parts: entry.body.parts.map((part) => ({ ...part }))
    }
  }

  cancelEdit(): void {
    this.editing = null
  }

  async submitEdit(): Promise<void> {
    const editing = this.editing
    if (!editing || !this.#validateParts(editing.parts)) return
    const succeeded = await this.#withSession(async (folder, id) => {
      await backend.chat.navigate(folder, id, editing.anchor)
      await backend.chat.prompt(folder, id, crypto.randomUUID(), editing.parts)
    })
    if (succeeded && this.editing === editing) this.editing = null
  }

  async selectBranch(entryId: string): Promise<void> {
    await this.#withSession((folder, id) => backend.chat.selectBranch(folder, id, entryId))
  }

  async fork(entryId: string): Promise<void> {
    await this.#withSession(async (folderPath, sessionId) => {
      const fork = await backend.chat.fork(folderPath, sessionId, entryId)
      const tabId = addChatTab(folderPath, { sessionId: fork.sessionId, title: this.state?.live.title ?? 'New Chat' })
      getOrCreate(tabId).draft = fork.editable ?? []
      requestFocusTab(tabId)
    })
  }

  release(): void {
    this.#generation++
    this.#handle?.dispose()
    this.#handle = null
    this.#attachId = null
    this.jobOutputs = {}
    this.#watch = null
    this.#watchSeq++
    this.connection = 'detached'
  }
}
