import type { Turn } from '$lib/features/chat/turns'
import { partsText } from '$lib/features/chat/parts/parts'

export interface TurnRailPreview {
  user: string
  agent: string | null
  time: string | null
}

function userLabel(turn: Turn): string {
  if (turn.user) {
    const prompt = partsText(turn.user.body.parts).replace(/\s+/g, ' ').trim()
    if (prompt) return prompt
  }
  for (const step of turn.steps) {
    if (step.kind === 'notice') {
      if (step.notice.kind === 'mail') return `Message from ${step.notice.sender}`
      if (step.notice.kind === 'jobWake') {
        return step.notice.jobs.length ? `Background work: ${step.notice.jobs.join(', ')}` : 'Background job finished'
      }
      const text = step.notice.text.replace(/\s+/g, ' ').trim()
      if (text) return text
    }
    if (step.kind === 'compaction') return 'Compaction summary'
    if (step.kind === 'branchSummary') return 'Branch summary'
  }
  return 'Turn'
}

/** Final assistant text for `turn`, or `null` when the agent produced no text output. */
function finalAgentText(turn: Turn): string | null {
  if (turn.running) {
    const liveText = turn.steps
      .filter(
        (step): step is Extract<Turn['steps'][number], { kind: 'text' }> =>
          step.kind === 'text' && step.entryId === null
      )
      .map((step) => step.text)
      .join(' ')
      .replace(/\s+/g, ' ')
      .trim()
    if (liveText) return liveText
  }

  for (let i = turn.assistants.length - 1; i >= 0; i--) {
    const text = turn.assistants[i].body.blocks
      .flatMap((block) => (block.type === 'text' ? [block.text] : []))
      .join(' ')
      .replace(/\s+/g, ' ')
      .trim()
    if (text) return text
  }
  return null
}

function formatTurnTime(iso: string | null): string | null {
  if (!iso) return null
  const date = new Date(iso)
  if (!Number.isFinite(date.getTime())) return null
  return date.toLocaleTimeString([], { hour: 'numeric', minute: '2-digit' })
}

export function turnPreview(turn: Turn): TurnRailPreview {
  return {
    user: userLabel(turn),
    agent: finalAgentText(turn),
    time: formatTurnTime(turn.startedAt)
  }
}
