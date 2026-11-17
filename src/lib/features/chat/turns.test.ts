import test from 'node:test'
import assert from 'node:assert/strict'
import type { ChatEntry, ChatEntryBody, ChatLive } from '$lib/types/backend'
import { compactionTokensAfter, transcriptTurns, turnDurationMs } from './turns'

const live: ChatLive = {
  title: null,
  running: false,
  maintenance: null,
  model: null,
  thinking: 'off',
  tools: [],
  queue: [],
  questions: [],
  retry: null,
  autoPaused: false,
  jobs: [],
  stats: {
    contextTokens: 0,
    contextWindow: 0,
    contextTrigger: null,
    contextReserve: null,
    usage: { input: 0, output: 0, cacheRead: 0, cacheWrite: 0, totalTokens: 0, cost: 0 },
    tree: null
  },
  agent: null,
  mode: null,
  effectiveMode: null,
  reviews: []
}
function entry(id: string, body: ChatEntryBody): ChatEntry {
  return { id, timestamp: '2026-01-01T00:00:00Z', body }
}
const user = entry('user:1', {
  kind: 'user',
  parts: [{ type: 'text', text: 'Do the work' }],
  anchor: null,
  siblings: ['user:1'],
  from: null,
  keywords: []
})
const thought = { type: 'thinking', text: 'Thinking', redacted: false } as const
const message = entry('answer', {
  kind: 'assistant',
  blocks: [{ type: 'text', text: 'Done' }],
  model: 'test/model',
  usage: { input: 1, output: 2, cacheRead: 0, cacheWrite: 0, totalTokens: 3, cost: 0.01 },
  stopReason: 'stop',
  error: null,
  durationMs: 10
})
const work = entry('work', {
  kind: 'assistant',
  blocks: [
    thought,
    { type: 'text', text: 'Working' },
    { type: 'toolCall', id: 'call', name: 'read', arguments: { path: 'file' } }
  ],
  model: 'test/model',
  usage: { input: 1, output: 2, cacheRead: 0, cacheWrite: 0, totalTokens: 3, cost: 0.01 },
  stopReason: 'toolUse',
  error: null,
  durationMs: 10
})
const result = entry('result', {
  kind: 'toolResult',
  callId: 'call',
  toolName: 'read',
  content: [{ type: 'text', text: 'File content' }],
  details: null,
  isError: false,
  deliverables: [],
  durationMs: 1
})
const end = entry('end', { kind: 'runEnd', outcome: 'done', reason: null })

test('steps retain final text in order and pair tool results', () => {
  const [turn] = transcriptTurns(
    [user, work, result, message, entry('compact', { kind: 'compaction', summary: 'Summary', tokensBefore: 10 }), end],
    null,
    live
  )
  assert.equal(turn.user, user)
  assert.deepEqual(
    turn.steps.map((step) => step.kind),
    ['thinking', 'text', 'tool', 'text', 'compaction']
  )
  const call = turn.steps[2]
  assert.equal(call.kind === 'tool' && call.result, result.body)
  assert.equal(turn.end, end.body)
})

test('only the last turn runs and streaming and live output belong to it', () => {
  const turns = transcriptTurns(
    [user, message, end, entry('user:2', user.body), work],
    [{ type: 'text', text: 'Continuing' }],
    {
      ...live,
      running: true,
      tools: [
        { callId: 'call', name: 'read', arguments: {}, output: [{ type: 'text', text: 'Partial' }], details: null }
      ]
    }
  )
  assert.equal(turns[0].running, false)
  assert.equal(turns[1].running, true)
  assert.deepEqual(turns[1].steps.at(-1), {
    id: 'streaming:0',
    kind: 'text',
    text: 'Continuing',
    entryId: null
  })
  const call = turns[1].steps[2]
  assert.equal(call.kind, 'tool')
  if (call.kind !== 'tool') throw new Error('Expected tool step')
  assert.equal(call.status, 'running')
  assert.deepEqual(call.output, [{ type: 'text', text: 'Partial' }])
})

test('failed tool results stay paired with their calls', () => {
  const [turn] = transcriptTurns(
    [
      user,
      work,
      entry('error', { ...(result.body as Extract<ChatEntryBody, { kind: 'toolResult' }>), isError: true }),
      end
    ],
    null,
    live
  )
  assert.deepEqual(
    turn.steps.map((step) => step.kind),
    ['thinking', 'text', 'tool']
  )
  const call = turn.steps[2]
  assert.equal(call.kind === 'tool' && call.status, 'error')
})

