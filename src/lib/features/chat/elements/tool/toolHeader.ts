import {
  BookOpen01Icon,
  FileCodeIcon,
  FileDiffIcon,
  FilePlusCornerIcon,
  FolderOpenIcon,
  Globe02Icon,
  Link01Icon,
  MessageSquareIcon,
  PackageIcon,
  Search01Icon,
  TerminalIcon
} from '@hugeicons/core-free-icons'
import type { IconData } from '$lib/icons/Icon.svelte'
import type { StatusTone } from '$lib/components/ui/status-dot'
import { formatCount, formatDuration } from '$lib/features/chat/format'
import { contentLines } from '$lib/features/chat/lineDiff'
import type { ToolStep } from '$lib/features/chat/turns'
import { isPathWithin, splitRemotePath } from '$lib/utils/paths'
import { formatCommandExit, parseBashDetails } from './bashOutput'
import { readOutput, readPath, readStartLine } from './readOutput'
import { fileChange, record } from './payload'
import { searchOutput } from './searchOutput'
import { parseSearchDocs, parseWebFetch, parseWebSearch, urlDisplay } from './webOutput'

export type ToolHeader = {
  name: string
  /** Rail verb: past tense when done, progressive while running; the raw name for unknown tools. */
  verb: string
  /** Whether the verb is a word rather than the tool's name. */
  known: boolean
  summary: string
  path: string | null
  line: number | undefined
  pathLabel: string
  command: string | null
  /** Edit line counts, shown as git's net line change. */
  lines: { added: number; removed: number } | null
  meta: string
  tone: StatusTone
  status: { label: string; tone: 'danger' } | null
}

const tools: Record<string, { verbs: [done: string, running: string]; icon: IconData; noun?: string }> = {
  bash: { verbs: ['Ran', 'Running'], icon: TerminalIcon, noun: 'command' },
  read: { verbs: ['Read', 'Reading'], icon: FileCodeIcon, noun: 'file' },
  edit: { verbs: ['Edited', 'Editing'], icon: FileDiffIcon, noun: 'file' },
  write: { verbs: ['Wrote', 'Writing'], icon: FilePlusCornerIcon, noun: 'file' },
  grep: { verbs: ['Searched', 'Searching'], icon: Search01Icon },
  find: { verbs: ['Found', 'Finding'], icon: FolderOpenIcon },
  search_docs: { verbs: ['Searched docs', 'Searching docs'], icon: BookOpen01Icon },
  web_search: { verbs: ['Searched the web', 'Searching the web'], icon: Globe02Icon },
  web_fetch: { verbs: ['Fetched', 'Fetching'], icon: Link01Icon, noun: 'page' },
  ask: { verbs: ['Asked', 'Asking'], icon: MessageSquareIcon }
}

export function toolIcon(name: string): IconData {
  return tools[name]?.icon ?? PackageIcon
}

/** The verb for a run of `count` calls to one tool: `Read 3 files`, `Searched 2 times`, `mcp_tool ×4`. */
export function groupVerb(name: string, count: number): string {
  const verb = tools[name]?.verbs?.[0]
  if (!verb) return `${name} ×${count}`
  return `${verb} ${formatCount(count, tools[name]?.noun ?? 'time')}`
}

/** Result rows that name a match (grep, skipping context rows) or a file (find); null when none were recognized. */
function searchCount(call: ToolStep): number | null {
  let count = 0
  for (const part of searchOutput(call).parts) {
    if (part.type !== 'text') continue
    for (const row of part.lines) {
      if (row.link && (call.name === 'find' || row.text[row.link.length] === ':')) count++
    }
  }
  return count || null
}

