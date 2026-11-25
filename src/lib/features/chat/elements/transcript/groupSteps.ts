import type { ChatDeliverable } from '$lib/types/backend'
import { firstLine } from '../../format'
import { jobKind, resolveJob, type JobKind, type TranscriptJob } from '../../jobs'
import { parseBashDetails } from '../tool/bashOutput'
import { blockedToolResult } from '../tool/payload'
import { isReceiptTool, killReceipt, sendReceipt, spawnReceipt, waitReceipt } from '../tool/receipt'
import type { Step, ToolStep } from '../../turns'

/** One or more adjacent thinking steps, already redacted; `entryId` is the last one's message. */
export type ThoughtStep = { kind: 'thinking'; id: string; entryId: string | null; text: string; streaming: boolean }
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
/** Background work the host did between model calls; adjacent ones share one band. */
export type MaintenanceStep = Extract<Step, { kind: 'compaction' | 'branchSummary' | 'contextMoved' }>
export type BandStep = { kind: 'band'; id: string; steps: MaintenanceStep[] }
export type GroupedStep =
  Exclude<Step, { kind: 'thinking' } | MaintenanceStep> | ThoughtStep | ToolGroupStep | ReceiptStep | BandStep
export type TranscriptStep = GroupedStep | JobStep

export const REDACTED_THINKING = 'Thinking is redacted.'

// Ask documents and policy blocks stay visible at their original transcript position.
function standaloneTool(call: ToolStep): boolean {
  return call.name === 'ask' || blockedToolResult(call.result) !== null
}

function maintenance(step: Step | undefined): step is MaintenanceStep {
  return step?.kind === 'compaction' || step?.kind === 'branchSummary' || step?.kind === 'contextMoved'
}

export function groupSteps(steps: Step[]): GroupedStep[] {
  const grouped: GroupedStep[] = []
  for (let index = 0; index < steps.length; index++) {
    const first = steps[index]
    if (first.kind === 'tool') {
      if (standaloneTool(first)) {
        grouped.push(first)
        continue
      }
      // A receipt with file records needs ordinary tool rendering, including its output.
      if (isReceiptTool(first)) {
        grouped.push({ kind: 'receipt', id: first.id, call: first })
        continue
      }
      const calls = [first]
      while (true) {
        const call = steps[index + 1]
        if (call?.kind !== 'tool' || call.name !== first.name || standaloneTool(call) || isReceiptTool(call)) break
        calls.push(call)
        index++
      }
      grouped.push(calls.length > 1 ? { kind: 'toolGroup', id: first.id, calls } : first)
    } else if (first.kind === 'thinking') {
      // Redaction is applied per segment so a hidden thought cannot hide its visible neighbors.
      const text = [first.redacted ? REDACTED_THINKING : first.text]
      let streaming = first.entryId === null
      let entryId = first.entryId
      while (true) {
        const thought = steps[index + 1]
        if (thought?.kind !== 'thinking') break
        index++
        text.push(thought.redacted ? REDACTED_THINKING : thought.text)
        entryId = thought.entryId
        if (thought.entryId === null) streaming = true
      }
      grouped.push({ kind: 'thinking', id: first.id, entryId, text: text.join('\n\n'), streaming })
    } else if (maintenance(first)) {
      const band: BandStep = { kind: 'band', id: `band:${first.id}`, steps: [first] }
      while (maintenance(steps[index + 1])) band.steps.push(steps[++index] as MaintenanceStep)
      grouped.push(band)
    } else {
      grouped.push(first)
    }
  }
  return grouped
}

export type TextStep = Extract<GroupedStep, { kind: 'text' }>
/** One step of a turn: a run of work, titled by the text the agent wrote before it (`narration`). */
export type WorkSegment = { kind: 'work'; id: string; narration: TextStep | null; steps: TranscriptStep[] }
export type TranscriptSegment = TranscriptStep | WorkSegment

