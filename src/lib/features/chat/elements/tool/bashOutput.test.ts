import test from 'node:test'
import assert from 'node:assert/strict'
import type { ToolStep } from '../../turns'
import type { ChatContent } from '$lib/types/backend'
import { bashOutput, formatCommandExit, parseBashDetails } from './bashOutput'
import { toolHeader } from './toolHeader'

function call(content: ChatContent[], isError = false, details: unknown = null): ToolStep {
  return {
    id: 'tool',
    entryId: 'entry',
    timestamp: null,
    endedAt: null,
    kind: 'tool',
    callId: 'bash-call',
    name: 'bash',
    arguments: { command: 'command' },
    status: isError ? 'error' : 'done',
    output: content,
    details,
    result: {
      kind: 'toolResult',
      callId: 'bash-call',
      toolName: 'bash',
      content,
      details,
      isError,
      deliverables: [],
      durationMs: null
    }
  }
}

const text = (value: string): ChatContent => ({ type: 'text', text: value })

test('bash preview keeps twelve actual tail lines and all original whitespace when expanded', () => {
  const source = Array.from({ length: 15 }, (_, index) => `\trow ${index + 1}   value  `).join('\n') + '\n'
  const step = call([text(source)])
  const preview = bashOutput(step, 12)
  assert.equal(preview.hidden, 3)
  assert.deepEqual(preview.parts, [text(source.split('\n').slice(3).join('\n'))])
  assert.deepEqual(bashOutput(step).parts, [text(source)])
})

test('bash preserves real blank lines rather than spending preview on a phantom trailing line', () => {
  const source = 'first\n\n\tlast\n\n'
  const output = bashOutput(call([text(source)]), 3)
  assert.equal(output.hidden, 1)
  assert.deepEqual(output.parts, [text('\n\tlast\n\n')])
})

test('parseBashDetails extracts typed exit, durationMs, fullOutput, and job without regexes', () => {
  assert.deepEqual(
    parseBashDetails({
      exit: { type: 'exited', code: 3 },
      durationMs: 1250,
      fullOutput: 'artifact://full.log',
      job: 'bg-1'
    }),
    {
      exit: { type: 'exited', code: 3 },
      durationMs: 1250,
      fullOutput: 'artifact://full.log',
      job: 'bg-1'
    }
  )
  assert.deepEqual(parseBashDetails({ exit: { type: 'signaled', signal: 9 } }).exit, { type: 'signaled', signal: 9 })
  assert.deepEqual(parseBashDetails({ exit: { type: 'timedOut', afterSecs: 5 } }).exit, {
    type: 'timedOut',
    afterSecs: 5
  })
  assert.deepEqual(parseBashDetails({ exit: { type: 'killed', by: 'host' } }).exit, { type: 'killed', by: 'host' })
  assert.deepEqual(parseBashDetails({ exit: { type: 'aborted' } }).exit, { type: 'aborted' })
  assert.deepEqual(parseBashDetails({ exit: { type: 'interrupted' } }).exit, { type: 'interrupted' })
  assert.equal(parseBashDetails({ exit: { type: 'killed', by: 'unknown' } }).exit, null)
  assert.equal(parseBashDetails({ durationMs: -1 }).durationMs, null)
  assert.equal(parseBashDetails({ job: '' }).job, null)
  for (const path of [
    '',
    'artifact://',
    '/tmp/output.log',
    'artifact://invalid\npath',
    'artifact://invalid\u0000path'
  ]) {
    assert.equal(parseBashDetails({ fullOutput: path }).fullOutput, null)
  }
})

test('formatCommandExit formats all exit variants and failure flags', () => {
  assert.deepEqual(formatCommandExit({ type: 'exited', code: 0 }), { label: 'Exited (0)', failed: false })
  assert.deepEqual(formatCommandExit({ type: 'exited', code: 3 }), { label: 'Exit 3', failed: true })
  assert.deepEqual(formatCommandExit({ type: 'signaled', signal: 15 }), { label: 'Signal 15', failed: true })
  assert.deepEqual(formatCommandExit({ type: 'timedOut', afterSecs: 1 }), { label: 'Timed out (1s)', failed: true })
  assert.deepEqual(formatCommandExit({ type: 'killed', by: 'host' }), { label: 'Stopped', failed: true })
  assert.deepEqual(formatCommandExit({ type: 'killed', by: 'model' }), { label: 'Killed', failed: true })
  assert.deepEqual(formatCommandExit({ type: 'aborted' }), { label: 'Aborted', failed: true })
  assert.deepEqual(formatCommandExit({ type: 'interrupted' }), { label: 'Interrupted', failed: true })
})

test('bashOutput preserves literal exit-like lines and reads fullOutput from details', () => {
  const path = 'artifact://output-with-spaces.log'
  const body = Array.from({ length: 19 }, (_, index) => `line ${index + 1}`).join('\n')
  const source = `${body}\nCommand exited with code 7`
  const output = bashOutput(call([text(source)], true, { fullOutput: path, exit: { type: 'exited', code: 7 } }), 12)
  assert.equal(output.hidden, 8)
  assert.equal(output.fullOutput, path)
  assert.deepEqual(output.parts, [text([...body.split('\n').slice(8), 'Command exited with code 7'].join('\n'))])
})

test('image parts survive tail preview', () => {
  const image: ChatContent = { type: 'image', mimeType: 'image/png', data: 'image' }
  const source = Array.from({ length: 15 }, (_, index) => `line ${index}`).join('\n')
  const output = bashOutput(call([image, text(source), image]), 12)
  assert.equal(output.hidden, 3)
  assert.deepEqual(output.parts, [image, text(source.split('\n').slice(3).join('\n')), image])
})

test('toolHeader formats typed bash exit statuses and details durationMs', () => {
  const cases: Array<{ details: unknown; isError: boolean; expectedLabel: string | null; expectedTone: string }> = [
    {
      details: { exit: { type: 'exited', code: 0 }, durationMs: 200 },
      isError: false,
      expectedLabel: null,
      expectedTone: 'success'
    },
    { details: { exit: { type: 'exited', code: 3 } }, isError: true, expectedLabel: 'Exit 3', expectedTone: 'danger' },
    {
      details: { exit: { type: 'timedOut', afterSecs: 1 } },
      isError: true,
      expectedLabel: 'Timed out (1s)',
      expectedTone: 'danger'
    },
    {
      details: { exit: { type: 'killed', by: 'host' } },
      isError: true,
      expectedLabel: 'Stopped',
      expectedTone: 'danger'
    },
    {
      details: { exit: { type: 'killed', by: 'model' } },
      isError: true,
      expectedLabel: 'Killed',
      expectedTone: 'danger'
    },
    { details: { exit: { type: 'aborted' } }, isError: true, expectedLabel: 'Aborted', expectedTone: 'danger' },
    { details: null, isError: true, expectedLabel: 'Aborted', expectedTone: 'danger' }
  ]
  for (const item of cases) {
    const header = toolHeader(call([], item.isError, item.details), '/repo')
    assert.equal(header.tone, item.expectedTone)
    assert.deepEqual(header.status, item.expectedLabel ? { label: item.expectedLabel, tone: 'danger' } : null)
  }
  const timed = toolHeader(call([], false, { exit: { type: 'exited', code: 0 }, durationMs: 1500 }), '/repo')
  assert.equal(timed.meta, '1.5s')
})
