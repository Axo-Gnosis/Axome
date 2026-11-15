import type { ToolStep } from '../../turns'
import type { ChatCommandExit, ChatContent } from '$lib/types/backend'
import { pathText, record } from './payload'

function parseExit(value: unknown): ChatCommandExit | null {
  const e = record(value)
  switch (e.type) {
    case 'exited':
      return typeof e.code === 'number' && Number.isFinite(e.code) ? { type: 'exited', code: e.code } : null
    case 'signaled':
      return typeof e.signal === 'number' && Number.isFinite(e.signal) ? { type: 'signaled', signal: e.signal } : null
    case 'timedOut':
      return typeof e.afterSecs === 'number' && Number.isFinite(e.afterSecs)
        ? { type: 'timedOut', afterSecs: e.afterSecs }
        : null
    case 'killed':
      return e.by === 'model' || e.by === 'host' ? { type: 'killed', by: e.by } : null
    case 'aborted':
      return { type: 'aborted' }
    case 'interrupted':
      return { type: 'interrupted' }
    default:
      return null
  }
}

export function parseBashDetails(details: unknown): {
  exit: ChatCommandExit | null
  durationMs: number | null
  fullOutput: string | null
  job: string | null
} {
  const d = record(details)
  const exit = parseExit(d.exit)
  const durationMs =
    typeof d.durationMs === 'number' && Number.isFinite(d.durationMs) && d.durationMs >= 0 ? d.durationMs : null
  const fullOutput =
    typeof d.fullOutput === 'string' &&
    pathText(d.fullOutput) &&
    d.fullOutput.startsWith('artifact://') &&
    d.fullOutput.length > 'artifact://'.length
      ? d.fullOutput
      : null
  const job = typeof d.job === 'string' && d.job.length > 0 ? d.job : null
  return { exit, durationMs, fullOutput, job }
}

export function formatCommandExit(exit: ChatCommandExit): { label: string; failed: boolean } {
  switch (exit.type) {
    case 'exited':
      return exit.code === 0 ? { label: 'Exited (0)', failed: false } : { label: `Exit ${exit.code}`, failed: true }
    case 'signaled':
      return { label: `Signal ${exit.signal}`, failed: true }
    case 'timedOut':
      return { label: `Timed out (${exit.afterSecs}s)`, failed: true }
    case 'killed':
      return { label: exit.by === 'host' ? 'Stopped' : 'Killed', failed: true }
    case 'aborted':
      return { label: 'Aborted', failed: true }
    case 'interrupted':
      return { label: 'Interrupted', failed: true }
  }
}

export function bashOutput(
  call: ToolStep,
  limit: number | null = null
): { parts: ChatContent[]; fullOutput: string | null; hidden: number } {
  const { fullOutput } = parseBashDetails(call.details)
  let parts = call.output
  let hidden = 0
  if (limit !== null) {
    const chunks = parts.map((part) => (part.type === 'text' && part.text ? part.text.split('\n') : []))
    const counts = chunks.map((lines) => lines.length - (lines.at(-1) === '' ? 1 : 0))
    hidden = Math.max(0, counts.reduce((sum, count) => sum + count, 0) - limit)
    let skip = hidden
    parts = parts.flatMap((part, index): ChatContent[] => {
      if (part.type === 'image') return [part]
      const from = Math.min(skip, counts[index])
      skip -= from
      return from < counts[index] ? [{ type: 'text', text: chunks[index].slice(from).join('\n') }] : []
    })
  }
  return { parts, fullOutput, hidden }
}