/** Work that folds: calls, thoughts, coordination receipts except `yield`, and job wakes. */
function foldable(step: GroupedStep): boolean {
  switch (step.kind) {
    case 'tool':
      return !standaloneTool(step)
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

/** A work run that did more than think. Only these take a narration, count as steps, and bound the reply. */
export function hasCalls(segment: { steps: { kind: string }[] }): boolean {
  return segment.steps.some((step) => step.kind !== 'thinking')
}

/** A narration's one-line step title: its first line without Markdown markers. */
export function narrationTitle(text: string): string {
  return firstLine(text)
    .replace(/^(#{1,6}|[-*+>])\s+/, '')
    .replace(/\*\*|`/g, '')
}

export function workCounts(steps: TranscriptStep[]): { calls: number; thoughts: number; failed: number } {
  const counts = { calls: 0, thoughts: 0, failed: 0 }
  for (const step of steps) {
    if (step.kind === 'thinking') counts.thoughts++
    const calls = step.kind === 'tool' ? [step] : step.kind === 'toolGroup' ? step.calls : []
    counts.calls += calls.length
    counts.failed += calls.filter((call) => call.status === 'error').length
  }
  return counts
}

/** The view id of a turn's summary fold. */
export function turnFoldId(turnId: string): string {
  return `turn-work:${turnId}`
}

/**
 * Split a turn into the work its summary folds and the reply that stays visible.
 * A finished turn's last text is always its answer, even when the agent meant it as a step title
 * (stopped, failed, or more calls after the answer): it leaves that step, which follows it untitled.
 * A finished turn with no text after its last step keeps that step in the reply.
 */
export function splitTurn(
  segments: TranscriptSegment[],
  running: boolean
): { work: TranscriptSegment[]; reply: TranscriptSegment[] } {
  const last = segments.findLastIndex((segment) => segment.kind === 'work' && hasCalls(segment))
  const after = segments.slice(last + 1)
  if (running || after.some((segment) => segment.kind === 'text'))
    return { work: segments.slice(0, last + 1), reply: after }
  const end = segments[last]
  if (end?.kind === 'work' && end.narration)
    return { work: segments.slice(0, last), reply: [end.narration, { ...end, narration: null }, ...after] }
  const start = Math.max(last, 0)
  return { work: segments.slice(0, start), reply: segments.slice(start) }
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
 * Fold contiguous work into steps. The text before a step that made calls becomes its `narration`;
 * other visible messages stay in order and end the current fold, even while streaming. Inside a fold,
 * each job's coordination receipts and deliverables become one `JobStep`.
 */
export function segmentSteps(steps: GroupedStep[], jobs: ReadonlyMap<string, TranscriptJob>): TranscriptSegment[] {
  type Run = { id: string; narration: TextStep | null; steps: GroupedStep[] }
  const isRun = (item: GroupedStep | Run | undefined): item is Run => !!item && !('kind' in item)
  const runs: (GroupedStep | Run)[] = []
  for (const step of steps) {
    if (foldable(step)) {
      const previous = runs.at(-1)
      if (isRun(previous)) previous.steps.push(step)
      else runs.push({ id: `work:${step.id}`, narration: null, steps: [step] })
    } else {
      runs.push(step)
    }
  }
  // The text before a run that made calls is that step's title, not a message.
  const titled: (GroupedStep | Run)[] = []
  for (const item of runs) {
    const text = titled.at(-1)
    if (isRun(item) && hasCalls(item) && text && !isRun(text) && text.kind === 'text') {
      titled.pop()
      item.narration = text
      // Thoughts the model wrote just before this narration belong to this step, not the one before.
      const before = titled.at(-1)
      if (isRun(before)) {
        while (true) {
          const last = before.steps.at(-1)
          if (last?.kind !== 'thinking' || last.entryId !== text.entryId) break
          item.steps.unshift(before.steps.pop()!)
        }
        if (!before.steps.length) titled.pop()
      }
    }
    titled.push(item)
  }
  const segments = titled.map((run): TranscriptSegment =>
    isRun(run) ? { kind: 'work', id: run.id, narration: run.narration, steps: foldJobs(run.id, run.steps, jobs) } : run
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
