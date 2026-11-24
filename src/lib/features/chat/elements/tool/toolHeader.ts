import {
  BookOpen01Icon,
  BrainIcon,
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
import { parseExtensionOrigin, type ExtensionOrigin } from '$lib/features/chat/extensions'
import { formatCount, formatDuration } from '$lib/features/chat/format'
import { contentLines } from '$lib/features/chat/lineDiff'
import type { ToolStep } from '$lib/features/chat/turns'
import { memoryNote } from '$lib/features/memory/notes'
import { parseMemorySearch } from './memoryOutput'
import { parseAskBlocks } from '../ask/askDocument'
import { formatCommandExit, parseBashDetails } from './bashOutput'
import { readOutput, readPath, readStartLine } from './readOutput'
import { blockedToolResult, displayFilePath, fileRecords, record, type ToolFileRecord } from './payload'
import { searchOutput } from './searchOutput'
import { browserHeaderSummary } from './browserOutput'
import { parseSearchDocs, parseWebFetch, parseWebSearch, urlDisplay } from './webOutput'

export type ToolHeader = {
  name: string
  icon: IconData
  /** Rail verb: past tense when done, progressive while running; the raw name for unknown tools. */
  verb: string
  /** Whether the verb is a word rather than the tool's name. */
  known: boolean
  summary: string
  path: string | null
  line: number | undefined
  pathLabel: string
  command: string | null
  /** Total additions and deletions across every recorded change, if all are known. */
  lines: { added: number; removed: number } | null
  meta: string
  tone: StatusTone
  status: { label: string; tone: 'danger' } | null
  unfinished: string | null
  blocked: { extension: ExtensionOrigin | null; reason: string } | null
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
  browser: { verbs: ['Browsed', 'Browsing'], icon: Globe02Icon },
  memory_search: { verbs: ['Searched memory', 'Searching memory'], icon: BrainIcon },
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

/** The one file a call is about; null for calls over several files or none. */
function callPath(call: ToolStep, records: ToolFileRecord[]): string | null {
  if (records.length > 1) return null
  if (call.name === 'read') return readPath(call, records)
  if (records.length === 1) return records[0].change?.url ?? records[0].ledger.path
  const path = record(call.arguments).path
  const search = call.name === 'grep' || call.name === 'find'
  return typeof path === 'string' && (!search || path) ? path : search ? '.' : null
}

function noteAt(call: ToolStep, path: string | null) {
  return path !== null && ['read', 'edit', 'write'].includes(call.name) ? memoryNote(path) : null
}

/** The memory note a read, edit, or write call is about; null for any other call. */
export function memoryNoteCall(
  call: ToolStep,
  records: ToolFileRecord[] = fileRecords(call.result?.details)
): { url: string; bank: string; note: string } | null {
  const url = callPath(call, records)
  const note = noteAt(call, url)
  return url !== null && note ? { url, ...note } : null
}

export function toolHeader(
  call: ToolStep,
  folderPath: string,
  records: ToolFileRecord[] = fileRecords(call.result?.details)
): ToolHeader {
  const args = record(call.arguments)
  const single = records.length === 1 ? records[0] : null
  const multi = records.length > 1
  const path = callPath(call, records)
  const note = noteAt(call, path)
  let summary = ''
  let pathLabel =
    note?.note ?? (path === null ? '' : path === single?.change?.url ? path : displayFilePath(path, folderPath))
  let command: string | null = null
  let detail = ''
  let lines: ToolHeader['lines'] = null
  let hasChange = false
  let allCounts = true
  let added = 0
  let removed = 0
  for (const { change } of records) {
    if (!change) continue
    hasChange = true
    if (!change.diff) {
      allCounts = false
      continue
    }
    added += change.diff.additions
    removed += change.diff.deletions
  }
  if (hasChange && allCounts) lines = { added, removed }
  const failed = call.status === 'error'
  const marker = blockedToolResult(call.result)
  const unfinished = call.result?.isError === true && records.length > 0
  const firstOutput = call.output[0]
  const stoppedEdit =
    unfinished &&
    !marker &&
    hasChange &&
    call.name === 'edit' &&
    call.output.length === 1 &&
    firstOutput?.type === 'text' &&
    firstOutput.text === 'Operation aborted'
  let line = single?.change?.diff?.firstChangedLine
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
      const source = readOutput(call, records)
      if (path !== null && source?.endLine !== undefined) {
        line = source.startLine
        pathLabel += `:${line}${source.endLine !== line ? `-${source.endLine}` : ''}`
      } else if (path !== null && !source && line === undefined) {
        line = readStartLine(args)
        if (line !== undefined && args.offset != null) pathLabel += `:${line}`
      }
      break
    }
    case 'edit':
    case 'write': {
      if (call.name === 'write' && !multi && typeof args.content === 'string')
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
    case 'memory_search': {
      summary = typeof args.query === 'string' ? JSON.stringify(args.query) : ''
      const parsed = call.status === 'done' ? parseMemorySearch(call) : null
      if (parsed) detail = formatCount(parsed.notes, 'note')
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
    case 'browser': {
      const info = browserHeaderSummary(call)
      summary = info.summary
      detail = info.detail
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
      const question = parseAskBlocks(call.arguments)?.find((block) => block.type !== 'markdown')
      summary = question?.question ?? ''
      break
    }
    default:
      if (!multi) summary = JSON.stringify(call.arguments) ?? ''
  }
  const name = call.name || 'tool'
  const verb = tools[name]?.verbs?.[call.status === 'running' ? 1 : 0]
  if (multi) summary = [`${records.length} files`, summary].filter(Boolean).join(' · ')
  if (marker && (!verb || (!summary && path === null && !command))) {
    summary = summary ? `${name} · ${summary}` : name
  }
  let unfinishedMessage: string | null = null
  if (stoppedEdit) {
    const paths = new Map<string, string>()
    for (const { change } of records) {
      if (change) paths.set(change.key, change.url ?? displayFilePath(change.path, folderPath))
    }
    const savedPath = paths.values().next().value
    unfinishedMessage =
      paths.size === 1 && savedPath !== undefined
        ? `You stopped this edit after it had saved the file. The change below is in ${savedPath}.`
        : 'You stopped this edit after it had saved files. The changes below are in those files.'
    status = null
  } else if (unfinished) {
    unfinishedMessage = hasChange
      ? 'This call did not finish. The recorded file changes below were saved before it ended.'
      : 'This call did not finish. Its recorded file accesses are shown below.'
  }
  const reason = marker ? call.output.flatMap((part) => (part.type === 'text' ? [part.text] : [])).join('\n') : ''
  return {
    name,
    icon: note ? BrainIcon : toolIcon(name),
    verb: marker ? 'Blocked' : stoppedEdit ? 'Edit stopped' : note ? `${verb} memory note` : (verb ?? name),
    known: marker !== null || verb !== undefined,
    summary: summary.replace(/[\r\n]+/g, ' '),
    path,
    line,
    pathLabel,
    command,
    lines,
    meta: [
      stoppedEdit ? 'saved before it stopped' : failed ? '' : detail,
      durationMs != null ? formatDuration(durationMs) : ''
    ]
      .filter(Boolean)
      .join(' · '),
    tone: marker ? 'danger' : tone,
    status: marker ? null : status,
    unfinished: unfinishedMessage,
    blocked: marker
      ? {
          extension: parseExtensionOrigin(marker.origin),
          reason: reason.trim() ? reason : 'Tool execution was blocked'
        }
      : null
  }
}
