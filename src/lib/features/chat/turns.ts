import type { ChatBlock, ChatContent, ChatEntry, ChatEntryBody, ChatLive, ChatThinking } from '$lib/types/backend'
import { record } from './elements/tool/payload'

export type UserEntry = ChatEntry & { body: Extract<ChatEntryBody, { kind: 'user' }> }
export type AssistantEntry = ChatEntry & { body: Extract<ChatEntryBody, { kind: 'assistant' }> }
export type ToolResult = Extract<ChatEntryBody, { kind: 'toolResult' }>
/** `timestamp` is the entry's (null while streaming). */
type StepBase = { id: string; entryId: string | null; timestamp: string | null }
export type ToolStep = StepBase & {
  kind: 'tool'
  callId: string
  name: string
  arguments: unknown
  status: 'running' | 'done' | 'error'
  result: ToolResult | null
  /** The tool-result entry's timestamp. */
  endedAt: string | null
  output: ChatContent[]
  details: unknown
}
export type Step =
  | ToolStep
  | (StepBase &
      (
        | { kind: 'text'; text: string }
        | { kind: 'thinking'; text: string; redacted: boolean }
        | Extract<
            ChatEntryBody,
            {
              kind:
                | 'notice'
                | 'compaction'
                | 'branchSummary'
                | 'modelChange'
                | 'thinkingChange'
                | 'contextMoved'
                | 'agentChange'
                | 'modeChange'
            }
          >
      ))
/** Recorded settings for this turn; null when absent from the transcript. */
export type TurnConfig = { model: string | null; thinking: ChatThinking | null }
export type Turn = {
  id: string
  user: UserEntry | null
  config: TurnConfig
  steps: Step[]
  assistants: AssistantEntry[]
  end: Extract<ChatEntryBody, { kind: 'runEnd' }> | null
  running: boolean
  startedAt: string | null
  endedAt: string | null
}

export function transcriptTurns(entries: ChatEntry[], streaming: ChatBlock[] | null, live: ChatLive): Turn[] {
  const turns: Turn[] = []
  const results = new Map<string, ChatEntry & { body: ToolResult }>()
  for (const entry of entries) {
    if (entry.body.kind === 'toolResult') results.set(entry.body.callId, entry as ChatEntry & { body: ToolResult })
  }
  const runs = new Map(live.tools.map((run) => [run.callId, run]))
  let current: Turn | undefined
  const settings: TurnConfig = { model: null, thinking: null }
  function start(user: UserEntry | null, timestamp: string | null = user?.timestamp ?? null): Turn {
    const turn: Turn = {
      id: user?.id ?? `turn:${turns.length}`,
      user,
      config: { ...settings },
      steps: [],
      assistants: [],
      end: null,
      running: false,
      startedAt: timestamp,
      endedAt: null
    }
    turns.push(turn)
    return turn
  }
  function blocks(turn: Turn, content: ChatBlock[], entryId: string | null, timestamp: string | null) {
    content.forEach((block, index) => {
      const base = { id: `${entryId ?? 'streaming'}:${index}`, entryId, timestamp }
      if (block.type === 'toolCall') {
        const resultEntry = results.get(block.id)
        const result = resultEntry?.body ?? null
        const run = runs.get(block.id)
        let name = block.name
        let arguments_ = block.arguments
        if (name === 'call') {
          const args = record(block.arguments)
          name =
            typeof args.tool === 'string' && args.tool.length > 0
              ? args.tool
              : run && run.name !== 'call'
                ? run.name
                : result && result.toolName !== 'call'
                  ? result.toolName
                  : 'call'
          arguments_ =
            name !== 'call'
              ? 'args' in args && args.args !== undefined
                ? args.args
                : (run?.arguments ?? {})
              : block.arguments
        }
        turn.steps.push({
          ...base,
          kind: 'tool',
          callId: block.id,
          name,
          arguments: arguments_,
          status: result ? (result.isError ? 'error' : 'done') : 'running',
          result,
          endedAt: resultEntry?.timestamp ?? null,
          details: result?.details ?? run?.details,
          output: result?.content ?? run?.output ?? []
        })
      } else if (block.type === 'text') {
        turn.steps.push({ ...base, kind: 'text', text: block.text })
      } else {
        turn.steps.push({ ...base, kind: 'thinking', text: block.text, redacted: block.redacted })
      }
    })
  }
  for (const entry of entries) {
    const body = entry.body
    if (body.kind === 'user') {
      current = start(entry as UserEntry)
      continue
    }
    if (body.kind === 'modelChange') settings.model = body.model
    if (body.kind === 'thinkingChange') settings.thinking = body.thinking
    // Selections before any content are the session's starting config, not command results.
    if (!current && (body.kind === 'modelChange' || body.kind === 'thinkingChange')) continue
    if (current && current.end !== null && (body.kind === 'assistant' || body.kind === 'notice')) {
      current = undefined
    }
    if (body.kind === 'toolResult') {
      if (current && current.end === null) current.endedAt = entry.timestamp
      continue
    }
    if (
      body.kind === 'notice' &&
      body.notice.kind === 'mail' &&
      current &&
      (current.user !== null || current.steps.length > 0)
    ) {
      current = start(null, entry.timestamp)
    }
    current ??= start(null, entry.timestamp)
    // Only the run's own entries time a reply; picker changes and maintenance recorded later don't.
    if (body.kind === 'assistant' || body.kind === 'notice' || body.kind === 'runEnd') current.endedAt = entry.timestamp
    if (body.kind === 'assistant') {
      current.config = { model: body.model, thinking: settings.thinking }
      current.assistants.push(entry as AssistantEntry)
      blocks(current, body.blocks, entry.id, entry.timestamp)
    } else if (body.kind === 'runEnd') current.end = body
    else current.steps.push({ ...body, id: entry.id, entryId: entry.id, timestamp: entry.timestamp })
  }
  if (streaming !== null) {
    if (current && current.end !== null) current = undefined
    current ??= start(null)
    blocks(current, streaming, null, null)
  }
  // Activity `running` also covers standalone maintenance; an ended turn never resumes.
  if (current) current.running = live.running && current.end === null
  return turns
}

