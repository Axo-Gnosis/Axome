import type { ToolStep } from '../../turns'
import type { ChatContent } from '$lib/types/backend'
import { pathText, record } from './payload'
import { isSchemeUrl } from '$lib/features/workbench/links/parseLink'

export type SearchLine = { text: string; link?: { path: string; line?: number; length: number; matches?: number } }
type SearchPart = { type: 'text'; lines: SearchLine[]; suffix: string } | Extract<ChatContent, { type: 'image' }>
export type SearchOutputModel = { parts: SearchPart[]; hidden: number }

export type GrepMode = 'content' | 'files' | 'count'

export const grepMode = (args: Record<string, unknown>): GrepMode =>
  args.mode === 'files' || args.mode === 'count' ? args.mode : 'content'

/** grep headers: cwd-relative or absolute paths, or URLs; never `.`/`..` segments. */
const grepPath = (value: string): boolean =>
  pathText(value) &&
  (isSchemeUrl(value) ||
    !value
      .replace(/^\//, '')
      .split('/')
      .some((part) => part === '.' || part === '..' || part === ''))

const relativePath = (value: string): boolean => !value.startsWith('/') && grepPath(value)

const CUT_KEYS = ['matchLimitReached', 'resultLimitReached', 'cappedFiles', 'linesTruncated', 'truncation']

/** The text without axo's recorded notice; null when text and details disagree. */
function searchBody(text: string, details: Record<string, unknown>): string | null {
  const { notice } = details
  // A cut recorded without its notice (older transcripts) keeps the raw text unlinked.
  if (notice == null) return CUT_KEYS.some((key) => details[key] != null) ? null : text
  if (typeof notice !== 'string') return null
  const suffix = `\n\n${notice}`
  return text.endsWith(suffix) ? text.slice(0, -suffix.length) : null
}

/** Links rows in place: find paths, grep file headers and the `N:`/`N-` rows under them, `files` paths and `count` rows. */
function linkRows(rows: string[], kind: 'grep' | 'find', mode: GrepMode): SearchLine[] {
  let header: string | null = null
  let start = true
  return rows.map((text): SearchLine => {
    const line = text.endsWith('\n') ? text.slice(0, -1) : text
    if (kind === 'find') {
      if (line === 'No files found matching pattern') return { text }
      const path = line.endsWith('/') ? line.slice(0, -1) : line
      return relativePath(path) ? { text, link: { path: line, length: line.length } } : { text }
    }
    if (mode === 'files') return grepPath(line) ? { text, link: { path: line, length: line.length } } : { text }
    if (mode === 'count') {
      const counted = /^([\s\S]+):(\d+)$/.exec(line)
      return counted && grepPath(counted[1])
        ? { text, link: { path: counted[1], length: counted[1].length, matches: Number(counted[2]) } }
        : { text }
    }
    if (line === '') {
      header = null
      start = true
      return { text }
    }
    if (start) {
      start = false
      header = grepPath(line) ? line : null
      return header !== null ? { text, link: { path: header, length: line.length } } : { text }
    }
    const numbered = header !== null ? /^([1-9]\d*)([:-])/.exec(line) : null
    const number = numbered ? Number(numbered[1]) : NaN
    if (header === null || !numbered || !Number.isSafeInteger(number)) return { text }
    const link: NonNullable<SearchLine['link']> = { path: header, line: number, length: numbered[1].length }
    if (numbered[2] === ':') link.matches = 1
    return { text, link }
  })
}

/** Annotate original text in place: no sorting, regex execution, or discarded unknown rows. */
export function searchOutput(call: ToolStep, limit: number | null = null): SearchOutputModel {
  const args = record(call.arguments)
  const kind = call.name === 'grep' || call.name === 'find' ? call.name : null
  const canLink =
    kind !== null &&
    call.status === 'done' &&
    typeof args.pattern === 'string' &&
    (args.path == null ||
      args.path === '' ||
      pathText(args.path) ||
      (Array.isArray(args.path) && args.path.every(pathText))) &&
    call.output.length === 1
  const mode = grepMode(args)
  let remaining = limit ?? Infinity
  let hidden = 0
  const parts: SearchPart[] = call.output.map((part): SearchPart => {
    if (part.type === 'image') return part
    const body = canLink && kind ? searchBody(part.text, record(call.result?.details)) : null
    const source = body ?? part.text
    const rows = source.match(/[^\n]*\n|[^\n]+$/g) ?? []
    const linked =
      body !== null && kind && !(kind === 'grep' && body === 'No matches found')
        ? linkRows(rows, kind, mode)
        : rows.map((text) => ({ text }))
    const shown = linked.slice(0, remaining)
    remaining = Math.max(0, remaining - shown.length)
    hidden += rows.length - shown.length
    let suffix = body !== null ? part.text.slice(body.length) : ''
    // The preview ends at this row, so retain the footer's original paragraph gap without an extra newline.
    if (suffix && shown.length < rows.length && shown.at(-1)?.text.endsWith('\n')) suffix = suffix.slice(1)
    return { type: 'text', lines: shown, suffix }
  })
  return { parts, hidden }
}