export function toolHeader(call: ToolStep, folderPath: string): ToolHeader {
  const args = record(call.arguments)
  const search = call.name === 'grep' || call.name === 'find'
  const path =
    call.name === 'read'
      ? readPath(call)
      : search
        ? typeof args.path === 'string' && args.path
          ? args.path
          : '.'
        : typeof args.path === 'string'
          ? args.path
          : null
  const root = path?.startsWith('axome://') ? folderPath : (splitRemotePath(folderPath)?.path ?? folderPath)
  const relative =
    path !== null && isPathWithin(root, path) ? path.slice(root.replace(/\/+$/, '').length + 1) || '.' : (path ?? '')
  let summary = ''
  let pathLabel = relative
  let command: string | null = null
  let detail = ''
  let lines: ToolHeader['lines'] = null
  const failed = call.status === 'error'
  let line: number | undefined
  let durationMs = call.result?.durationMs ?? null
  let tone: StatusTone = failed ? 'danger' : call.status === 'running' ? 'working' : 'success'
  let status: ToolHeader['status'] = failed ? { label: 'Failed', tone: 'danger' } : null
  switch (call.name) {
    case 'bash': {
      command = typeof args.command === 'string' ? args.command : ''
      const bash = parseBashDetails(call.details)
      durationMs = bash.durationMs ?? durationMs
      if (bash.exit !== null) {
        const exitInfo = formatCommandExit(bash.exit)
        if (exitInfo.failed) {
          tone = 'danger'
          status = { label: exitInfo.label, tone: 'danger' }
        }
      } else if (failed) {
        tone = 'danger'
        status = { label: 'Aborted', tone: 'danger' }
      }
      break
    }
    case 'read': {
      const source = readOutput(call)
      if (source?.endLine !== undefined) {
        line = source.startLine
        pathLabel += `:${line}${source.endLine !== line ? `-${source.endLine}` : ''}`
      } else if (!source) {
        line = readStartLine(args)
        if (line !== undefined && args.offset != null) pathLabel += `:${line}`
      }
      break
    }
    case 'edit':
    case 'write': {
      const diff = failed ? undefined : fileChange(call.result?.details)?.diff
      if (diff) {
        lines = { added: diff.additions, removed: diff.deletions }
        if (diff.firstChangedLine) line = diff.firstChangedLine
      }
      if (call.name === 'write' && typeof args.content === 'string')
        detail = `${contentLines(args.content).length} lines`
      break
    }
    case 'grep':
    case 'find': {
      summary = typeof args.pattern === 'string' ? JSON.stringify(args.pattern) : ''
      const count = call.status === 'done' ? searchCount(call) : null
      if (count !== null)
        detail = call.name === 'grep' ? `${count} ${count === 1 ? 'match' : 'matches'}` : formatCount(count, 'file')
      break
    }
    case 'web_search': {
      summary = typeof args.query === 'string' ? JSON.stringify(args.query) : ''
      const parsed = parseWebSearch(call)
      if (parsed) {
        const parts: string[] = []
        if (parsed.hits.length > 0) parts.push(formatCount(parsed.hits.length, 'result'))
        if (parsed.provider) parts.push(parsed.provider)
        if (parsed.fallbackCount > 0) parts.push('fallback')
        detail = parts.join(' · ')
      }
      break
    }
    case 'web_fetch': {
      const url = typeof args.url === 'string' ? args.url : ''
      summary = url ? urlDisplay(url) : ''
      const parsed = parseWebFetch(call)
      if (parsed) {
        const parts: string[] = []
        if (parsed.provider) parts.push(parsed.provider)
        if (parsed.savedLocalUrl) parts.push('saved')
        detail = parts.join(' · ')
      }
      break
    }
    case 'search_docs': {
      const q = typeof args.query === 'string' && args.query.trim() ? JSON.stringify(args.query.trim()) : ''
      const src = typeof args.source === 'string' && args.source.trim() ? args.source.trim() : ''
      summary = q && src ? `${q} in ${src}` : q || src || 'Cached sources'
      const parsed = parseSearchDocs(call)
      if (parsed) {
        if (parsed.mode === 'list') detail = formatCount(parsed.sources.length, 'source')
        else if (parsed.mode === 'toc') detail = formatCount(parsed.entries.length, 'section')
        else detail = formatCount(parsed.hits.length, 'section')
      }
      break
    }
    case 'ask': {
      const question = Array.isArray(args.questions) ? args.questions[0] : null
      summary =
        question !== null && typeof question === 'object' && typeof question.question === 'string'
          ? question.question
          : ''
      break
    }
    default:
      summary = JSON.stringify(call.arguments) ?? ''
  }
  const name = call.name || 'tool'
  const verb = tools[name]?.verbs
  return {
    name,
    verb: verb ? verb[call.status === 'running' ? 1 : 0] : name,
    known: verb !== undefined,
    summary: summary.replace(/[\r\n]+/g, ' '),
    path,
    line,
    pathLabel,
    command,
    lines,
    meta: [failed ? '' : detail, durationMs != null ? formatDuration(durationMs) : ''].filter(Boolean).join(' · '),
    tone,
    status
  }
}
