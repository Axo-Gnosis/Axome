import { patchRows, type DiffResult } from '$lib/features/chat/lineDiff'
import { isPathWithin, splitRemotePath } from '$lib/utils/paths'
import type { ToolStep } from '../../turns'

export const record = (value: unknown): Record<string, unknown> =>
  value !== null && typeof value === 'object' && !Array.isArray(value) ? (value as Record<string, unknown>) : {}

export const integer = (value: unknown): value is number =>
  typeof value === 'number' && Number.isSafeInteger(value) && value >= 0

const encoder = new TextEncoder()
export const pathText = (value: unknown): value is string =>
  typeof value === 'string' && value.length > 0 && !/[\u0000-\u001f\u007f]/.test(value)

type BlockedCall = { origin: string | null }

export function blockedCall(details: unknown): BlockedCall | null {
  const marker = record(details).blocked
  if (marker === null || typeof marker !== 'object' || Array.isArray(marker)) return null
  const { origin } = record(marker)
  return origin == null || typeof origin === 'string' ? { origin: origin ?? null } : null
}

export function blockedToolResult(result: ToolStep['result']): BlockedCall | null {
  return result?.isError === true ? blockedCall(result.details) : null
}

export type Truncation = Record<string, unknown> & { outputLines: number; outputBytes: number }

/** axo's truncation record: the fields every capped tool output shares; null when any disagrees. */
export function truncation(value: unknown): Truncation | null {
  const cut = record(value)
  return cut.truncated === true &&
    integer(cut.outputLines) &&
    integer(cut.outputBytes) &&
    integer(cut.totalLines) &&
    integer(cut.totalBytes) &&
    cut.totalLines >= cut.outputLines &&
    cut.totalBytes >= cut.outputBytes &&
    cut.maxBytes === 51200 &&
    cut.outputBytes <= 51200
    ? (cut as Truncation)
    : null
}

/** Exact bytes and lines prove that stripping a footer cannot consume recorded output. */
export const coversBody = (cut: Truncation, body: string): boolean =>
  encoder.encode(body).length === cut.outputBytes && (body === '' ? 0 : body.split('\n').length) === cut.outputLines

interface ToolSnapshotBlob {
  sha256: string
  bytes: number
  binary: boolean
  omitted?: 'tooLarge' | 'ioError'
}

interface ToolLineDiff {
  additions: number
  deletions: number
  firstChangedLine?: number
  patch?: string
  patchTruncated?: boolean
}

export interface ToolFileChange {
  path: string
  key: string
  url?: string
  created: boolean
  before?: ToolSnapshotBlob
  after: ToolSnapshotBlob
  diff?: ToolLineDiff
  raw: Record<string, unknown>
}

export interface ToolFileRecord {
  ledger: { path: string; sha256: string; access: 'read' | 'modify' }
  change: ToolFileChange | null
}

export function displayFilePath(path: string, folderPath: string): string {
  const root = path.startsWith('axome://') ? folderPath : (splitRemotePath(folderPath)?.path ?? folderPath)
  return isPathWithin(root, path) ? path.slice(root.replace(/\/+$/, '').length + 1) || '.' : path
}

export function fileRecords(details: unknown): ToolFileRecord[] {
  const files = record(details).files
  if (!Array.isArray(files)) return []
  const records: ToolFileRecord[] = []
  for (const file of files) {
    const { ledger, change } = record(file)
    const { path, sha256, access } = record(ledger)
    if (!pathText(path) || typeof sha256 !== 'string' || (access !== 'read' && access !== 'modify')) continue
    const parsed = parseFileChange(change)
    records.push({ ledger: { path, sha256, access }, change: parsed?.path === path ? parsed : null })
  }
  return records
}

function snapshotBlob(value: unknown): ToolSnapshotBlob | null {
  const fields = record(value)
  if (typeof fields.sha256 !== 'string' || typeof fields.binary !== 'boolean' || !integer(fields.bytes)) return null
  const omitted = fields.omitted === 'tooLarge' || fields.omitted === 'ioError' ? fields.omitted : undefined
  return { sha256: fields.sha256, bytes: fields.bytes, binary: fields.binary, omitted }
}

function lineDiff(value: unknown): ToolLineDiff | undefined {
  const fields = record(value)
  if (!integer(fields.additions) || !integer(fields.deletions)) return undefined
  return {
    additions: fields.additions,
    deletions: fields.deletions,
    firstChangedLine:
      integer(fields.firstChangedLine) && fields.firstChangedLine > 0 ? fields.firstChangedLine : undefined,
    patch: typeof fields.patch === 'string' ? fields.patch : undefined,
    patchTruncated: fields.patchTruncated === true
  }
}

function parseFileChange(value: unknown): ToolFileChange | null {
  const raw = record(value)
  if (!pathText(raw.path) || typeof raw.key !== 'string' || !raw.key || typeof raw.created !== 'boolean') {
    return null
  }
  const after = snapshotBlob(raw.after)
  if (!after) return null
  const before = raw.created ? undefined : (snapshotBlob(raw.before) ?? undefined)
  if (!raw.created && !before) return null
  return {
    path: raw.path,
    key: raw.key,
    url: typeof raw.url === 'string' && raw.url ? raw.url : undefined,
    created: raw.created,
    before,
    after,
    diff: raw.diff != null ? lineDiff(raw.diff) : undefined,
    raw
  }
}

/** Unified patch rows from a parsed file change; empty when unchanged, null when absent or truncated. */
export function editDiff(change: ToolFileChange | null): DiffResult | null {
  const diff = change?.diff
  if (!diff || diff.patchTruncated) return null
  if (typeof diff.patch === 'string') {
    const parsed = patchRows(diff.patch)
    return parsed.added === diff.additions && parsed.removed === diff.deletions ? parsed : null
  }
  return !change.created && diff.additions === 0 && diff.deletions === 0 ? { rows: [], added: 0, removed: 0 } : null
}
