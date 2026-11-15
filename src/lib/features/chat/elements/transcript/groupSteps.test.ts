import test from 'node:test'
import assert from 'node:assert/strict'
import type { Step, ToolStep } from '../../turns'
import { groupSteps, REDACTED_THINKING, segmentSteps, type ThoughtStep } from './groupSteps'

function tool(id: string, name = 'read', entryId: string | null = id, details: unknown = null): ToolStep {
  return {
    id,
    entryId,
    kind: 'tool',
    callId: id,
    name,
    arguments: {},
    status: 'running',
    result: null,
    output: [],
    details
  }
}
function thinking(id: string, text: string, redacted = false, entryId: string | null = id): Step {
  return { id, entryId, kind: 'thinking', text, redacted }
}
function thought(id: string, text: string, streaming = false): ThoughtStep {
  return { kind: 'thinking', id, text, streaming }
}
const boundaries: Step[] = [
  { id: 'text', entryId: 'text', kind: 'text', text: 'Between calls' },
  { id: 'notice', entryId: 'notice', kind: 'notice', notice: { kind: 'host', text: 'Notice' } },
  { id: 'compaction', entryId: 'compaction', kind: 'compaction', summary: 'Summary', tokensBefore: 10 },
  { id: 'branch', entryId: 'branch', kind: 'branchSummary', summary: 'Branch' }
]

test('only adjacent same-name tools and adjacent thinking merge, preserving boundaries and order', () => {
  assert.deepEqual(groupSteps([]), [])
  for (const boundary of [...boundaries, thinking('thought', 'Separate'), tool('write', 'write')]) {
    const calls = [tool('a'), tool('b'), tool('c'), tool('d')]
    const expected = boundary.kind === 'thinking' ? thought(boundary.id, boundary.text) : boundary
    assert.deepEqual(groupSteps([calls[0], calls[1], boundary, calls[2], calls[3]]), [
      { kind: 'toolGroup', id: 'a', calls: calls.slice(0, 2) },
      expected,
      { kind: 'toolGroup', id: 'c', calls: calls.slice(2) }
    ])
    const singleton = groupSteps([boundary])
    if (boundary.kind === 'thinking') assert.deepEqual(singleton, [expected])
    else assert.equal(singleton[0], boundary)
  }
  for (const boundary of [...boundaries, tool('read')]) {
    assert.deepEqual(
      groupSteps([
        thinking('a', 'First'),
        thinking('b', 'Second'),
        boundary,
        thinking('c', 'Third'),
        thinking('d', 'Fourth')
      ]),
      [thought('a', 'First\n\nSecond'), boundary, thought('c', 'Third\n\nFourth')]
    )
  }
  const read = tool('read')
  const write = tool('write', 'write')
  assert.deepEqual(groupSteps([read, write, read]), [read, write, read])
})

test('visible messages end each work fold, preserving the entire turn sequence', () => {
  const intro: Step = { id: 'intro', entryId: 'intro', kind: 'text', text: 'Looking into it' }
  const update: Step = { id: 'update', entryId: 'update', kind: 'text', text: 'Found it' }
  const answer: Step = { id: 'answer', entryId: null, kind: 'text', text: 'Done' }
  const steps = groupSteps([intro, thinking('thought', 'Thinking'), tool('a'), tool('b'), update, tool('c'), answer])
  assert.deepEqual(segmentSteps(steps), [
    intro,
    { kind: 'work', id: 'work:thought', steps: steps.slice(1, 3) },
    update,
    { kind: 'work', id: 'work:c', steps: steps.slice(4, 5) },
    answer
  ])
  assert.deepEqual(segmentSteps([]), [])
  assert.deepEqual(segmentSteps([intro, answer]), [intro, answer])
})

test('notices and summaries stay visible at their original positions', () => {
  for (const boundary of boundaries) {
    const first = tool('a')
    const last = tool('b')
    assert.deepEqual(segmentSteps(groupSteps([first, boundary, last])), [
      { kind: 'work', id: 'work:a', steps: [first] },
      boundary,
      { kind: 'work', id: 'work:b', steps: [last] }
    ])
  }
})

test('streaming growth keeps the first call group id and child identities', () => {
  const first = tool('saved:0')
  const second = tool('streaming:0', 'read', null)
  const third = tool('streaming:1', 'read', null)
  assert.equal(groupSteps([first])[0], first)
  for (const calls of [
    [first, second],
    [first, second, third]
  ]) {
    const [group] = groupSteps(calls)
    assert.equal(group.kind, 'toolGroup')
    if (group.kind !== 'toolGroup') throw new Error('Expected tool group')
    assert.equal(group.id, first.id)
    assert.deepEqual(group.calls, calls)
    assert.equal(group.calls[0], first)
    assert.equal(group.calls[1], second)
  }
})

test('merged thinking preserves visible text in order without exposing redacted text', () => {
  const steps = [
    thinking('first', 'Visible before'),
    thinking('hidden', 'SECRET', true, null),
    thinking('last', 'Visible after')
  ]
  const [merged] = groupSteps(steps)
  assert.deepEqual(merged, {
    kind: 'thinking',
    id: 'first',
    text: `Visible before\n\n${REDACTED_THINKING}\n\nVisible after`,
    streaming: true
  })
  assert.equal(JSON.stringify(merged).includes('SECRET'), false)
  assert.deepEqual(groupSteps([thinking('x', 'SECRET', true)]), [thought('x', REDACTED_THINKING)])
  assert.deepEqual(groupSteps([thinking('hidden', 'SECRET', true), thinking('visible', 'Visible')]), [
    thought('hidden', `${REDACTED_THINKING}\n\nVisible`)
  ])
  assert.deepEqual(groupSteps([thinking('a', 'SECRET', true), thinking('b', 'SECRET', true)]), [
    thought('a', `${REDACTED_THINKING}\n\n${REDACTED_THINKING}`)
  ])
  assert.deepEqual(groupSteps([thinking('a', 'Growing', false, null), thinking('b', 'Saved')]), [
    thought('a', 'Growing\n\nSaved', true)
  ])
})

test('receipt tools never merge into toolGroup and stay outside folded work segments', () => {
  const spawnA = tool('s1', 'spawn')
  const spawnB = tool('s2', 'spawn')
  const waitCall = tool('w1', 'wait')
  const bgBash = tool('b1', 'bash', 'b1', { job: 'bg_1' })
  const fgBash = tool('b2', 'bash')
  const readA = tool('r1', 'read')
  const readB = tool('r2', 'read')

  const receipt = (call: ToolStep) => ({ kind: 'receipt', id: call.id, call })
  assert.deepEqual(groupSteps([spawnA, spawnB, waitCall, bgBash, fgBash]), [
    receipt(spawnA),
    receipt(spawnB),
    receipt(waitCall),
    receipt(bgBash),
    fgBash
  ])
  assert.deepEqual(groupSteps([fgBash, bgBash]), [fgBash, receipt(bgBash)])

  const steps = groupSteps([readA, spawnA, waitCall, bgBash, readB])
  assert.deepEqual(segmentSteps(steps), [
    { kind: 'work', id: 'work:r1', steps: [readA] },
    receipt(spawnA),
    receipt(waitCall),
    receipt(bgBash),
    { kind: 'work', id: 'work:r2', steps: [readB] }
  ])
})
