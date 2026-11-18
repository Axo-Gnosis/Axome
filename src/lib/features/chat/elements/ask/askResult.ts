import type { ToolStep } from '../../turns'
import { record } from '../tool/payload'

export type AskOutcome = { text: string; tone: 'answered' | 'muted' | 'danger' }

/** One asked question joined with what the user chose; nothing chosen for steered, dismissed or failed calls. */
export type AskAnswer = {
  id: string
  question: string
  options: { label: string; description: string | null }[]
  selected: string[]
  custom: string | null
  timedOut: boolean
}

// axo's tool errors for a dismissed ask (also an empty single-select answer) and a closed or aborted one.
const DISMISSED = 'Ask tool was cancelled by the user'
const CANCELLED = 'Ask input was cancelled'

const strings = (value: unknown): string[] =>
  Array.isArray(value) ? value.filter((item): item is string => typeof item === 'string') : []

/** `details` of a settled ask: one result for a single question, `{ results }` for several. */
function results(details: unknown): Record<string, unknown>[] {
  const value = record(details)
  if (Array.isArray(value.results)) return value.results.map(record)
  return typeof value.id === 'string' ? [value] : []
}

/** Picks and custom text of one result, as shown in a summary line. */
export function answerText(selected: string[], custom: string | null): string {
  return [selected.join(', '), custom === null ? '' : `"${custom}"`].filter(Boolean).join(', ') || 'none selected'
}

export function questionLabel(q: { header?: string | null }, index: number): string {
  return q.header ?? `Q${index + 1}`
}

export function askOutcome(call: ToolStep): AskOutcome {
  if (call.status === 'error') {
    const text = call.output.map((part) => (part.type === 'text' ? part.text : '')).join('\n')
    if (text.includes(DISMISSED)) return { text: 'dismissed', tone: 'muted' }
    if (text.includes(CANCELLED)) return { text: 'cancelled', tone: 'muted' }
    return { text: 'Failed', tone: 'danger' }
  }
  if (record(call.details).outcome === 'steered') return { text: 'answered by your message', tone: 'muted' }
  const settled = results(call.details)
  if (settled.length > 1) return { text: `${settled.length} answers`, tone: 'answered' }
  const [result] = settled
  if (!result) return { text: '', tone: 'muted' }
  const custom = typeof result.customInput === 'string' ? result.customInput : null
  const text = answerText(strings(result.selectedOptions), custom)
  return { text: result.timedOut === true ? `${text} · auto-selected` : text, tone: 'answered' }
}

export function askAnswers(call: ToolStep): AskAnswer[] {
  const asked = record(call.arguments).questions
  const settled = new Map(results(call.details).map((result) => [result.id, result]))
  return (Array.isArray(asked) ? asked : []).map(record).flatMap((question): AskAnswer[] => {
    if (typeof question.id !== 'string' || typeof question.question !== 'string') return []
    const result = settled.get(question.id)
    const options = (Array.isArray(question.options) ? question.options : [])
      .map(record)
      .flatMap((option) =>
        typeof option.label === 'string'
          ? [{ label: option.label, description: typeof option.description === 'string' ? option.description : null }]
          : []
      )
    return [
      {
        id: question.id,
        question: question.question,
        options,
        selected: strings(result?.selectedOptions),
        custom: typeof result?.customInput === 'string' ? result.customInput : null,
        timedOut: result?.timedOut === true
      }
    ]
  })
}
