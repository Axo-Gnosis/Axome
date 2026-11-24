import type { ChatAnswer, ChatAskBlock, ChatAskOption } from '$lib/types/backend'
import { record } from '../tool/payload'

export type AskQuestion = Exclude<ChatAskBlock, { type: 'markdown' }>

export interface AskQuestionDraft {
  selected: string[]
  text: string
  otherOpen: boolean
  other: string
  noteOpen: boolean
  note: string
  skipped: boolean
  /** Settled by the user; renders as its answer row until reopened. */
  collapsed: boolean
}

/** Markdown blocks, and runs of back-to-back questions that render as one card. */
export type AskSection = { type: 'markdown'; text: string } | { type: 'questions'; questions: AskQuestion[] }

export interface AskDraft {
  blocks: ChatAskBlock[]
  questions: Record<string, AskQuestionDraft>
  busy: boolean
  errors: Record<string, string>
  error: string | null
}

export interface AskValidation {
  answers: ChatAnswer[]
  errors: Record<string, string>
  ready: number
  total: number
}

const blank = (text: string) => /^[\p{White_Space}]*$/u.test(text)
const finite = (value: unknown): value is number => typeof value === 'number' && Number.isFinite(value)
const optionalBool = (value: unknown): value is boolean | undefined => value === undefined || typeof value === 'boolean'
const nullableString = (value: unknown): value is string | null | undefined =>
  value == null || typeof value === 'string'
const nullableNumber = (value: unknown): value is number | null | undefined => value == null || finite(value)
const only = (object: Record<string, unknown>, keys: string[]) => Object.keys(object).every((key) => keys.includes(key))

export function askKey(id: string): string {
  return '$' + id
}

export function parseAskBlocks(arguments_: unknown): ChatAskBlock[] | null {
  const { blocks: input } = record(arguments_)
  if (!Array.isArray(input)) return null
  const blocks: ChatAskBlock[] = []
  for (const raw of input) {
    const block = record(raw)
    if (block.type === 'markdown') {
      if (typeof block.text !== 'string' || !only(block, ['type', 'text'])) return null
      blocks.push({ type: 'markdown', text: block.text })
      continue
    }
    const { type, id, question, optional } = block
    if (typeof id !== 'string' || typeof question !== 'string' || !optionalBool(optional)) return null
    const base = { id, question, optional: optional ?? false }
    const keys = ['type', 'id', 'question', 'optional']
    switch (type) {
      case 'choice': {
        const { multi, options: rawOptions } = block
        if (!Array.isArray(rawOptions) || !optionalBool(multi) || !only(block, [...keys, 'multi', 'options']))
          return null
        const options: ChatAskOption[] = []
        for (const rawOption of rawOptions) {
          const option = record(rawOption)
          const { value, label, description, recommended } = option
          if (
            typeof value !== 'string' ||
            typeof label !== 'string' ||
            !nullableString(description) ||
            !optionalBool(recommended) ||
            !only(option, ['value', 'label', 'description', 'recommended'])
          )
            return null
          options.push({ value, label, description: description ?? null, recommended: recommended ?? false })
        }
        blocks.push({ type, ...base, options, multi: multi ?? false })
        break
      }
      case 'text': {
        const { multiline, placeholder, prefill } = block
        if (
          !optionalBool(multiline) ||
          !nullableString(placeholder) ||
          !nullableString(prefill) ||
          !only(block, [...keys, 'multiline', 'placeholder', 'prefill'])
        )
          return null
        blocks.push({
          type,
          ...base,
          multiline: multiline ?? false,
          placeholder: placeholder ?? null,
          prefill: prefill ?? null
        })
        break
      }
      case 'number': {
        const { integer, min, max, default: suggested } = block
        if (
          !optionalBool(integer) ||
          !nullableNumber(min) ||
          !nullableNumber(max) ||
          !nullableNumber(suggested) ||
          !only(block, [...keys, 'integer', 'min', 'max', 'default'])
        )
          return null
        blocks.push({
          type,
          ...base,
          integer: integer ?? false,
          min: min ?? null,
          max: max ?? null,
          default: suggested ?? null
        })
        break
      }
      default:
        return null
    }
  }
  return validAskBlocks(blocks) ? blocks : null
}

