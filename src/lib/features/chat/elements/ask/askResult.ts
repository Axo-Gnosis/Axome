import type { ChatAnswer, ChatAskBlock } from '$lib/types/backend'
import { partsText } from '../../parts/parts'
import type { ToolStep } from '../../turns'
import { record } from '../tool/payload'
import { answerError, askAnswerText, askKey, parseAskBlocks } from './askDocument'

export type AskOutcome = 'answered' | 'timedOut' | 'dismissed' | 'cancelled' | 'steered' | 'failed' | 'unavailable'

export interface AskAnswerRow {
  /** False when the recorded answer is `null` (skipped, or no suggestion at timeout). */
  answered: boolean
  text: string
  note: string | null
  suggested: boolean
}

export interface AskResult {
  outcome: AskOutcome
  blocks: ChatAskBlock[] | null
  rows: Record<string, AskAnswerRow>
  message: string | null
}

/** Every question reads "Dismissed"; the run went on without answers. */
function dismissed(blocks: ChatAskBlock[] | null): AskResult {
  const rows: AskResult['rows'] = {}
  for (const block of blocks ?? [])
    if (block.type !== 'markdown')
      rows[askKey(block.id)] = { answered: false, text: 'Dismissed', note: null, suggested: false }
  return { outcome: 'dismissed', blocks, rows, message: null }
}

export function parseAskResult(call: ToolStep): AskResult {
  const blocks = parseAskBlocks(call.arguments)
  const rows: AskResult['rows'] = {}
  if (call.status === 'error') {
    const message = partsText(call.output.filter((part) => part.type === 'text'))
    // Journals written before axo let a dismissal continue the run.
    if (message === 'Ask tool was cancelled by the user') return dismissed(blocks)
    if (message === 'Ask input was cancelled') return { outcome: 'cancelled', blocks, rows, message: null }
    return { outcome: 'failed', blocks, rows, message: message || null }
  }
  const details = record(call.details)
  if (call.status === 'done' && details.outcome === 'steered')
    return { outcome: 'steered', blocks, rows, message: null }
  if (call.status === 'done' && details.outcome === 'dismissed') return dismissed(blocks)
  const unavailable: AskResult = { outcome: 'unavailable', blocks, rows: {}, message: 'Question details unavailable.' }
  if (!blocks || call.status !== 'done' || (details.timedOut !== undefined && typeof details.timedOut !== 'boolean'))
    return unavailable

  const answers = record(details.answers)
  const questions = blocks.filter((block) => block.type !== 'markdown')
  if (Object.keys(answers).length !== questions.length) return unavailable
  const timedOut = details.timedOut === true
  for (const question of questions) {
    const { id } = question
    if (!Object.hasOwn(answers, id)) return unavailable
    const raw = answers[id]
    if (raw === null) {
      if (!timedOut && !question.optional) return unavailable
      rows[askKey(id)] = { answered: false, text: 'no answer', note: null, suggested: false }
      continue
    }
    const { value, other, note } = record(raw)
    if (
      value !== undefined &&
      typeof value !== 'string' &&
      typeof value !== 'number' &&
      !(Array.isArray(value) && value.every((item) => typeof item === 'string'))
    )
      return unavailable
    if ((other !== undefined && typeof other !== 'string') || (note !== undefined && typeof note !== 'string'))
      return unavailable
    const answer: ChatAnswer = { id, value, other, note }
    if (answerError(question, answer, true)) return unavailable
    rows[askKey(id)] = {
      answered: true,
      text: askAnswerText(question, answer),
      note: note && /[^\p{White_Space}]/u.test(note) ? note : null,
      suggested: timedOut
    }
  }
  return { outcome: timedOut ? 'timedOut' : 'answered', blocks, rows, message: null }
}
