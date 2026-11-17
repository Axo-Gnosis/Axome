import { pathText, record } from '$lib/features/chat/elements/tool/payload'
import type { ChatEntry } from '$lib/types/backend'

/** A file axo's ledger recorded for a successful tool result; `index` is the result's position in `entries`. */
export interface LedgerFile {
  path: string
  access: 'read' | 'modify'
  index: number
}

/** Ledger records in transcript order. Paths are absolute host paths, `local://` files included. */
export function ledgerFiles(entries: ChatEntry[]): LedgerFile[] {
  const files: LedgerFile[] = []
  entries.forEach(({ body }, index) => {
    if (body.kind !== 'toolResult' || body.isError) return
    const ledger = record(record(body.details).ledger)
    if (!pathText(ledger.path) || typeof ledger.sha256 !== 'string') return
    files.push({ path: ledger.path, access: ledger.access === 'modify' ? 'modify' : 'read', index })
  })
  return files
}
