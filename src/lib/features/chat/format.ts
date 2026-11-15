import type { ChatStats, ChatUsage } from '$lib/types/backend'

export function formatTokens(n: number): string {
  if (n < 1000) return String(n)
  if (n < 1e6) return `${(n / 1000).toFixed(1).replace(/\.0$/, '')}k`
  return `${(n / 1e6).toFixed(1).replace(/\.0$/, '')}M`
}

export function formatCost(usd: number): string {
  return `$${usd.toFixed(2)}`
}

export function formatDuration(ms: number): string {
  if (ms < 1000) return `${Math.round(ms)}ms`
  if (ms < 60_000) return `${(ms / 1000).toFixed(1).replace(/\.0$/, '')}s`
  const seconds = Math.floor(ms / 1000)
  return `${Math.floor(seconds / 60)}m${seconds % 60}s`
}

export function formatCount(count: number, noun: string): string {
  return `${count} ${noun}${count === 1 ? '' : 's'}`
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
