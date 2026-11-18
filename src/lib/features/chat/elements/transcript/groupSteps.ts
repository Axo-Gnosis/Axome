import type { ChatDeliverable } from '$lib/types/backend'
import { jobKind, resolveJob, type JobKind, type TranscriptJob } from '../../jobs'
import { parseBashDetails } from '../tool/bashOutput'
import { isReceiptTool, killReceipt, sendReceipt, spawnReceipt, waitReceipt } from '../tool/receipt'
import type { Step, ToolStep } from '../../turns'

/** One or more adjacent thinking steps, already redacted, as the transcript renders them. */
export type ThoughtStep = { kind: 'thinking'; id: string; text: string; streaming: boolean }
export type ToolGroupStep = { kind: 'toolGroup'; id: string; calls: ToolStep[] }
export type ReceiptStep = { kind: 'receipt'; id: string; call: ToolStep }
/** What a job did inside one work block; `member` names the teammate for team jobs. */
export type JobEvent = { member: string | null } & (
  | { kind: 'spawn'; call: ToolStep }
  | { kind: 'send'; call: ToolStep; send: 'steer' | 'followUp' | 'mail' }
  | { kind: 'wait'; call: ToolStep }
  | { kind: 'kill'; call: ToolStep; stopping: boolean }
  | { kind: 'deliverable'; item: ChatDeliverable }
)
/** Every event of one job within one work block, at the position of its first event. */
export type JobStep = {
  kind: 'job'
  id: string
  job: string
  target: JobKind
  events: JobEvent[]
  /** A later work block of the same turn has events for this job too. */
  continues: boolean
}
export type GroupedStep = Exclude<Step, { kind: 'thinking' }> | ThoughtStep | ToolGroupStep | ReceiptStep
export type TranscriptStep = GroupedStep | JobStep

export const REDACTED_THINKING = 'Thinking is redacted.'

export function groupSteps(steps: Step[]): GroupedStep[] {
  const grouped: GroupedStep[] = []
  for (let index = 0; index < steps.length; index++) {
    const first = steps[index]
    if (first.kind === 'tool') {
      if (isReceiptTool(first)) {
        grouped.push({ kind: 'receipt', id: first.id, call: first })
        continue
      }
      const calls = [first]
      while (true) {
        const call = steps[index + 1]
        if (call?.kind !== 'tool' || call.name !== first.name || isReceiptTool(call)) break
        calls.push(call)
        index++
      }
      grouped.push(calls.length > 1 ? { kind: 'toolGroup', id: first.id, calls } : first)
    } else if (first.kind === 'thinking') {
      // Redaction is applied per segment so a hidden thought cannot hide its visible neighbors.
      const text = [first.redacted ? REDACTED_THINKING : first.text]
      let streaming = first.entryId === null
      while (true) {
        const thought = steps[index + 1]
        if (thought?.kind !== 'thinking') break
        index++
        text.push(thought.redacted ? REDACTED_THINKING : thought.text)
        if (thought.entryId === null) streaming = true
      }
      grouped.push({ kind: 'thinking', id: first.id, text: text.join('\n\n'), streaming })
    } else {
      grouped.push(first)
    }
  }
  return grouped
}

export type WorkSegment = { kind: 'work'; id: string; steps: TranscriptStep[] }
export type TranscriptSegment = TranscriptStep | WorkSegment

/** Work that folds: calls, thoughts, coordination receipts except `yield`, and job wakes. */
function foldable(step: GroupedStep): boolean {
  switch (step.kind) {
    case 'tool':
    case 'toolGroup':
    case 'thinking':
      return true
    case 'receipt':
      return step.call.name !== 'yield'
    case 'notice':
      return step.notice.kind === 'jobWake'
    default:
      return false
  }
}

