import test from 'node:test'
import assert from 'node:assert/strict'
import type { ChatDeliverable } from '$lib/types/backend'
import { resolveJob, transcriptJobs } from './jobs'
import type { Step, ToolStep, Turn } from './turns'

const at = '2026-01-01T00:00:00Z'
function tool(
  id: string,
  name: string,
  args: unknown,
  details: unknown,
  deliverables: ChatDeliverable[] = []
): ToolStep {
  return {
    id,
    entryId: id,
    timestamp: at,
    endedAt: at,
    kind: 'tool',
    callId: id,
    name,
    arguments: args,
    status: 'done',
    output: [],
    details,
    result: {
      kind: 'toolResult',
      callId: id,
      toolName: name,
      content: [],
      details,
      isError: false,
      deliverables,
      durationMs: 1
    }
  }
}
function turn(steps: Step[]): Turn {
  return {
    id: 'turn:0',
    user: null,
    steps,
    assistants: [],
    end: null,
    running: false,
    startedAt: at,
    endedAt: at
  }
}
const result = (jobId: string): ChatDeliverable => ({
  kind: 'child',
  jobId,
  agent: null,
  team: null,
  note: false,
  partial: false,
  isError: false,
  text: 'Done',
  value: null
})

test('transcriptJobs derives each job outcome from the transcript alone', () => {
  const jobs = transcriptJobs([
    turn([
      tool('spawn', 'spawn', { task: 'Audit the parser\nThoroughly', agent: 'scout' }, { job: 'agent_7' }),
      tool('wait', 'wait', {}, { reason: 'results', jobs: ['agent_7/1'] }, [result('agent_7')]),
      tool('bash', 'bash', { command: 'make test' }, { job: 'bg_31' }),
      {
        id: 'wake',
        entryId: 'wake',
        timestamp: at,
        kind: 'notice',
        notice: {
          kind: 'jobWake',
          jobs: ['bg_31'],
          deliverables: [
            { kind: 'command', jobId: 'bg_31', command: 'make test', exit: { type: 'exited', code: 1 }, isError: true }
          ]
        }
      },
      tool('kill', 'kill', { id: 'bg_30' }, { id: 'bg_30', stopping: true })
    ])
  ])
  assert.deepEqual(
    [...jobs.values()].map((job) => [job.id, job.kind, job.title, job.outcome]),
    [
      ['agent_7', 'child', 'Audit the parser', 'done'],
      ['bg_31', 'command', 'make test', 'failed'],
      ['bg_30', 'command', '', 'stopped']
    ]
  )
  assert.equal(jobs.get('agent_7')?.agent, 'scout')
  assert.deepEqual(jobs.get('bg_31')?.exit, { type: 'exited', code: 1 })
})

test('a team spawn indexes the team and its members, and teammate names resolve to the team', () => {
  const jobs = transcriptJobs([
    turn([
      tool(
        'spawn',
        'spawn',
        {
          tasks: { worker: 'Build it\nCarefully', critic: 'Review it' },
          members: [{ name: 'worker', agent: 'coder' }]
        },
        { team: 'team_1', jobs: { worker: 'agent_8', critic: 'agent_9' } }
      )
    ])
  ])
  const team = jobs.get('team_1')
  assert.equal(team?.kind, 'team')
  assert.deepEqual(team?.members, [
    ['worker', 'agent_8'],
    ['critic', 'agent_9']
  ])
  const worker = jobs.get('agent_8')
  assert.deepEqual(
    [worker?.title, worker?.agent, worker?.team],
    ['Build it', 'coder', { id: 'team_1', member: 'worker' }]
  )
  assert.equal(jobs.get('agent_9')?.title, 'Review it')
  assert.deepEqual(resolveJob('worker', jobs), { job: 'team_1', member: 'worker' })
  assert.deepEqual(resolveJob('agent_9/2', jobs), { job: 'team_1', member: 'critic' })
  assert.deepEqual(resolveJob('agent_42', jobs), { job: 'agent_42', member: null })
  assert.equal(resolveJob('parent', jobs), null)
})