export function validAskBlocks(blocks: ChatAskBlock[]): boolean {
  const ids = new Set<string>()
  for (const block of blocks) {
    if (block.type === 'markdown') continue
    if (blank(block.id) || ids.has(block.id)) return false
    ids.add(block.id)
    if (block.type === 'choice') {
      const values = block.options.map((option) => option.value)
      if (values.length < 2 || new Set(values).size !== values.length || values.some(blank)) return false
      if (!block.multi && block.options.filter((option) => option.recommended).length > 1) return false
    } else if (block.type === 'number') {
      const { lower, upper } = askNumberBounds(block)
      if (lower > upper) return false
      if (block.default !== null && numberError(block, block.default)) return false
    }
  }
  return ids.size > 0
}

/** The accepted range; integer questions round their bounds inward. Missing bounds are infinite. */
export function askNumberBounds({ min, max, integer }: Extract<AskQuestion, { type: 'number' }>) {
  return {
    lower: min === null ? -Infinity : integer ? Math.ceil(min) : min,
    upper: max === null ? Infinity : integer ? Math.floor(max) : max
  }
}

export function createAskDraft(blocks: ChatAskBlock[]): AskDraft {
  const questions: AskDraft['questions'] = {}
  for (const block of blocks) {
    if (block.type === 'markdown') continue
    questions[askKey(block.id)] = {
      selected:
        block.type === 'choice'
          ? block.options.filter((option) => option.recommended).map((option) => option.value)
          : [],
      text:
        block.type === 'text'
          ? (block.prefill ?? '')
          : block.type === 'number' && block.default !== null
            ? String(block.default)
            : '',
      otherOpen: false,
      other: '',
      noteOpen: false,
      note: '',
      skipped: false,
      collapsed: false
    }
  }
  return { blocks, questions, busy: false, errors: {}, error: null }
}

export function askSections(blocks: ChatAskBlock[]): AskSection[] {
  const sections: AskSection[] = []
  for (const block of blocks) {
    const last = sections.at(-1)
    if (block.type === 'markdown') sections.push({ type: 'markdown', text: block.text })
    else if (last?.type === 'questions') last.questions.push(block)
    else sections.push({ type: 'questions', questions: [block] })
  }
  return sections
}

/** Display text of a validated answer that carries a value or `other`. */
export function askAnswerText(question: AskQuestion, { value, other }: ChatAnswer): string {
  if (question.type !== 'choice') return question.type === 'number' ? String(value) : `"${value}"`
  const picks = Array.isArray(value) ? value : typeof value === 'string' ? [value] : []
  const labels = picks.flatMap((pick) =>
    question.options.filter((option) => option.value === pick).map((option) => option.label)
  )
  if (other && !blank(other)) labels.push(`"${other}"`)
  return labels.join(', ')
}

function numberError(question: Extract<AskQuestion, { type: 'number' }>, value: number): string | null {
  if (!finite(value)) return 'Enter a number.'
  const { min, max, integer } = question
  if ((min !== null && value < min) || (max !== null && value > max)) {
    if (min !== null && max !== null) return `Must be between ${min} and ${max}.`
    return min !== null ? `Must be at least ${min}.` : `Must be at most ${max}.`
  }
  return integer && !Number.isInteger(value) ? 'Enter a whole number.' : null
}

// Keep resolve_one's ordering: normalize empty fields, check omission, then check their types.
export function answerError(
  question: AskQuestion,
  answer?: ChatAnswer,
  requireAnswer = !question.optional
): string | null {
  let value = answer?.value
  if ((typeof value === 'string' && blank(value)) || (Array.isArray(value) && !value.length)) value = undefined
  const other = answer?.other !== undefined && !blank(answer.other)
  const note = answer?.note !== undefined && !blank(answer.note)
  if (value === undefined && !other) {
    if (note) return 'A note needs an answer.'
    if (!requireAnswer) return null
    return question.type === 'choice'
      ? 'Pick an option or type your own answer.'
      : question.type === 'text'
        ? 'Enter an answer.'
        : 'Enter a number.'
  }
  switch (question.type) {
    case 'choice': {
      if (!question.multi && value !== undefined && other) return 'answer an option or `other`, not both'
      if (value === undefined) return null
      if (!question.multi && typeof value !== 'string') return 'expects one option value'
      if (question.multi && !Array.isArray(value)) return 'expects a list of option values'
      const seen = new Set<string>()
      for (const pick of Array.isArray(value) ? value : [String(value)]) {
        if (!question.options.some((option) => option.value === pick)) return `\`${pick}\` is not an option`
        if (seen.has(pick)) return `\`${pick}\` is picked twice`
        seen.add(pick)
      }
      return null
    }
    case 'text':
      if (other || note) return 'a text answer takes no `other` or `note`'
      return typeof value === 'string' ? null : 'expects text'
    case 'number':
      if (other) return 'a number answer takes no `other`'
      return typeof value === 'number' ? numberError(question, value) : 'expects a number'
  }
}