export type TurnEndNotice = { kind: 'failed'; text: string } | { kind: 'stopped' } | { kind: 'note'; text: string }

/** The one line that ends a turn; the first matching outcome wins. */
export function turnEndNotice(turn: Turn): TurnEndNotice | null {
  const end = turn.end
  const stops = turn.assistants.map((entry) => entry.body.stopReason)
  if (end?.outcome === 'failed' || stops.includes('error')) {
    const error = turn.assistants.findLast((entry) => entry.body.error)?.body.error
    return { kind: 'failed', text: end?.reason || error || 'The turn failed.' }
  }
  if (end?.outcome === 'aborted' || stops.includes('aborted')) return { kind: 'stopped' }
  if (end?.outcome === 'interrupted') return { kind: 'note', text: end.reason || 'The turn was interrupted.' }
  if (stops.includes('length')) return { kind: 'note', text: 'Stopped at the output token limit.' }
  return null
}

export function turnDurationMs(turn: Turn, now: number): number | null {
  if (turn.startedAt === null || (!turn.running && turn.endedAt === null)) return null
  const start = Date.parse(turn.startedAt)
  const end = turn.running ? now : Date.parse(turn.endedAt!)
  return Number.isFinite(start) && Number.isFinite(end) ? Math.max(0, end - start) : null
}

function assistantPromptTokens(entry: AssistantEntry): number | null {
  const usage = entry.body.usage
  const prompt = usage.input + usage.cacheRead + usage.cacheWrite
  const tokens = prompt > 0 ? prompt : usage.totalTokens
  return tokens > 0 ? tokens : null
}

/**
 * Estimates post-compaction context tokens for a compaction checkpoint from the first
 * subsequent assistant request or, for the latest checkpoint, `liveContextTokens`.
 */
export function compactionTokensAfter(
  turns: Turn[],
  turnIndex: number,
  stepId: string,
  liveContextTokens: number | null | undefined
): number | null {
  const turn = turns[turnIndex]
  if (!turn) return null
  const stepIndex = turn.steps.findIndex((step) => step.id === stepId)
  if (stepIndex === -1) return null

  // Same-turn assistant entry after compaction.
  const assistantById = new Map(turn.assistants.map((assistant) => [assistant.id, assistant]))
  for (let i = stepIndex + 1; i < turn.steps.length; i++) {
    const step = turn.steps[i]
    if (step.kind === 'compaction') return null
    if (step.entryId) {
      const assistant = assistantById.get(step.entryId)
      if (assistant) {
        const tokens = assistantPromptTokens(assistant)
        if (tokens !== null) return tokens
      }
    }
  }

  // First assistant response in any subsequent turn before another compaction.
  for (let t = turnIndex + 1; t < turns.length; t++) {
    const later = turns[t]
    if (later.steps.some((step) => step.kind === 'compaction')) {
      return later.assistants[0] ? assistantPromptTokens(later.assistants[0]) : null
    }
    if (later.assistants[0]) {
      const tokens = assistantPromptTokens(later.assistants[0])
      if (tokens !== null) return tokens
    }
  }

  return liveContextTokens && liveContextTokens > 0 ? liveContextTokens : null
}
