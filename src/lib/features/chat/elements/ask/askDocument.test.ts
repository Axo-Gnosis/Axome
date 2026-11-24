import test from 'node:test'
import assert from 'node:assert/strict'
import type { ToolStep } from '../../turns'
import {
  answerError,
  askCountdown,
  askKey,
  askNumberStep,
  askRefusal,
  askSections,
  buildAskAnswers,
  createAskDraft,
  parseAskBlocks
} from './askDocument'
import { parseAskResult } from './askResult'

const options = [
  { value: 'overlay', label: 'Overlay the game', recommended: true },
  { value: 'replace', label: 'Replace the HUD' }
]
const choice = { type: 'choice', id: 'layout', question: 'Layout?', options }
const number = {
  type: 'number',
  id: 'countdown',
  question: 'Countdown?',
  optional: true,
  integer: true,
  min: 0,
  max: 5,
  default: 3
}
const title = { type: 'text', id: 'title', question: 'Title?', prefill: 'Paused' }
const owner = { type: 'text', id: '__proto__', question: 'Owner?' }

function document(...input: unknown[]) {
  const blocks = parseAskBlocks({ blocks: input })
  assert.ok(blocks)
  return {
    ...createAskDraft(blocks),
    build() {
      return buildAskAnswers(blocks, this.questions)
    }
  }
}

test('keeps wire values distinct from labels and single choice exclusive with Other', () => {
  const draft = document(choice, {
    ...choice,
    id: 'unsuggested',
    options: options.map(({ value, label }) => ({ value, label }))
  })
  assert.deepEqual(draft.build().answers, [{ id: 'layout', value: 'overlay' }, { id: 'unsuggested' }])
  assert.equal(draft.build().ready, 1)
  const layout = draft.questions.$layout
  Object.assign(layout, { selected: [], otherOpen: true, other: ' custom ', noteOpen: true, note: ' note ' })
  assert.deepEqual(draft.build().answers[0], { id: 'layout', other: ' custom ', note: ' note ' })
  layout.selected = ['overlay']
  assert.match(draft.build().errors.$layout, /not both/)
})

test('supports ordered multi-choice and Other, rejects invalid picks and unfinished Other', () => {
  const draft = document({ ...choice, optional: true, multi: true })
  const layout = draft.questions.$layout
  Object.assign(layout, { selected: ['replace', 'overlay'], otherOpen: true, other: ' own ' })
  assert.deepEqual(draft.build().answers, [{ id: 'layout', value: ['replace', 'overlay'], other: ' own ' }])
  layout.selected = []
  assert.equal(draft.build().ready, 1)
  layout.selected = ['replace', 'replace']
  assert.match(draft.build().errors.$layout, /picked twice/)
  layout.selected = ['missing']
  assert.match(draft.build().errors.$layout, /not an option/)
  Object.assign(layout, { selected: ['replace'], other: '  ' })
  assert.equal(draft.build().errors.$layout, 'Type your answer, or pick one of the options.')
  layout.selected = []
  assert.equal(draft.build().ready, 0)
})

test('validates omissions and notes, preserving suggestions and hostile IDs', () => {
  const draft = document({ type: 'markdown', text: '' }, number, title, owner)
  const key = askKey('__proto__')
  assert.deepEqual(draft.build().answers, [
    { id: 'countdown', value: 3 },
    { id: 'title', value: 'Paused' },
    { id: '__proto__' }
  ])
  assert.equal(draft.build().ready, 2)
  const { questions, blocks } = draft
  questions[key].text = ' \t\n '
  assert.equal(draft.build().errors[key], 'Enter an answer.')
  questions[key].text = ' Me '
  questions.$countdown.text = '0'
  assert.equal(draft.build().ready, 3)
  assert.deepEqual(draft.build().answers[0], { id: 'countdown', value: 0 })
  Object.assign(questions.$countdown, { text: '', noteOpen: true, note: 'kept' })
  assert.equal(draft.build().errors.$countdown, 'A note needs an answer.')
  questions.$countdown.skipped = true
  assert.deepEqual(draft.build().answers[0], { id: 'countdown' })
  assert.equal(draft.build().ready, 3)
  const titleQuestion = blocks[2]
  assert.ok(titleQuestion.type === 'text')
  for (const extra of [{ other: 'not allowed' }, { note: 'not allowed' }]) {
    assert.match(answerError(titleQuestion, { ...draft.build().answers[1], ...extra }) ?? '', /takes no/)
  }
  assert.deepEqual(askRefusal('Invalid argument: question `__proto__`: refused', blocks), {
    id: '__proto__',
    message: 'refused'
  })
  const longer = document(owner, { ...owner, id: '__proto__`: x' })
  assert.deepEqual(askRefusal('question `__proto__`: x`: refused', longer.blocks), {
    id: '__proto__`: x',
    message: 'refused'
  })
  assert.deepEqual(askRefusal('Invalid argument: question `unknown`: refused', blocks), {
    id: null,
    message: 'question `unknown`: refused'
  })
})

