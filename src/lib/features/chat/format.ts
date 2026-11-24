import type { ChatRetry, ChatStats, ChatUsage } from '$lib/types/backend'

export function firstLine(text: unknown): string {
  return typeof text === 'string' ? text.trim().split('\n', 1)[0] : ''
}

export function formatTokens(n: number): string {
  if (n < 1000) return String(n)
  if (n < 1e6) return `${(n / 1000).toFixed(1).replace(/\.0$/, '')}k`
  return `${(n / 1e6).toFixed(1).replace(/\.0$/, '')}M`
}

export function formatCost(usd: number | null): string {
  return usd === null ? 'unknown' : `$${usd.toFixed(2)}`
}

export function formatDuration(ms: number): string {
  if (ms < 1000) return `${Math.round(ms)}ms`
  if (ms < 60_000) return `${(ms / 1000).toFixed(1).replace(/\.0$/, '')}s`
  const seconds = Math.floor(ms / 1000)
  return `${Math.floor(seconds / 60)}m${seconds % 60}s`
}

/** "Retrying 2/5 · in 4s": the retry row of a running turn or standalone maintenance. */
export function formatRetry(retry: ChatRetry): string {
  return `Retrying ${retry.attempt}/${retry.maxRetries} · in ${formatDuration(retry.delayMs)}`
}

export function formatCount(count: number, noun: string): string {
  return `${count} ${noun}${count === 1 ? '' : 's'}`
}

const listFormat = new Intl.ListFormat('en', { type: 'conjunction' })

/** "a", "a and b", "a, b, and c". */
export function formatList(items: string[]): string {
  return listFormat.format(items)
}

// "anthropic/claude-opus-5-5" → "claude-opus-5-5"; a trailing build date goes too.
export function modelName(model: string): string {
  return model.slice(model.lastIndexOf('/') + 1).replace(/-\d{8}$/, '')
}

/** Share of input served from cache, formatted to one decimal; null when nothing was read from cache. */
export function cacheHitPercent(usage: Pick<ChatUsage, 'input' | 'cacheRead' | 'cacheWrite'>): string | null {
  const totalInput = usage.input + usage.cacheRead + usage.cacheWrite
  if (usage.cacheRead <= 0 || totalInput <= 0) return null
  return ((usage.cacheRead / totalInput) * 100).toFixed(1).replace(/\.0$/, '')
}

export function contextUsage(stats: ChatStats): {
  tokens: number
  window: number | null
  trigger: number | null
  fraction: number
  atTrigger: boolean
} {
  const tokens = stats.contextTokens
  const window = stats.contextWindow || null
  const trigger = stats.contextTrigger || null
  return {
    tokens,
    window,
    trigger,
    fraction: window ? tokens / window : 0,
    atTrigger: trigger !== null && tokens >= trigger
  }
}
