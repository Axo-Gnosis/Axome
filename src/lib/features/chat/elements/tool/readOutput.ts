import type { ChatContent } from '$lib/types/backend'
import type { DiffRow } from '$lib/features/chat/lineDiff'
import type { ToolStep } from '../../turns'
import { isSchemeUrl } from '$lib/features/workbench/links/parseLink'
import { fileRecords, integer, pathText, record } from './payload'

export type ReadSource = {
  /** `details.reads[i].path`, as axo shows it. */
  path: string
  /** The block's first line, verbatim. */
  header: string
  /** Numbered source rows (directory entries carry no number); gap rows between ranges and at outline folds. */
  rows: DiffRow[]
  /** The shown line ranges; empty for outlines and directories. */
  ranges: [number, number][]
  directory: boolean
  /** The verified footer, without its paragraph gap. */
  notice: string | null
  /** Blocks after this one up to the next source: images, nested context, per-path errors and notices. */
  attachments: ChatContent[]
}
export type ReadOutput = { leading: ChatContent[]; sources: ReadSource[] }

type ReadEntry =
  | { path: string; directory: true; entries: number; notice: string | null }
  | {
      path: string
      directory: false
      totalLines: number
      lines: [number, number][]
      outline: boolean
      truncated: boolean
      notice: string | null
    }

const RANGE = String.raw`(?:\d+(?:-\d*|\+\d+)?|-\d+)`
// Greedy base: the selector starts after the last ':', like axo's split_selector.
const SELECTOR = new RegExp(String.raw`^([\s\S]+):(${RANGE}(?:,${RANGE})*)$`)

/** Splits `path:selector`; null when the suffix is not a line selector (so the whole string is a path). */
export function readSelector(raw: string): { base: string; selector: string; line: number | undefined } | null {
  const match = SELECTOR.exec(raw)
  if (!match) return null
  const first = /^\d+/.exec(match[2])
  const line = first ? Number(first[0]) : NaN
  return { base: match[1], selector: match[2], line: Number.isSafeInteger(line) && line > 0 ? line : undefined }
}

export const renderRanges = (ranges: [number, number][]): string =>
  ranges.map(([a, b]) => (a === b ? `${a}` : `${a}-${b}`)).join(',')

/** The one requested path: a string, or a one-element array; null otherwise. */
export function readArgPath(args: Record<string, unknown>): string | null {
  const path = Array.isArray(args.path) && args.path.length === 1 ? args.path[0] : args.path
  return pathText(path) ? path : null
}

/** Session URLs stay as requested; plain paths use an unambiguous read record's normalization. */
export function readPath(call: ToolStep, records = fileRecords(call.result?.details)): string | null {
  const raw = readArgPath(record(call.arguments))
  if (raw === null) return null
  const base = readSelector(raw)?.base ?? raw
  if (isSchemeUrl(base)) return base
  if (records.length === 1 && records[0].ledger.access === 'read') return records[0].ledger.path
  return base
}

function readEntry(value: unknown): ReadEntry | null {
  const { path, directory, entries, totalLines, lines, outline, truncated, notice } = record(value)
  if (!pathText(path) || (notice != null && typeof notice !== 'string')) return null
  if (directory === true) return integer(entries) ? { path, directory: true, entries, notice: notice ?? null } : null
  if (!integer(totalLines) || typeof outline !== 'boolean' || typeof truncated !== 'boolean' || !Array.isArray(lines))
    return null
  const ranges: [number, number][] = []
  for (const range of lines) {
    if (!Array.isArray(range) || range.length !== 2) return null
    const [a, b] = range
    if (!integer(a) || !integer(b) || a < 1 || b < a) return null
    ranges.push([a, b])
  }
  return { path, directory: false, totalLines, lines: ranges, outline, truncated, notice: notice ?? null }
}

/** One read block checked against its entry; null on any mismatch. */
function parseSource(text: string, entry: ReadEntry): ReadSource | null {
  const { notice } = entry
  const suffix = notice === null ? '' : '\n\n' + notice
  if (!text.endsWith(suffix)) return null
  const content = suffix ? text.slice(0, -suffix.length) : text
  const newline = content.indexOf('\n')
  const header = newline < 0 ? content : content.slice(0, newline)
  const body = newline < 0 ? [] : content.slice(newline + 1).split('\n')
  const source = {
    path: entry.path,
    header,
    notice,
    attachments: [] as ChatContent[],
    directory: entry.directory
  }

  if (entry.directory) {
    const { entries } = entry
    if (header !== `${entry.path}/ (${entries === 1 ? '1 entry' : `${entries} entries`})`) return null
    if (notice === null ? body.length !== entries : body.length >= entries) return null
    const rows: DiffRow[] = body.map((text) => ({ kind: 'ctx', text, lineNumber: null }))
    return { ...source, rows, ranges: [] }
  }

  if ((notice !== null) !== (entry.truncated || entry.outline)) return null
  if (!header.startsWith(entry.path)) return null
  const counted = /^(?::[\d,+-]+)? \((\d+) lines?(, outline)?\)$/.exec(header.slice(entry.path.length))
  if (!counted || Number(counted[1]) !== entry.totalLines || (counted[2] !== undefined) !== entry.outline) return null
  if (entry.outline && entry.lines.length > 0) return null

  const rows: DiffRow[] = []
  let previous: number | null = null
  // Non-outline rows must be exactly the recorded ranges, in order, with gaps between ranges.
  // Outlines skip blank lines without a fold, so only their `… a-b` rows become gaps.
  let range = 0
  let expected = entry.lines[0]?.[0]
  for (const line of body) {
    const fold = entry.outline ? /^… ([1-9]\d*)-([1-9]\d*)$/.exec(line) : null
    if (fold) {
      const [start, end] = [Number(fold[1]), Number(fold[2])]
      if (!Number.isSafeInteger(end) || end < start || (previous !== null && start <= previous)) return null
      rows.push({ kind: 'gap' })
      previous = end
      continue
    }
    const numbered = /^([1-9]\d*)\t([\s\S]*)$/.exec(line)
    if (!numbered) return null
    const number = Number(numbered[1])
    if (!Number.isSafeInteger(number) || (previous !== null && number <= previous)) return null
    if (!entry.outline) {
      if (number !== expected) return null
      const [, end] = entry.lines[range]
      if (number === end) expected = entry.lines[++range]?.[0]
      else expected = number + 1
      if (previous !== null && number !== previous + 1) rows.push({ kind: 'gap' })
    }
    rows.push({ kind: 'ctx', text: numbered[2], lineNumber: number })
    previous = number
  }
  if (!entry.outline && range !== entry.lines.length) return null
  return { ...source, rows, ranges: entry.outline ? [] : entry.lines }
}

/** Sections driven by `details.reads`; null (raw output) whenever any block disagrees with its entry. */
export function readOutput(call: ToolStep): ReadOutput | null {
  if (call.name !== 'read' || !call.result || call.status === 'error') return null
  const reads = record(call.result.details).reads
  if (!Array.isArray(reads) || reads.length === 0) return null
  const entries: ReadEntry[] = []
  for (const value of reads) {
    const entry = readEntry(value)
    if (!entry) return null
    entries.push(entry)
  }
  const leading: ChatContent[] = []
  const sources: ReadSource[] = []
  for (const part of call.output) {
    const next = sources.length < entries.length ? entries[sources.length] : null
    const source = part.type === 'text' && next ? parseSource(part.text, next) : null
    if (source) sources.push(source)
    else (sources.at(-1)?.attachments ?? leading).push(part)
  }
  return sources.length === entries.length ? { leading, sources } : null
}
