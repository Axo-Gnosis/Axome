import type { ToolStep } from '$lib/features/chat/turns'
import { record } from './payload'

export interface MemoryHit {
  rowType: string
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
    const fields = record(row)
    // Saved chats carry snake_case hits; axo-memory writes camelCase.
    const rowType = fields.rowType ?? fields.row_type
    const { title, category, snippet, url, line, links } = fields
    if (
      typeof rowType !== 'string' ||
      typeof title !== 'string' ||
      typeof snippet !== 'string' ||
      typeof url !== 'string'
    )
      continue
    hits.push({
      rowType,
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
  return { hits, notes: new Set(hits.map((hit) => hit.url)).size }
}
