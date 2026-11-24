import { parseMemorySearch } from '../elements/tool/memoryOutput'
import { memoryNoteCall } from '../elements/tool/toolHeader'
import type { Turn } from '../turns'

export type MemoryUse =
  | { kind: 'search'; stepId: string; query: string; notes: number | null }
  | { kind: 'note'; stepId: string; url: string; note: string; access: 'read' | 'edited' }

/** The chat's memory searches and note calls in transcript order; one row per note, `edited` once any call changed it. */
export function memoryUses(turns: Turn[]): MemoryUse[] {
  const uses: MemoryUse[] = []
  const notes = new Map<string, Extract<MemoryUse, { kind: 'note' }>>()
  for (const turn of turns) {
    for (const step of turn.steps) {
      if (step.kind !== 'tool') continue
      if (step.name === 'memory_search') {
        const query = (step.arguments as { query?: unknown } | null)?.query
        uses.push({
          kind: 'search',
          stepId: step.id,
          query: typeof query === 'string' ? query : '',
          notes: parseMemorySearch(step)?.notes ?? null
        })
        continue
      }
      const note = memoryNoteCall(step)
      if (!note) continue
      const access: 'read' | 'edited' = step.name === 'read' ? 'read' : 'edited'
      const known = notes.get(note.url)
      if (known) {
        if (access === 'edited') known.access = 'edited'
        continue
      }
      const use = { kind: 'note' as const, stepId: step.id, url: note.url, note: note.note, access }
      notes.set(note.url, use)
      uses.push(use)
    }
  }
  return uses
}