test('leading content has no fabricated user and leading model selections are dropped', () => {
  const turns = transcriptTurns(
    [
      entry('model', { kind: 'modelChange', model: 'test/model' }),
      entry('thinking', { kind: 'thinkingChange', thinking: 'high' }),
      message,
      user,
      work
    ],
    null,
    live
  )
  assert.equal(turns.length, 2)
  assert.equal(turns[0].user, null)
  assert.equal(turns[0].id, 'turn:0')
  assert.deepEqual(
    turns[0].steps.map((step) => step.kind),
    ['text']
  )
  assert.equal(turns[0].startedAt, message.timestamp)
  assert.equal(turns[1].id, user.id)
})

test('turn settings retain unchanged reasoning; later picker changes are command rows, not config', () => {
  const thinking = (id: string, level: 'low' | 'medium' | 'high') =>
    entry(id, { kind: 'thinkingChange', thinking: level })
  const turns = transcriptTurns(
    [
      thinking('t0', 'low'),
      user,
      message,
      thinking('t1', 'medium'),
      thinking('t2', 'high'),
      { ...user, id: 'u2' },
      thinking('t3', 'low'),
      thinking('t4', 'high'),
      { ...user, id: 'u3' }
    ],
    null,
    live
  )
  assert.deepEqual(
    turns.map((turn) => turn.config),
    [
      { model: 'test/model', thinking: 'low' },
      { model: null, thinking: 'high' },
      { model: null, thinking: 'high' }
    ]
  )
  assert.deepEqual(
    turns.map((turn) => turn.steps.map((step) => step.kind)),
    [['text', 'thinkingChange', 'thinkingChange'], ['thinkingChange', 'thinkingChange'], []]
  )
})

test('assistant settings reflect recorded reasoning, including off and mid-turn changes', () => {
  const [turn] = transcriptTurns(
    [
      user,
      entry('off', { kind: 'thinkingChange', thinking: 'off' }),
      work,
      entry('high', { kind: 'thinkingChange', thinking: 'high' }),
      message,
      entry('next', { kind: 'thinkingChange', thinking: 'low' })
    ],
    null,
    live
  )
  assert.deepEqual(turn.config, { model: 'test/model', thinking: 'high' })
  const [off] = transcriptTurns([entry('off', { kind: 'thinkingChange', thinking: 'off' }), user, message], null, live)
  assert.equal(off.config.thinking, 'off')
  const [unknown] = transcriptTurns([user, message], null, { ...live, thinking: 'high' })
  assert.equal(unknown.config.thinking, null)
})

test('replayed turns retain text without an invented run end', () => {
  const [turn] = transcriptTurns([user, message], null, live)
  assert.equal(turn.end, null)
  assert.deepEqual(
    turn.steps.map((step) => step.kind),
    ['text']
  )
  assert.deepEqual(transcriptTurns([], null, live), [])
})

test('duration uses entry timestamps when finished and the clock while running', () => {
  const startedAt = '2026-01-01T00:00:00Z'
  const endedAt = '2026-01-01T00:00:12Z'
  const [turn] = transcriptTurns(
    [
      { ...user, timestamp: startedAt },
      { ...message, timestamp: endedAt }
    ],
    null,
    live
  )
  assert.equal(turn.startedAt, startedAt)
  assert.equal(turn.endedAt, endedAt)
  assert.equal(turnDurationMs(turn, Date.parse(endedAt) + 5000), 12_000)
  assert.equal(turnDurationMs({ ...turn, running: true }, Date.parse(endedAt) + 5000), 17_000)
  assert.equal(turnDurationMs({ ...turn, startedAt: null }, Date.now()), null)
  assert.equal(turnDurationMs({ ...turn, endedAt: null }, Date.now()), null)
  const [userOnly] = transcriptTurns([user], null, live)
  assert.equal(userOnly.endedAt, null)
})

test('later config and maintenance entries join the turn without extending its duration', () => {
  const endedAt = '2026-01-01T00:00:12Z'
  const [turn] = transcriptTurns(
    [
      user,
      { ...message, timestamp: endedAt },
      { ...end, timestamp: endedAt },
      { id: 'model', timestamp: '2026-01-01T01:00:00Z', body: { kind: 'modelChange', model: 'x/y' } },
      { id: 'compact', timestamp: '2026-01-01T01:00:05Z', body: { kind: 'compaction', summary: 's', tokensBefore: 1 } }
    ],
    null,
    live
  )
  assert.equal(turn.endedAt, endedAt)
  assert.equal(turnDurationMs(turn, Date.parse(endedAt) + 5000), 12_000)
  assert.ok(turn.steps.some((step) => step.kind === 'modelChange'))
})

