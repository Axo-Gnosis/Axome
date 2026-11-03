import type { Step, ToolStep } from '../../turns'

/** One or more adjacent thinking steps, already redacted, as the transcript renders them. */
export type ThoughtStep = { kind: 'thinking'; id: string; text: string; streaming: boolean }
export type ToolGroupStep = { kind: 'toolGroup'; id: string; calls: ToolStep[] }
export type TranscriptStep = Exclude<Step, { kind: 'thinking' }> | ThoughtStep | ToolGroupStep

export const REDACTED_THINKING = 'Thinking is redacted.'

export function groupSteps(steps: Step[]): TranscriptStep[] {
  const grouped: TranscriptStep[] = []
  for (let index = 0; index < steps.length; index++) {
    const first = steps[index]
    if (first.kind === 'tool') {
      const calls = [first]
      while (true) {
        const call = steps[index + 1]
        if (call?.kind !== 'tool' || call.name !== first.name) break
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

/** Fold contiguous work only. Visible messages stay in order and end the current fold, even while streaming. */
export function segmentSteps(steps: TranscriptStep[]): TranscriptSegment[] {
  const segments: TranscriptSegment[] = []
  for (const step of steps) {
    if (step.kind === 'tool' || step.kind === 'toolGroup' || step.kind === 'thinking') {
      const previous = segments.at(-1)
      if (previous?.kind === 'work') previous.steps.push(step)
      else segments.push({ kind: 'work', id: `work:${step.id}`, steps: [step] })
    } else {
      segments.push(step)
    }
  }
  return segments
}
