import type { ToolStep } from '$lib/features/chat/turns'
import { integer, record } from './payload'

export interface WebSearchHit {
  title: string
  url: string
  snippet?: string
  publishedDate?: string
}

export interface WebSearchData {
  provider: string | null
  answer: string | null
  hits: WebSearchHit[]
  notes: string[]
  fallbackCount: number
}

export interface WebFetchData {
  provider: string | null
  url: string | null
  finalUrl: string | null
  title: string | null
  savedLocalUrl: string | null
}

export interface DocsSearchHit {
  crumb: string
  path: string
  docUrl: string | null
  startLine: number
  endLine: number
  webUrl: string | null
  snippet: string
}

export interface DocsTocItem {
  crumb: string
  path: string
  docUrl: string | null
  startLine: number
  endLine: number
}

export interface DocsListSource {
  key: string
  docPath: string
  sections: number
}

export type DocsSearchData =
  | { mode: 'list'; sources: DocsListSource[]; configured: string[] }
  | { mode: 'toc'; entries: DocsTocItem[] }
  | { mode: 'search'; loose: boolean; hits: DocsSearchHit[] }

function str(value: unknown): string | null {
  return typeof value === 'string' && value.length > 0 ? value : null
}

export function urlHost(raw: string): string {
  try {
    return new URL(raw).hostname.replace(/^www\./, '')
  } catch {
    return raw
  }
}

export function urlDisplay(raw: string): string {
  try {
    const u = new URL(raw)
    const host = u.host.replace(/^www\./, '')
    const path = u.pathname === '/' ? '' : u.pathname
    const full = `${host}${path}`
    return full.length > 64 ? `${full.slice(0, 61)}...` : full
  } catch {
    return raw
  }
}

export function parseWebSearch(call: ToolStep): WebSearchData | null {
  const details = record(call.details)
  const hitsRaw = Array.isArray(details.hits) ? details.hits : null
  const answer = str(details.answer)
  const provider = str(details.provider)
  if (!hitsRaw && !answer && !provider) return null
  const hits: WebSearchHit[] = (hitsRaw ?? [])
    .map((item) => record(item))
    .filter((item) => typeof item.url === 'string' && item.url.length > 0)
    .map((item) => ({
      title: str(item.title) ?? String(item.url),
      url: String(item.url),
      snippet: str(item.snippet) ?? undefined,
      publishedDate: str(item.publishedDate) ?? undefined
    }))
  const notes = Array.isArray(details.notes)
    ? details.notes.filter((n): n is string => typeof n === 'string' && n.length > 0)
    : []
  const fallbackCount = Array.isArray(details.fallbackFrom) ? details.fallbackFrom.length : 0
  return { provider, answer, hits, notes, fallbackCount }
}

export function parseWebFetch(call: ToolStep): WebFetchData | null {
  const details = record(call.details)
  const url = str(details.url)
  const provider = str(details.provider)
  if (!url && !provider) return null
  return {
    provider,
    url,
    finalUrl: str(details.finalUrl),
    title: str(details.title),
    savedLocalUrl: str(details.savedLocalUrl)
  }
}

export function parseSearchDocs(call: ToolStep): DocsSearchData | null {
  const details = record(call.details)
  const mode = str(details.mode)
  if (mode === 'list') {
    const sources = (Array.isArray(details.sources) ? details.sources : [])
      .map((item) => record(item))
      .filter((item) => typeof item.key === 'string')
      .map((item) => ({
        key: String(item.key),
        docPath: str(item.docPath) ?? String(item.key),
        sections: integer(item.sections) ? item.sections : 0
      }))
    const configured = Array.isArray(details.configured)
      ? details.configured.filter((s): s is string => typeof s === 'string')
      : []
    return { mode: 'list', sources, configured }
  }
  if (mode === 'toc') {
    const entries = (Array.isArray(details.entries) ? details.entries : [])
      .map((item) => record(item))
      .filter((item) => typeof item.path === 'string' && integer(item.startLine) && integer(item.endLine))
      .map((item) => ({
        crumb: str(item.crumb) ?? String(item.path),
        path: String(item.path),
        docUrl: str(item.docUrl),
        startLine: item.startLine as number,
        endLine: item.endLine as number
      }))
    return { mode: 'toc', entries }
  }
  if (mode === 'search') {
    const hits = (Array.isArray(details.hits) ? details.hits : [])
      .map((item) => record(item))
      .filter((item) => typeof item.path === 'string' && integer(item.startLine) && integer(item.endLine))
      .map((item) => ({
        crumb: str(item.crumb) ?? String(item.path),
        path: String(item.path),
        docUrl: str(item.docUrl),
        startLine: item.startLine as number,
        endLine: item.endLine as number,
        webUrl: str(item.webUrl),
        snippet: typeof item.snippet === 'string' ? item.snippet : ''
      }))
    return { mode: 'search', loose: details.loose === true, hits }
  }
  return null
}
