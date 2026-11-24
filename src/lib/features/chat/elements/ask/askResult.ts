import type { ChatAnswer, ChatAskBlock } from '$lib/types/backend'
import { partsText } from '../../parts/parts'
import type { ToolStep } from '../../turns'
import { record } from '../tool/payload'
import { askKey, parseAskBlocks, validateAskAnswers } from './askDocument'

export type AskOutcome = 'answered' | 'timedOut' | 'dismissed' | 'cancelled' | 'steered' | 'failed' | 'unavailable'

export interface AskAnswerRow {
  id: string
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

export function parseAskResult(call: ToolStep): AskResult {
  const blocks = parseAskBlocks(call.arguments)
  const rows: AskResult['rows'] = {}
  if (call.status === 'error') {
    const message = partsText(call.output.filter((part) => part.type === 'text'))
    if (message === 'Ask tool was cancelled by the user') return { outcome: 'dismissed', blocks, rows, message: null }
    if (message === 'Ask input was cancelled') return { outcome: 'cancelled', blocks, rows, message: null }
    return { outcome: 'failed', blocks, rows, message: message || null }
  }
  const details = record(call.details)
  if (call.status === 'done' && details.outcome === 'steered')
    return { outcome: 'steered', blocks, rows, message: null }
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
      rows[askKey(id)] = { id, answered: false, text: 'no answer', note: null, suggested: false }
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
    const validation = validateAskAnswers([{ ...question, optional: false }], [answer])
    if (validation.error || validation.errors[askKey(id)]) return unavailable

    let text: string
    if (question.type === 'choice') {
      const picks = Array.isArray(value) ? value : typeof value === 'string' ? [value] : []
      const labels = picks.flatMap((pick) =>
        question.options.filter((option) => option.value === pick).map((option) => option.label)
      )
      if (other && /[^\p{White_Space}]/u.test(other)) labels.push(`"${other}"`)
      text = labels.join(', ')
    } else {
      text = question.type === 'number' ? String(value) : `"${value}"`
    }
    rows[askKey(id)] = {
      id,
      answered: true,
      text,
      note: note && /[^\p{White_Space}]/u.test(note) ? note : null,
      suggested: timedOut
    }
  }
  return { outcome: timedOut ? 'timedOut' : 'answered', blocks, rows, message: null }
}