/** Merge each job's receipts and deliverables into one `JobStep`; receipts that name no job stay rows. */
function foldJobs(segmentId: string, steps: GroupedStep[], jobs: ReadonlyMap<string, TranscriptJob>): TranscriptStep[] {
  const folded: TranscriptStep[] = []
  const byJob = new Map<string, JobStep>()
  function add(job: string, event?: JobEvent) {
    let step = byJob.get(job)
    if (!step) {
      step = { kind: 'job', id: `job:${segmentId}:${job}`, job, target: jobKind(job), events: [], continues: false }
      byJob.set(job, step)
      folded.push(step)
    }
    if (event) step.events.push(event)
  }
  function deliver(item: ChatDeliverable) {
    const { job, member } = resolveJob(item.jobId, jobs) ?? { job: item.jobId, member: null }
    add(job, { kind: 'deliverable', member, item })
  }
  for (const step of steps) {
    if (step.kind === 'notice' && step.notice.kind === 'jobWake') {
      for (const ref of step.notice.jobs) {
        const resolved = resolveJob(ref, jobs)
        if (resolved) add(resolved.job)
      }
      for (const item of step.notice.deliverables) deliver(item)
      continue
    }
    if (step.kind !== 'receipt') {
      folded.push(step)
      continue
    }
    const call = step.call
    let event: { job: string; event: JobEvent } | null = null
    if (call.name === 'spawn') {
      const spawned = spawnReceipt(call.details)
      if (spawned)
        event = {
          job: spawned.kind === 'child' ? spawned.job : spawned.team,
          event: { kind: 'spawn', member: null, call }
        }
    } else if (call.name === 'bash') {
      const job = parseBashDetails(call.details).job
      if (job) event = { job, event: { kind: 'spawn', member: null, call } }
    } else if (call.name === 'send') {
      const sent = sendReceipt(call.details)
      const resolved = sent && sent.kind !== 'note' ? resolveJob(sent.to, jobs) : null
      if (sent && sent.kind !== 'note' && resolved)
        event = { job: resolved.job, event: { kind: 'send', member: resolved.member, call, send: sent.kind } }
    } else if (call.name === 'kill') {
      const killed = killReceipt(call.details)
      const resolved = killed ? resolveJob(killed.id, jobs) : null
      if (killed && resolved)
        event = {
          job: resolved.job,
          event: { kind: 'kill', member: resolved.member, call, stopping: killed.stopping }
        }
    } else if (call.name === 'wait') {
      const waited = waitReceipt(call.details)
      const targets = (waited?.reason === 'results' ? waited.jobs : []).flatMap((ref) => resolveJob(ref, jobs) ?? [])
      const only = new Set(targets.map((target) => target.job)).size === 1
      const members = new Set(targets.map((target) => target.member))
      if (only) add(targets[0].job, { kind: 'wait', member: members.size === 1 ? targets[0].member : null, call })
      else folded.push(step)
      for (const item of call.result?.deliverables ?? []) deliver(item)
      continue
    }
    if (event) add(event.job, event.event)
    else folded.push(step)
  }
  return folded
}

/**
 * Fold contiguous work only. Visible messages stay in order and end the current fold, even while
 * streaming. Inside a fold, each job's coordination receipts and deliverables become one `JobStep`.
 */
export function segmentSteps(steps: GroupedStep[], jobs: ReadonlyMap<string, TranscriptJob>): TranscriptSegment[] {
  const runs: (GroupedStep | { id: string; steps: GroupedStep[] })[] = []
  for (const step of steps) {
    if (foldable(step)) {
      const previous = runs.at(-1)
      if (previous && 'steps' in previous) previous.steps.push(step)
      else runs.push({ id: `work:${step.id}`, steps: [step] })
    } else {
      runs.push(step)
    }
  }
  const segments = runs.map((run): TranscriptSegment =>
    'steps' in run ? { kind: 'work', id: run.id, steps: foldJobs(run.id, run.steps, jobs) } : run
  )
  const later = new Set<string>()
  for (let index = segments.length - 1; index >= 0; index--) {
    const segment = segments[index]
    if (segment.kind !== 'work') continue
    const here: string[] = []
    for (const step of segment.steps) {
      if (step.kind !== 'job') continue
      step.continues = later.has(step.job)
      here.push(step.job)
    }
    for (const job of here) later.add(job)
  }
  return segments
}
