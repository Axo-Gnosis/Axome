import type { StatusTone, ToneStatus } from '$lib/components/ui/status-dot'
import type { ChatCommandExit, ChatDeliverable, ChatJob, ChatJobStatus } from '$lib/types/backend'
import { parseBashDetails } from './elements/tool/bashOutput'
import { record } from './elements/tool/payload'
import { killReceipt, spawnReceipt } from './elements/tool/receipt'
import { firstLine } from './format'
import type { Turn } from './turns'

export function isActiveJob(status: ChatJobStatus): boolean {
  return status === 'queued' || status === 'running' || status === 'helpersRunning'
}

export function jobTone(status: ChatJobStatus): StatusTone {
  switch (status) {
    case 'queued':
    case 'running':
    case 'helpersRunning':
      return 'working'
    case 'finished':
      return 'success'
    case 'interrupted':
      return 'waiting'
    case 'failed':
      return 'danger'
  }
}

export function pickJob<T extends ChatJob>(jobs: T[], ref: string | null): T | null {
  return (
    jobs.find((job) => job.id === ref) ??
    (ref === null
      ? undefined
      : jobs.find(
          (job) => job.kind.kind === 'child' && (job.kind.team?.id === ref || job.kind.team?.member === ref)
        )) ??
    jobs.findLast((job) => isActiveJob(job.status)) ??
    jobs.at(-1) ??
    null
  )
}

export function jobsStatus(jobs: ChatJob[], kind: 'command' | 'child', failedText: string): ToneStatus | null {
  const matching = jobs.filter((job) => job.kind.kind === kind)
  const active = matching.filter((job) => isActiveJob(job.status)).length
  if (active > 0) return { tone: 'working', text: `${active} running` }
  if (matching.some((job) => job.status === 'failed')) return { tone: 'danger', text: failedText }
  return null
}

export type JobKind = 'child' | 'command' | 'team'
export type JobOutcome = 'done' | 'failed' | 'stopped'

/** What the transcript alone says about a job; `live.jobs` entries can disappear. */
export type TranscriptJob = {
  id: string
  kind: JobKind
  /** The turn that started it, or first mentioned it. */
  turnId: string
  /** First line of the task, or the command. */
  title: string
  agent: string | null
  /** Set on team member jobs. */
  team: { id: string; member: string } | null
  /** Team entries only. */
  members: [member: string, jobId: string][]
  startedAt: string | null
  endedAt: string | null
  exit: ChatCommandExit | null
  outcome: JobOutcome | null
}

export function jobKind(id: string): JobKind {
  return id.startsWith('bg_') ? 'command' : id.startsWith('team_') ? 'team' : 'child'
}

/** `agent_7/2` → `agent_7`: drops a wake's delivery counter. */
export function stripRef(ref: string): string {
  return ref.replace(/\/\d+$/, '')
}

export function isJobId(id: string): boolean {
  return /^(agent|bg|team)_/.test(id)
}

export function jobDurationMs(job: TranscriptJob | undefined, live: ChatJob | undefined, now: number): number | null {
  if (live && isActiveJob(live.status)) return Math.max(0, now - live.startedAt)
  if (live?.kind.kind === 'command' && live.kind.endedAt != null) return live.kind.endedAt - live.startedAt
  if (job?.startedAt && job.endedAt) return Date.parse(job.endedAt) - Date.parse(job.startedAt)
  return null
}

