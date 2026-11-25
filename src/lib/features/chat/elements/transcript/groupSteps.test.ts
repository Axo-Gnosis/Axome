import test from 'node:test'
import assert from 'node:assert/strict'
import type { ChatDeliverable, ChatNotice } from '$lib/types/backend'
import { transcriptJobs } from '../../jobs'
import type { Step, ToolStep, Turn } from '../../turns'
import {
  groupSteps,
  narrationTitle,
  REDACTED_THINKING,
  segmentSteps,
  splitTurn,
  type TextStep,
  type ThoughtStep,
  type TranscriptSegment,
  type TranscriptStep,
  type WorkSegment
} from './groupSteps'
import { toolHeader } from '../tool/toolHeader'

const none = new Map()

function tool(id: string, name = 'read', entryId: string | null = id, details: unknown = null): ToolStep {
  return {
    id,
    entryId,
    timestamp: null,
    endedAt: null,
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
  return { id, entryId, timestamp: null, kind: 'thinking', text, redacted }
}
function thought(id: string, text: string, streaming = false, entryId: string | null = id): ThoughtStep {
  return { kind: 'thinking', id, entryId, text, streaming }
}
function work(id: string, steps: TranscriptStep[], narration: TextStep | null = null): WorkSegment {
  return { kind: 'work', id, narration, steps }
}
const times = { timestamp: null }
const boundaries: Step[] = [
  { id: 'text', entryId: 'text', ...times, kind: 'text', text: 'Between calls' },
  { id: 'notice', entryId: 'notice', ...times, kind: 'notice', notice: { kind: 'host', text: 'Notice' } }
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
      [thought('a', 'First\n\nSecond', false, 'b'), boundary, thought('c', 'Third\n\nFourth', false, 'd')]
    )
  }
  const read = tool('read')
  const write = tool('write', 'write')
  assert.deepEqual(groupSteps([read, write, read]), [read, write, read])
})

test('a record-bearing send remains a normal tool step instead of a receipt or job', () => {
  const details = {
    to: 'child',
    kind: 'steer',
    sessionId: 'child',
    files: [{ ledger: { path: '/repo/entry.md', sha256: 'hash', access: 'read' } }]
  }
  const send = tool('send', 'send', 'send', details)
  send.status = 'error'
  send.result = {
    kind: 'toolResult',
    toolName: 'send',
    callId: send.callId,
    isError: true,
    content: [],
    details,
    deliverables: [],
    durationMs: null
  }
  const grouped = groupSteps([send])
  assert.deepEqual(grouped, [send])
  assert.deepEqual(segmentSteps(grouped, none), [work('work:send', [send])])
})

test('the text before a step that made calls becomes its narration; the text after the last step is the answer', () => {
  const intro: Step = { id: 'intro', entryId: 'intro', ...times, kind: 'text', text: 'Looking into it' }
  const update: Step = { id: 'update', entryId: 'update', ...times, kind: 'text', text: 'Found it' }
  const answer: Step = { id: 'answer', entryId: null, ...times, kind: 'text', text: 'Done' }
  const steps = groupSteps([intro, thinking('thought', 'Thinking'), tool('a'), tool('b'), update, tool('c'), answer])
  assert.deepEqual(segmentSteps(steps, none), [
    work('work:thought', steps.slice(1, 3), intro as TextStep),
    work('work:c', steps.slice(4, 5), update as TextStep),
    answer
  ])
  assert.deepEqual(segmentSteps([], none), [])
  assert.deepEqual(segmentSteps([intro, answer], none), [intro, answer])
})

test('adjacent maintenance becomes one band between the work blocks around it', () => {
  const first = tool('a')
  const last = tool('b')
  const compaction: Step = {
    id: 'compaction',
    entryId: 'compaction',
    ...times,
    kind: 'compaction',
    summary: 'Summary',
    tokensBefore: 10
  }
  const moved: Step = { id: 'moved', entryId: 'moved', ...times, kind: 'contextMoved', blocks: 2 }
  assert.deepEqual(segmentSteps(groupSteps([first, compaction, moved, last]), none), [
    work('work:a', [first]),
    { kind: 'band', id: 'band:compaction', steps: [compaction, moved] },
    work('work:b', [last])
  ])
})

test('notices stay visible at their original positions', () => {
  for (const boundary of boundaries) {
    const first = tool('a')
    const last = tool('b')
    // A text before calls is absorbed as the next step's narration; a notice stays visible.
    assert.deepEqual(
      segmentSteps(groupSteps([first, boundary, last]), none),
      boundary.kind === 'text'
        ? [work('work:a', [first]), work('work:b', [last], boundary)]
        : [work('work:a', [first]), boundary, work('work:b', [last])]
    )
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
    entryId: 'last',
    text: `Visible before\n\n${REDACTED_THINKING}\n\nVisible after`,
    streaming: true
  })
  assert.equal(JSON.stringify(merged).includes('SECRET'), false)
  assert.deepEqual(groupSteps([thinking('x', 'SECRET', true)]), [thought('x', REDACTED_THINKING)])
  assert.deepEqual(groupSteps([thinking('hidden', 'SECRET', true), thinking('visible', 'Visible')]), [
    thought('hidden', `${REDACTED_THINKING}\n\nVisible`, false, 'visible')
  ])
  assert.deepEqual(groupSteps([thinking('a', 'SECRET', true), thinking('b', 'SECRET', true)]), [
    thought('a', `${REDACTED_THINKING}\n\n${REDACTED_THINKING}`, false, 'b')
  ])
  assert.deepEqual(groupSteps([thinking('a', 'Growing', false, null), thinking('b', 'Saved')]), [
    thought('a', 'Growing\n\nSaved', true, 'b')
  ])
})

