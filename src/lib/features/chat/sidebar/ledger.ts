import { fileRecords } from '$lib/features/chat/elements/tool/payload'
import type { ChatEntry } from '$lib/types/backend'

/** A recorded file access; `index` is the tool result's position in `entries`. */
export interface LedgerFile {
  path: string
  access: 'read' | 'modify'
  index: number
  unfinished: boolean
}

/** Ledger records in transcript order. Paths are absolute host paths, `local://` files included. */
export function ledgerFiles(entries: ChatEntry[]): LedgerFile[] {
  const files: LedgerFile[] = []
  entries.forEach(({ body }, index) => {
    if (body.kind !== 'toolResult') return
    for (const { ledger } of fileRecords(body.details)) {
      files.push({ path: ledger.path, access: ledger.access, index, unfinished: body.isError })
    }
  })
  return files
}