export function askNumberValue(text: string): number | null {
  const literal = text.trim()
  if (!/^[+-]?(?:\d+(?:\.\d*)?|\.\d+)(?:[eE][+-]?\d+)?$/.test(literal)) return null
  const value = Number(literal)
  return finite(value) ? value : null
}

export function buildQuestionAnswer(
  question: AskQuestion,
  draft?: AskQuestionDraft
): { answer: ChatAnswer; error: string | null; hasAnswer: boolean } {
  const answer: ChatAnswer = { id: question.id }
  let unfinished: string | null = null
  if (draft && !draft.skipped) {
    if (question.type === 'choice') {
      if (draft.selected.length) answer.value = question.multi ? [...draft.selected] : draft.selected[0]
      if (draft.otherOpen) {
        if (blank(draft.other)) unfinished = 'Type your answer, or pick one of the options.'
        else answer.other = draft.other
      }
    } else if (!blank(draft.text)) {
      if (question.type === 'text') answer.value = draft.text
      else {
        const value = askNumberValue(draft.text)
        if (value === null) unfinished = 'Enter a number.'
        else answer.value = value
      }
    }
    if (draft.noteOpen && !blank(draft.note)) answer.note = draft.note
  }
  const error = unfinished ?? answerError(question, answer)
  const hasAnswer = answer.value !== undefined || answer.other !== undefined
  return { answer, error, hasAnswer }
}

export function buildAskAnswers(blocks: ChatAskBlock[], questions: Record<string, AskQuestionDraft>): AskValidation {
  const answers: ChatAnswer[] = []
  const errors: Record<string, string> = {}
  for (const block of blocks) {
    if (block.type === 'markdown') continue
    const key = askKey(block.id)
    const { answer, error } = buildQuestionAnswer(block, questions[key])
    answers.push(answer)
    if (error) errors[key] = error
  }
  const total = answers.length
  return { answers, errors, total, ready: total - Object.keys(errors).length }
}

export function askDecimalPlaces(value: number): number {
  const [mantissa, exponent = '0'] = String(value).split('e')
  return Math.max(0, (mantissa.split('.')[1]?.length ?? 0) - Number(exponent))
}

export function askNumberStep(question: Extract<AskQuestion, { type: 'number' }>): number {
  if (question.integer) return 1
  const digits = Math.max(
    ...[question.default, question.min, question.max].map((value) => (value === null ? 0 : askDecimalPlaces(value)))
  )
  return Number(`1e-${digits || 1}`)
}

/** Where the keyboard enters an ask: the first stop of its first open question, else any stop. */
export const ASK_ENTRY = '[data-ask-question-id]:not([data-collapsed]) [data-nav-stop]'

export function askCountdown(deadlineMs: number, now: number): string {
  const seconds = Math.ceil(Math.max(0, deadlineMs - now) / 1000)
  return `${Math.floor(seconds / 60)}:${String(seconds % 60).padStart(2, '0')}`
}

export function askRefusal(message: string, blocks: ChatAskBlock[]): { id: string | null; message: string } {
  const reason = message.replace(/^Invalid argument: /, '')
  let id: string | null = null
  let length = 0
  for (const block of blocks) {
    if (block.type === 'markdown') continue
    const prefix = `question \`${block.id}\`: `
    if (prefix.length > length && reason.startsWith(prefix)) {
      id = block.id
      length = prefix.length
    }
  }
  return { id, message: reason.slice(length) }
}