export function transcriptJobs(turns: Turn[]): Map<string, TranscriptJob> {
  const jobs = new Map<string, TranscriptJob>()
  function entry(id: string, kind: JobKind, turnId: string): TranscriptJob {
    let job = jobs.get(id)
    if (!job) {
      job = {
        id,
        kind,
        turnId,
        title: '',
        agent: null,
        team: null,
        members: [],
        startedAt: null,
        endedAt: null,
        exit: null,
        outcome: null
      }
      jobs.set(id, job)
    }
    return job
  }
  function deliver(item: ChatDeliverable, turnId: string, endedAt: string | null) {
    const job = entry(item.jobId, item.kind, turnId)
    if (item.kind === 'command') {
      job.title ||= item.command
      job.endedAt = endedAt
      job.exit = item.exit
      job.outcome = item.exit?.type === 'killed' ? 'stopped' : item.isError ? 'failed' : 'done'
    } else {
      job.team ??= item.team && { id: item.team.id, member: item.team.member }
      job.agent ??= item.agent
      if (item.note) return
      job.endedAt = endedAt
      job.outcome = item.isError ? 'failed' : item.partial ? 'stopped' : 'done'
    }
  }
  for (const turn of turns) {
    for (const step of turn.steps) {
      if (step.kind === 'notice') {
        if (step.notice.kind === 'jobWake')
          for (const item of step.notice.deliverables) deliver(item, turn.id, step.timestamp)
        continue
      }
      if (step.kind !== 'tool') continue
      const args = record(step.arguments)
      if (step.name === 'spawn') {
        const receipt = spawnReceipt(step.details)
        if (receipt?.kind === 'child') {
          Object.assign(entry(receipt.job, 'child', turn.id), {
            title: firstLine(args.task),
            agent: typeof args.agent === 'string' ? args.agent : null,
            startedAt: step.endedAt
          })
        } else if (receipt) {
          Object.assign(entry(receipt.team, 'team', turn.id), {
            title: typeof args.agent === 'string' ? args.agent : 'Team',
            members: receipt.members,
            startedAt: step.endedAt
          })
          const tasks = record(args.tasks)
          const roster = Array.isArray(args.members) ? args.members.map(record) : []
          for (const [member, id] of receipt.members) {
            const agent = roster.find((item) => item.name === member)?.agent
            Object.assign(entry(id, 'child', turn.id), {
              team: { id: receipt.team, member },
              title: firstLine(tasks[member] ?? args.task),
              agent: typeof agent === 'string' ? agent : null,
              startedAt: step.endedAt
            })
          }
        }
      } else if (step.name === 'bash') {
        const id = parseBashDetails(step.details).job
        if (id !== null)
          Object.assign(entry(id, 'command', turn.id), {
            title: typeof args.command === 'string' ? args.command : '',
            startedAt: step.endedAt
          })
      } else if (step.name === 'kill') {
        const receipt = killReceipt(step.details)
        if (receipt) {
          const id = stripRef(receipt.id)
          const job = entry(id, jobKind(id), turn.id)
          job.outcome ??= 'stopped'
        }
      }
      for (const item of step.result?.deliverables ?? []) deliver(item, turn.id, step.endedAt)
    }
  }
  return jobs
}

/** Which job a ref (`agent_7/2`, `bg_3`, `team_1`, or a teammate name) belongs to; team members resolve to their team. */
export function resolveJob(
  ref: string,
  jobs: ReadonlyMap<string, TranscriptJob>
): { job: string; member: string | null } | null {
  const id = stripRef(ref)
  const job = jobs.get(id)
  if (job?.team) return { job: job.team.id, member: job.team.member }
  if (job || isJobId(id)) return { job: id, member: null }
  const team = [...jobs.values()].findLast(
    (item) => item.kind === 'team' && item.members.some(([member]) => member === id)
  )
  return team ? { job: team.id, member: id } : null
}

const LIVE_OUTCOME: Partial<Record<ChatJobStatus, JobOutcome>> = {
  finished: 'done',
  failed: 'failed',
  interrupted: 'stopped'
}
const OUTCOME_STATE = {
  done: { tone: 'success', label: 'Done' },
  failed: { tone: 'danger', label: 'Failed' },
  stopped: { tone: 'idle', label: 'Stopped' }
} as const

export function jobState(
  job: TranscriptJob | undefined,
  live: ChatJob | undefined
): { tone: StatusTone; label: '' | 'Queued' | 'Running' | 'Done' | 'Failed' | 'Stopped' } {
  if (live && isActiveJob(live.status))
    return { tone: 'working', label: live.status === 'queued' ? 'Queued' : 'Running' }
  const outcome = job?.outcome ?? (live ? LIVE_OUTCOME[live.status] : undefined)
  return outcome ? OUTCOME_STATE[outcome] : { tone: 'idle', label: '' }
}