test('receipt tools never merge into toolGroup', () => {
  const spawnA = tool('s1', 'spawn')
  const spawnB = tool('s2', 'spawn')
  const waitCall = tool('w1', 'wait')
  const bgBash = tool('b1', 'bash', 'b1', { job: 'bg_1' })
  const fgBash = tool('b2', 'bash')

  const receipt = (call: ToolStep) => ({ kind: 'receipt', id: call.id, call })
  assert.deepEqual(groupSteps([spawnA, spawnB, waitCall, bgBash, fgBash]), [
    receipt(spawnA),
    receipt(spawnB),
    receipt(waitCall),
    receipt(bgBash),
    fgBash
  ])
  assert.deepEqual(groupSteps([fgBash, bgBash]), [fgBash, receipt(bgBash)])
})

function done(id: string, name: string, args: unknown, details: unknown, deliverables: ChatDeliverable[] = []) {
  const call = tool(id, name, id, details)
  call.arguments = args
  call.status = 'done'
  call.result = {
    kind: 'toolResult',
    callId: id,
    toolName: name,
    content: [],
    details,
    isError: false,
    deliverables,
    durationMs: 1
  }
  return call
}

test('only marked failed calls break groups and folds, including coordination receipts', () => {
  const details = { blocked: { origin: 'extension gate (scope abc123)' } }
  const reason = { type: 'text' as const, text: 'Tool execution was blocked' }
  const fail = (call: ToolStep) => {
    call.status = 'error'
    call.result!.isError = true
    call.result!.content = call.output = [reason]
    return call
  }
  const first = tool('first', 'bash')
  const stopped = fail(done('blocked-bash', 'bash', { command: 'printf hello' }, details))
  const ordinary = fail(done('ordinary', 'bash', {}, null))
  const successful = done('successful', 'bash', {}, details)
  const send = fail(done('blocked-send', 'send', { to: 'child' }, details))
  const grouped = groupSteps([first, stopped, ordinary, successful, send])
  const after = { kind: 'toolGroup' as const, id: ordinary.id, calls: [ordinary, successful] }
  assert.deepEqual(grouped, [first, stopped, after, send])
  assert.deepEqual(segmentSteps(grouped, none), [
    work('work:first', [first]),
    stopped,
    work('work:ordinary', [after]),
    send
  ])

  const header = toolHeader(stopped, '/repo')
  assert.equal(header.verb, 'Blocked')
  assert.equal(header.command, 'printf hello')
  assert.equal(header.status, null)
  assert.deepEqual(header.blocked, { extension: { id: 'gate', scoped: true }, reason: reason.text })
  assert.equal(toolHeader(ordinary, '/repo').blocked, null)
  assert.equal(toolHeader(successful, '/repo').blocked, null)
})

const text = (id: string): Step => ({ id, entryId: id, ...times, kind: 'text', text: id })
const notice = (id: string, value: ChatNotice): Step => ({ id, entryId: id, ...times, kind: 'notice', notice: value })
const child = (jobId: string, note: boolean): ChatDeliverable => ({
  kind: 'child',
  jobId,
  agent: null,
  team: null,
  note,
  partial: false,
  isError: false,
  text: note ? 'Note' : 'Result',
  value: null
})
function turnOf(steps: Step[]): Turn {
  return {
    id: 'turn:0',
    user: null,
    steps,
    assistants: [],
    end: null,
    running: false,
    startedAt: null,
    endedAt: null,
    personas: [],
    modes: []
  }
}

