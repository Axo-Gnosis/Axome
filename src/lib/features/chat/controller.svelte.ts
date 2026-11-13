import { backend } from '$lib/api/backend'
import type { StreamHandle } from '$lib/api/transport'
import { notify } from '$lib/features/notifications/state.svelte'
import type { ChatTab, TabId } from '$lib/features/workbench/model'
import {
  addChatTab,
  getTabs,
  requestFocusTab,
  setChatTabSessionId,
  setChatTabStatus,
  setChatTabTitle
} from '$lib/features/workbench/state.svelte'
import type { ChatAnswer, ChatBlock, ChatEntry, ChatEvent, ChatPart, ChatState } from '$lib/types/backend'
import { getErrorMessage } from '$lib/utils/client-error'
import { getOrCreate } from './registry'
import { getChatModels, loadChatModels, MODELS_HINT } from './models.svelte'
import { CONTENT_LIMIT, CONTENT_LIMIT_MESSAGE, contentBytes, hasContent } from './parts/parts'

export class ChatController {
  readonly tabId: TabId
  state = $state<ChatState | null>(null)
  connection = $state<'detached' | 'attaching' | 'live' | 'reconnecting' | 'closed'>('detached')
  error = $state<string | null>(null)
  draft = $state<ChatPart[]>([])
  editing = $state<{ entryId: string; anchor: string | null; parts: ChatPart[] } | null>(null)
  #handle: StreamHandle<void> | null = null
  #generation = 0
  #closing = false

  constructor(tabId: TabId) {
    this.tabId = tabId
  }

  get #tab(): ChatTab | undefined {
    const tab = getTabs().find((candidate) => candidate.id === this.tabId)
    return tab?.kind === 'chat' ? tab : undefined
  }

  get busy(): boolean {
    return !!this.state && (this.state.live.running || this.state.live.maintenance !== null)
  }

  async attach(): Promise<void> {
    this.release()
    const tab = this.#tab
    if (!tab) return
    this.error = null
    // A reconnect keeps the transcript it shows until the new snapshot replaces it.
    if (this.state?.sessionId !== tab.sessionId) this.state = null
    this.connection = 'attaching'
    const generation = this.#generation
    try {
      let sessionId = tab.sessionId
      if (sessionId) void loadChatModels(tab.folderPath)
      else {
        await loadChatModels(tab.folderPath)
        if (generation !== this.#generation) return
        const { catalog, error } = getChatModels(tab.folderPath)
        const model = catalog?.defaultModel ?? catalog?.models[0]?.selector
        if (!catalog || !model) throw new Error(error ?? `No axo models are available. ${MODELS_HINT}`)
        sessionId = await backend.chat.create(tab.folderPath, model, catalog.defaultThinking)
        // Kept even when superseded: the next attach reuses it, and the backend reclaims an unattached one.
        setChatTabSessionId(this.tabId, sessionId)
        if (generation !== this.#generation) return
      }
      const handle = backend.chat.attach(
        { attachId: crypto.randomUUID(), folderPath: tab.folderPath, sessionId },
        {
          onEvent: (event) => {
            if (generation === this.#generation) this.apply(event)
          }
        }
      )
      this.#handle = handle
      await handle.ready
    } catch (error) {
      if (generation !== this.#generation) return
      this.connection = 'closed'
      this.error = getErrorMessage(error)
      setChatTabStatus(this.tabId, 'failed')
      throw error
    }
  }

  apply(event: ChatEvent): void {
    switch (event.type) {
      case 'snapshot':
        this.state = event.state
        this.connection = 'live'
        this.error = null
        setChatTabTitle(this.tabId, event.state.live.title ?? 'New Chat')
        break
      case 'entry':
        if (this.state) {
          this.state.entries.push(event.entry)
          if (event.entry.body.kind === 'assistant') this.state.streaming = null
        }
        break
      case 'live':
        if (this.state) this.state.live = event.live
        setChatTabTitle(this.tabId, event.live.title ?? 'New Chat')
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
      case 'notice':
        notify.error('axo', event.message)
        break
      case 'closed':
        if (this.#closing) return
        if (event.missing) {
          setChatTabSessionId(this.tabId, null)
          this.state = null
          this.editing = null
          this.error = null
          notify.info('Started a new chat', 'The previous axo session no longer exists.')
          void this.attach().catch(() => {})
        } else {
          this.connection = 'closed'
          this.error = event.message
        }
        break
      case 'reconnecting':
        this.connection = 'reconnecting'
        break
    }
    setChatTabStatus(
      this.tabId,
      this.connection === 'closed'
        ? 'failed'
        : this.state?.live.questions.length
          ? 'waiting'
          : this.busy
            ? 'running'
            : 'idle'
    )
  }

  async close(): Promise<void> {
    const tab = this.#tab
    if (!tab?.sessionId) return
    this.#closing = true
    try {
      await backend.chat.close(tab.folderPath, tab.sessionId)
    } catch (error) {
      this.#closing = false
      throw error
    }
  }

  async send(): Promise<void> {
    const parts = this.draft
    if (!this.#validateParts(parts)) return
    const generation = this.#generation
    const succeeded = await this.#withSession((folder, id) =>
      this.state?.live.running
        ? backend.chat.steer(folder, id, parts)
        : backend.chat.prompt(folder, id, crypto.randomUUID(), parts)
    )
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
  async runCommand(name: string, args: Record<string, unknown>): Promise<string | null> {
    const tab = this.#tab
    if (!tab?.sessionId || this.connection !== 'live') return 'The chat is not connected.'
    try {
      await backend.chat.runCommand(tab.folderPath, tab.sessionId, name, args)
      return null
    } catch (error) {
      return getErrorMessage(error)
    }
  }

  async #withSession(operation: (folderPath: string, sessionId: string) => Promise<unknown>): Promise<boolean> {
    const tab = this.#tab
    if (!tab?.sessionId) return false
    try {
      await operation(tab.folderPath, tab.sessionId)
      return true
    } catch (error) {
      notify.error('axo', getErrorMessage(error))
      return false
    }
  }

  async followUp(): Promise<void> {
    const parts = this.draft
    if (!this.#validateParts(parts)) return
    const generation = this.#generation
    if (await this.#withSession((folder, id) => backend.chat.followUp(folder, id, parts))) {
      if (generation === this.#generation && this.draft === parts) this.draft = []
    }
  }

  async stop(): Promise<void> {
    const tab = this.#tab
    // Closing a tab must observe abort failures and keep the tab open.
    if (tab?.sessionId) await backend.chat.abort(tab.folderPath, tab.sessionId)
  }

  async answer(callId: string, answers: ChatAnswer[]): Promise<void> {
    await this.#withSession((folder, id) => backend.chat.answer(folder, id, callId, answers))
  }

  async dismiss(callId: string): Promise<void> {
    await this.#withSession((folder, id) => backend.chat.dismiss(folder, id, callId))
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
      const tab = getTabs().find((candidate) => candidate.id === tabId)
      if (tab?.kind === 'chat') getOrCreate(tab).draft = fork.editable ?? []
      requestFocusTab(tabId)
    })
  }

  release(): void {
    this.#generation++
    this.#handle?.dispose()
    this.#handle = null
    this.connection = 'detached'
  }
}