test('compactionTokensAfter reads post-compaction assistant input tokens or falls back to live context for the latest checkpoint', () => {
  const afterMsg = entry('a-after', {
    kind: 'assistant',
    blocks: [{ type: 'text', text: 'Continued' }],
    model: 'test/m',
    usage: { input: 1200, output: 100, cacheRead: 38_800, cacheWrite: 0, totalTokens: 40_100, cost: 0 },
    stopReason: 'stop',
    error: null,
    durationMs: 10
  })
  const compact1 = entry('c1', { kind: 'compaction', summary: 'First', tokensBefore: 196_000 })
  const compact2 = entry('c2', { kind: 'compaction', summary: 'Second', tokensBefore: 180_000 })
  const turns = transcriptTurns(
    [user, message, end, compact1, entry('u2', user.body), afterMsg, end, compact2],
    null,
    live
  )
  assert.equal(compactionTokensAfter(turns, 0, 'c1', 25_000), 40_000)
  assert.equal(compactionTokensAfter(turns, 1, 'c2', 25_000), 25_000)
  assert.equal(compactionTokensAfter(turns, 1, 'c2', 0), null)
})

test('call tool calls unwrap with explicit, running, then result name and args precedence', () => {
  const callEntry = entry('call-msg', {
    kind: 'assistant',
    blocks: [
      { type: 'toolCall', id: 'c1', name: 'call', arguments: { tool: 'read', args: { path: 'a.ts' } } },
      { type: 'toolCall', id: 'c2', name: 'call', arguments: {} },
      { type: 'toolCall', id: 'c3', name: 'call', arguments: {} },
      { type: 'toolCall', id: 'c4', name: 'read', arguments: { path: 'b.ts' } }
    ],
    model: 'test/model',
    usage: { input: 1, output: 1, cacheRead: 0, cacheWrite: 0, totalTokens: 2, cost: 0 },
    stopReason: 'toolUse',
    error: null,
    durationMs: 5
  })
  const c3Result = entry('r3', {
    kind: 'toolResult',
    callId: 'c3',
    toolName: 'grep',
    content: [{ type: 'text', text: 'match' }],
    details: null,
    isError: false,
    deliverables: [],
    durationMs: 1
  })
  const [turn] = transcriptTurns([user, callEntry, c3Result], null, {
    ...live,
    running: true,
    tools: [{ callId: 'c2', name: 'bash', arguments: { command: 'ls' }, output: [], details: null }]
  })
  const steps = turn.steps.filter((s) => s.kind === 'tool')
  assert.equal(steps.length, 4)
  assert.equal(steps[0].name, 'read')
  assert.deepEqual(steps[0].arguments, { path: 'a.ts' })

  assert.equal(steps[1].name, 'bash')
  assert.deepEqual(steps[1].arguments, { command: 'ls' })

  assert.equal(steps[2].name, 'grep')
  assert.deepEqual(steps[2].arguments, {})

  assert.equal(steps[3].name, 'read')
  assert.deepEqual(steps[3].arguments, { path: 'b.ts' })
})

test('autonomous jobWake after runEnd opens a fresh userless turn with unique id and wake timestamp', () => {
  const wakeTime = '2026-01-01T00:05:00Z'
  const turns = transcriptTurns(
    [
      user,
      message,
      end,
      {
        id: 'wake-1',
        timestamp: wakeTime,
        body: { kind: 'notice', notice: { kind: 'jobWake', jobs: ['j1'], deliverables: [] } }
      },
      { ...message, id: 'wake-answer', timestamp: '2026-01-01T00:05:05Z' }
    ],
    null,
    live
  )
  assert.equal(turns.length, 2)
  assert.equal(turns[0].end, end.body)
  assert.equal(turns[1].user, null)
  assert.equal(turns[1].id, 'turn:1')
  assert.equal(turns[1].startedAt, wakeTime)
  assert.deepEqual(
    turns[1].steps.map((s) => s.kind),
    ['notice', 'text']
  )
})

test('incoming mail notice starts its own userless turn even mid-session with unique ids', () => {
  const mail1Time = '2026-01-01T00:01:00Z'
  const mail2Time = '2026-01-01T00:02:00Z'
  const turns = transcriptTurns(
    [
      user,
      work,
      {
        id: 'mail-1',
        timestamp: mail1Time,
        body: { kind: 'notice', notice: { kind: 'mail', sender: 'scout', text: 'Found it' } }
      },
      message,
      end,
      {
        id: 'mail-2',
        timestamp: mail2Time,
        body: { kind: 'notice', notice: { kind: 'mail', sender: 'reviewer', text: 'LGTM' } }
      }
    ],
    null,
    live
  )
  assert.equal(turns.length, 3)
  assert.equal(turns[0].id, user.id)
  assert.equal(turns[1].user, null)
  assert.equal(turns[1].id, 'turn:1')
  assert.equal(turns[1].startedAt, mail1Time)
  assert.deepEqual(
    turns[1].steps.map((s) => s.kind),
    ['notice', 'text']
  )
  assert.equal(turns[2].user, null)
  assert.equal(turns[2].id, 'turn:2')
  assert.equal(turns[2].startedAt, mail2Time)
  assert.deepEqual(
    turns[2].steps.map((s) => s.kind),
    ['notice']
  )
})
