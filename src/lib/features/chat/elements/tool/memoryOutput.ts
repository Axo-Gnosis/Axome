import type { ToolStep } from '$lib/features/chat/turns'
import { record } from './payload'

export interface MemoryHit {
  rowType: string
  entityId: number
  title: string
  category: string | null
  snippet: string
  url: string
  line: number | null
  links: { url: string }[]
}

export function parseMemorySearch(call: ToolStep): { hits: MemoryHit[]; notes: number } | null {
  const rows = record(call.details).hits
  if (!Array.isArray(rows)) return null
  const hits: MemoryHit[] = []
  for (const row of rows) {
    const { row_type, entity_id, title, category, snippet, url, line, links } = record(row)
    if (
      typeof row_type !== 'string' ||
      typeof entity_id !== 'number' ||
      typeof title !== 'string' ||
      typeof snippet !== 'string' ||
      typeof url !== 'string'
    )
      continue
    hits.push({
      rowType: row_type,
      entityId: entity_id,
      title,
      category: typeof category === 'string' ? category : null,
      snippet,
      url,
      line: typeof line === 'number' ? line : null,
      links: Array.isArray(links)
        ? links.flatMap((link) => {
            const { url } = record(link)
            return typeof url === 'string' ? [{ url }] : []
          })
        : []
    })
  }
  return { hits, notes: new Set(hits.map((hit) => hit.entityId)).size }
}