test('validates complete decimal numerals, integer bounds and exponent-aware steps', () => {
  const draft = document(number, { type: 'number', id: 'decimal', question: 'Decimal?', min: -1e-7, default: 1.25 })
  const field = draft.questions.$countdown
  for (const [text, expected] of [
    ['0', undefined],
    ['5', undefined],
    ['6', 'Must be between 0 and 5.'],
    ['2.5', 'Enter a whole number.'],
    ['2px', 'Enter a number.'],
    ['0xff', 'Enter a number.'],
    ['1e500', 'Enter a number.'],
    ['Infinity', 'Enter a number.']
  ]) {
    field.text = text!
    assert.equal(draft.build().errors.$countdown, expected)
  }
  draft.questions.$decimal.text = '2.51e-8'
  assert.deepEqual(draft.build().answers[1], { id: 'decimal', value: 2.51e-8 })
  assert.equal(draft.build().errors.$decimal, undefined)
  const decimal = draft.blocks[1]
  assert.ok(decimal.type === 'number')
  assert.equal(askNumberStep(decimal), 1e-7)
  assert.equal(askCountdown(61_001, 0), '1:02')
  assert.equal(askCountdown(10, 20), '0:00')
})

test('rejects malformed documents and invalid defaults', () => {
  for (const blocks of [
    [],
    [{ type: 'markdown', text: '' }],
    [choice, choice],
    [{ ...choice, id: ' ' }],
    [{ ...choice, multi: null }],
    [{ ...choice, options: [options[0]] }],
    [{ ...choice, options: [options[0], options[0]] }],
    [{ ...choice, options: [options[0], { ...options[1], recommended: true }] }],
    [{ ...number, min: 5, max: 4 }],
    [{ ...number, min: 0.1, max: 0.9, default: null }],
    [{ ...number, default: 2.5 }],
    [{ ...number, default: 6 }],
    [{ ...number, max: Infinity }]
  ])
    assert.equal(parseAskBlocks({ blocks }), null)
  assert.equal(parseAskBlocks({ questions: [choice] }), null)
})

const call: ToolStep = {
  kind: 'tool',
  id: 'step',
  entryId: null,
  timestamp: null,
  callId: 'ask-1',
  name: 'ask',
  status: 'done',
  result: null,
  endedAt: null,
  output: [],
  arguments: { blocks: [{ ...choice, multi: true }, number, owner] },
  details: null
}

test('reads only typed recorded answers and recognizes terminal outcomes', () => {
  const answers = JSON.parse(
    '{"layout":{"value":["replace","overlay"],"other":"Mine","note":"because"},"countdown":null,"__proto__":{"value":"Me"}}'
  )
  const result = parseAskResult({ ...call, details: { answers } })
  assert.equal(result.outcome, 'answered')
  assert.deepEqual(result.rows.$layout, {
    answered: true,
    text: 'Replace the HUD, Overlay the game, "Mine"',
    note: 'because',
    suggested: false
  })
  assert.equal(result.rows.$countdown.text, 'no answer')
  assert.equal(result.rows[askKey('__proto__')].text, '"Me"')
  answers.__proto__ = null
  const timedOut = parseAskResult({ ...call, details: { timedOut: true, answers } })
  assert.equal(timedOut.outcome, 'timedOut')
  assert.equal(timedOut.rows.$layout.suggested, true)
  assert.deepEqual(timedOut.rows[askKey('__proto__')], {
    answered: false,
    text: 'no answer',
    note: null,
    suggested: false
  })
  assert.equal(parseAskResult({ ...call, details: { answers } }).outcome, 'unavailable')
  assert.equal(parseAskResult({ ...call, details: { answers: [] } }).message, 'Question details unavailable.')
  assert.equal(
    parseAskResult({ ...call, details: { answers: { ...answers, layout: { value: ['invalid'] } }, timedOut: true } })
      .outcome,
    'unavailable'
  )
  assert.equal(parseAskResult({ ...call, arguments: { questions: [choice] }, details: { results: [] } }).blocks, null)
  assert.equal(parseAskResult({ ...call, details: { outcome: 'steered' } }).outcome, 'steered')
  for (const [text, expected] of [
    ['Ask tool was cancelled by the user', 'dismissed'],
    ['Ask input was cancelled', 'cancelled'],
    ['other error', 'failed']
  ]) {
    const result = parseAskResult({ ...call, status: 'error', output: [{ type: 'text', text }] })
    assert.equal(result.outcome, expected)
    if (expected === 'failed') assert.equal(result.message, text)
  }
})

test('groups back-to-back questions into one section, split by Markdown', () => {
  const blocks = parseAskBlocks({ blocks: [choice, number, { type: 'markdown', text: 'Then' }, title] })
  assert.ok(blocks)
  assert.deepEqual(
    askSections(blocks).map((section) =>
      section.type === 'markdown' ? section.text : section.questions.map((question) => question.id)
    ),
    [['layout', 'countdown'], 'Then', ['title']]
  )
})