test('job receipts and wakes fold into one job step per work segment', () => {
  const turn = turnOf([
    tool('read'),
    done('spawn', 'spawn', { task: 'Audit', agent: 'scout' }, { job: 'agent_7' }),
    text('intro'),
    done('send', 'send', { to: 'agent_7', message: 'Also' }, { to: 'agent_7', kind: 'followUp', sessionId: 's' }),
    done('wait', 'wait', {}, { reason: 'results', jobs: ['agent_7/1'] }, [child('agent_7', true)]),
    notice('wake2', { kind: 'jobWake', jobs: ['agent_7/2'], deliverables: [child('agent_7', true)] }),
    notice('wake3', { kind: 'jobWake', jobs: ['agent_7/3'], deliverables: [child('agent_7', false)] }),
    done('bash', 'bash', { command: 'make test' }, { job: 'bg_31' }),
    notice('wake4', {
      kind: 'jobWake',
      jobs: ['bg_31'],
      deliverables: [
        { kind: 'command', jobId: 'bg_31', command: 'make test', exit: { type: 'exited', code: 1 }, isError: true }
      ]
    }),
    done('kill', 'kill', { id: 'bg_30' }, { id: 'bg_30', stopping: true }),
    text('answer')
  ])
  const segments = segmentSteps(groupSteps(turn.steps), transcriptJobs([turn]))
  assert.deepEqual(
    segments.map((segment) => segment.kind),
    ['work', 'work', 'text']
  )
  const summary = (index: number) => {
    const segment = segments[index]
    if (segment.kind !== 'work') throw new Error('Expected work segment')
    return segment.steps.map((step) =>
      step.kind === 'job'
        ? [step.id, step.target, step.events.map((event) => event.kind), step.continues]
        : [step.id, step.kind]
    )
  }
  assert.deepEqual(summary(0), [
    ['read', 'tool'],
    ['job:work:read:agent_7', 'child', ['spawn'], true]
  ])
  assert.deepEqual(summary(1), [
    ['job:work:send:agent_7', 'child', ['send', 'wait', 'deliverable', 'deliverable', 'deliverable'], false],
    ['job:work:send:bg_31', 'command', ['spawn', 'deliverable'], false],
    ['job:work:send:bg_30', 'command', ['kill'], false]
  ])
  const work = segments[1]
  if (work.kind !== 'work' || work.steps[0].kind !== 'job') throw new Error('Expected job step')
  assert.equal(work.narration?.text, 'intro')
  assert.deepEqual(work.steps[0].events[0], { kind: 'send', member: null, call: turn.steps[3], send: 'followUp' })
})

test('a yield receipt ends work, and a note to the parent stays a receipt row inside it', () => {
  const yieldCall = done('yield', 'yield', {}, null)
  const steps = groupSteps([tool('a'), yieldCall, tool('b', 'grep')])
  assert.deepEqual(
    segmentSteps(steps, none).map((segment) => segment.kind),
    ['work', 'receipt', 'work']
  )
  const note = done('note', 'send', { to: 'parent', message: 'Hi' }, { to: 'parent', kind: 'note', sessionId: 's' })
  const [segment] = segmentSteps(groupSteps([tool('c'), note]), none)
  assert.equal(segment.kind, 'work')
  if (segment.kind !== 'work') throw new Error('Expected work segment')
  assert.deepEqual(segment.steps[1], { kind: 'receipt', id: 'note', call: note })
})

test('thoughts written just before a narration move into the step it introduces', () => {
  const said = (id: string, entryId: string): Step => ({ id, entryId, ...times, kind: 'text', text: `Said ${id}` })
  const x1 = said('x1', 'm1') as TextStep
  const x2 = said('x2', 'm2') as TextStep
  const answer = said('a', 'm3')
  const c1 = tool('c1', 'read', 'm1')
  const c2 = tool('c2', 'write', 'm2')
  const steps = groupSteps([
    thinking('t1', '…', false, 'm1'),
    x1,
    c1,
    thinking('t2', '…', false, 'm2'),
    x2,
    c2,
    thinking('t3', '…', false, 'm3'),
    answer
  ])
  assert.deepEqual(segmentSteps(steps, none), [
    work('work:c1', [thought('t1', '…', false, 'm1'), c1], x1),
    work('work:c2', [thought('t2', '…', false, 'm2'), c2, thought('t3', '…', false, 'm3')], x2),
    answer
  ])
})

test('a thought-only run takes no narration', () => {
  const segments = segmentSteps(groupSteps([text('p1'), thinking('t', 'Hmm'), text('p2')]), none)
  assert.deepEqual(
    segments.map((segment) => segment.kind),
    ['text', 'work', 'text']
  )
  assert.equal((segments[1] as WorkSegment).narration, null)
})

test('the reply starts after the last step with calls, and a finished turn always ends on its last text', () => {
  const calls = work('work:x', [tool('x')])
  const musing = work('work:t', [thought('t', 'Hmm')])
  const [a, b] = groupSteps([text('a'), text('b')]) as TextStep[]
  const replyStart = (segments: TranscriptSegment[], running: boolean) => splitTurn(segments, running).work.length
  assert.equal(replyStart([calls, a], false), 1)
  assert.equal(replyStart([calls], false), 0)
  assert.equal(replyStart([calls], true), 1)
  assert.equal(replyStart([a], false), 0)
  assert.equal(replyStart([calls, a, musing, b], false), 1)
  // Stopped mid-step: the last step's title is the answer, and the step follows it untitled.
  const titled = work('work:y', [tool('y')], b)
  assert.deepEqual(splitTurn([calls, titled], false), { work: [calls], reply: [b, work('work:y', [tool('y')])] })
  assert.deepEqual(splitTurn([calls, titled], true), { work: [calls, titled], reply: [] })
})

test('step titles drop Markdown markers', () => {
  assert.equal(narrationTitle('## Now the **table** in `spec.ts`:\n\nmore'), 'Now the table in spec.ts:')
})
