import type { StatusTone, ToneStatus } from '$lib/components/ui/status-dot'
import type { ChatJob, ChatJobStatus } from '$lib/types/backend'

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
