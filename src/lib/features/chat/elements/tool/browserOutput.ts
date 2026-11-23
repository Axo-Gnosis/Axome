import { formatCount } from '$lib/features/chat/format'
import type { ToolStep } from '$lib/features/chat/turns'
import { record } from './payload'
import { urlDisplay } from './webOutput'

export interface BrowserConsoleError {
  level: string
  text: string
}

export interface BrowserFailedRequest {
  method: string
  url: string
  status: number | null
  failure: string | null
}

export interface BrowserOutputData {
  action: string
  steps: string[]
  tab: string
  url: string | null
  title: string | null
  consoleErrors: BrowserConsoleError[]
  failedRequests: BrowserFailedRequest[]
  spilledPath: string | null
  hasScreenshot: boolean
}

export function parseBrowserOutput(call: ToolStep): BrowserOutputData | null {
  const details = record(call.details)
  if (typeof details.action !== 'string' && !Array.isArray(details.steps)) return null

  const steps = Array.isArray(details.steps) ? details.steps.filter((s): s is string => typeof s === 'string') : []
  const consoleErrors: BrowserConsoleError[] = Array.isArray(details.consoleErrors)
    ? details.consoleErrors
        .map((item) => record(item))
        .filter((item) => typeof item.text === 'string')
        .map((item) => ({
          level: typeof item.level === 'string' ? item.level : 'error',
          text: String(item.text)
        }))
    : []
  const failedRequests: BrowserFailedRequest[] = Array.isArray(details.failedRequests)
    ? details.failedRequests
        .map((item) => record(item))
        .filter((item) => typeof item.url === 'string')
        .map((item) => ({
          method: typeof item.method === 'string' ? item.method : '',
          url: String(item.url),
          status: typeof item.status === 'number' ? item.status : null,
          failure: typeof item.failure === 'string' ? item.failure : null
        }))
    : []

  return {
    action: typeof details.action === 'string' ? details.action : (steps[0] ?? ''),
    steps,
    tab: typeof details.tab === 'string' && details.tab ? details.tab : 'main',
    url: typeof details.url === 'string' && details.url ? details.url : null,
    title: typeof details.title === 'string' && details.title ? details.title : null,
    consoleErrors,
    failedRequests,
    spilledPath: typeof details.spilledPath === 'string' && details.spilledPath ? details.spilledPath : null,
    hasScreenshot: details.hasScreenshot === true
  }
}

export function browserHeaderSummary(call: ToolStep): { summary: string; detail: string } {
  const args = record(call.arguments)
  const parsed = parseBrowserOutput(call)

  let summary = ''
  if (Array.isArray(args.steps) && args.steps.length > 1) {
    const names = args.steps.map((s) => record(s).action).filter((a): a is string => typeof a === 'string')
    summary = `${formatCount(args.steps.length, 'step')}${names.length ? ` (${names.join(' -> ')})` : ''}`
  } else {
    const step = Array.isArray(args.steps) && args.steps.length === 1 ? record(args.steps[0]) : args
    const action = typeof step.action === 'string' ? step.action : (parsed?.action ?? '')
    const url = typeof step.url === 'string' ? step.url : ''
    const target =
      typeof step.target === 'string' ? step.target : typeof step.selector === 'string' ? step.selector : ''
    if (url) summary = `${action} ${urlDisplay(url)}`
    else if (target) summary = `${action} ${target}`
    else summary = action
  }

  const detailParts: string[] = []
  if (parsed) {
    if (parsed.tab !== 'main') detailParts.push(parsed.tab)
    if (parsed.hasScreenshot) detailParts.push('screenshot')
    if (parsed.consoleErrors.length > 0) detailParts.push(formatCount(parsed.consoleErrors.length, 'console error'))
    if (parsed.failedRequests.length > 0) detailParts.push(formatCount(parsed.failedRequests.length, 'failed request'))
  }

  return { summary: summary.trim(), detail: detailParts.join(' · ') }
}
