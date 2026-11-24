import type { ChatContent } from '$lib/types/backend'
import type { ToolStep } from '../../turns'
import { isSchemeUrl } from '$lib/features/workbench/links/parseLink'
import { coversBody, fileRecords, pathText, record, truncation } from './payload'

export type ReadOutput = {
  path: string
  body: string
  startLine: number
  endLine: number | undefined
  lineCount: number
  notice: string | null
  attachments: ChatContent[]
}

function numericArg(args: Record<string, unknown>, name: string, fallback: number): number | null {
  const value = args[name]
  return value == null ? fallback : typeof value === 'number' && Number.isFinite(value) ? value : null
}

export function readStartLine(arguments_: unknown): number | undefined {
  const offset = numericArg(record(arguments_), 'offset', 1)
  if (offset === null) return undefined
  const start = Math.floor(Math.max(0, offset - 1)) + 1
  return Number.isSafeInteger(start) ? start : undefined
}

/** Preserve session-owned URLs; plain paths use an unambiguous read record's normalization. */
export function readPath(call: ToolStep, records = fileRecords(call.result?.details)): string | null {
  const path = record(call.arguments).path
  if (pathText(path) && isSchemeUrl(path)) return path
  if (records.length === 1 && records[0].ledger.access === 'read') return records[0].ledger.path
  return pathText(path) ? path : null
}

/** Return captured source, never a range inferred from the requested limit or attached context. */
export function readOutput(call: ToolStep, records = fileRecords(call.result?.details)): ReadOutput | null {
  if (call.name !== 'read' || !call.result || call.status === 'error') return null
  const args = record(call.arguments)
  const details = record(call.result.details)
  const hasReadRecord = records.length === 1 && records[0].ledger.access === 'read'
  if (records.length > 0 && !hasReadRecord) return null
  const path = readPath(call, records)
  if (!path) return null
  const first = call.output[0]
  if (first?.type !== 'text') return null
  const start = readStartLine(args)
  const limit = numericArg(args, 'limit', Infinity)
  if (start === undefined || limit === null) return null
  const count = Math.floor(Math.max(0, limit))
  if (count !== Infinity && !Number.isSafeInteger(count)) return null
  const text = first.text
  // These describe bytes or attachments, not source lines. Preserve the original response.
  if (
    call.output[1]?.type === 'image' ||
    text.startsWith('Read image file [') ||
    text.startsWith(`Binary file: ${args.path} (`) ||
    text.startsWith(`Binary file: ${path} (`) ||
    /^\[Line \d+ is [\d.]+(?:B|KB|MB), exceeds 50\.0KB limit\./.test(text)
  )
    return null
  let body = text
  let notice: string | null = null
  let returned: number
  if (details.truncation != null) {
    const cut = truncation(details.truncation)
    if (
      !hasReadRecord ||
      !cut ||
      (cut.truncatedBy !== 'lines' && cut.truncatedBy !== 'bytes') ||
      cut.outputLines < 1 ||
      cut.outputLines > 2000 ||
      cut.maxLines !== 2000 ||
      cut.firstLineExceedsLimit !== false ||
      (cut.truncatedBy === 'lines' && cut.outputLines !== 2000)
    )
      return null
    const end = start + cut.outputLines - 1
    const footer = text.match(
      /\n\n(\[Showing lines ([1-9]\d*)-([1-9]\d*) of ([1-9]\d*)( \(50\.0KB limit\))?\. Use offset=([1-9]\d*) to continue\.\])$/
    )
    if (
      !footer ||
      Number(footer[2]) !== start ||
      Number(footer[3]) !== end ||
      Number(footer[4]) <= end ||
      Number(footer[6]) !== end + 1 ||
      Boolean(footer[5]) !== (cut.truncatedBy === 'bytes')
    )
      return null
    body = text.slice(0, footer.index)
    // Exact bytes and lines prove that stripping the notice cannot consume source.
    if (!coversBody(cut, body) || cut.outputLines > count) return null
    notice = footer[1]
    returned = cut.outputLines
  } else {
    const footer = text.match(/\n\n(\[([1-9]\d*) more lines in file\. Use offset=([1-9]\d*) to continue\.\])$/)
    if (footer) {
      const source = text.slice(0, footer.index)
      // User limits lack truncation metadata: require the ledger and exact count/continuation.
      if (
        !hasReadRecord ||
        count === Infinity ||
        Number(footer[3]) !== start + count ||
        (source === '' ? count > 1 : source.split('\n').length !== count)
      )
        return null
      body = source
      notice = footer[1]
      returned = count
    } else {
      if (/\n\n\[Showing lines \d+-\d+ of \d+.*\]$/.test(body)) return null
      returned = body === '' ? 0 : body.split('\n').length
      if (returned > count) return null
    }
  }
  if (returned > 0 && !Number.isSafeInteger(start + returned - 1)) return null
  return {
    path,
    body,
    startLine: start,
    endLine: returned > 0 ? start + returned - 1 : undefined,
    lineCount: returned,
    notice,
    attachments: call.output.slice(1)
  }
}
