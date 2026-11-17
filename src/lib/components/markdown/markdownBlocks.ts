import { unified } from 'unified'
import remarkParse from 'remark-parse'
import remarkGfm from 'remark-gfm'

export type MessageBlock = { kind: 'markdown'; source: string } | { kind: 'code'; lang: string; code: string }

const FENCE_RE = /^([ ]{0,3})(`{3,}|~{3,})([^\r\n]*)$/
export const MARKDOWN_FENCE_LANGS = new Set(['markdown', 'md'])

/**
 * Upgrades naive outer `markdown`/`md` fences when inner fenced code blocks
 * reuse the same or wider fence length (e.g. ` ```markdown ` wrapping ` ```mermaid `).
 */
export function normalizeNestedMarkdownFences(source: string): string {
  if (!source.includes('```') && !source.includes('~~~')) return source

  const parts = source.split(/(\r?\n)/)
  type Fence = { linePartIndex: number; indent: string; ch: '`' | '~'; len: number; info: string }
  let outer: (Fence & { hadNaiveInner: boolean; maxSameCharLen: number }) | null = null
  let plainOuter: { ch: '`' | '~'; len: number } | null = null
  let innerStack: { ch: '`' | '~'; len: number }[] = []
  let changed = false

  const upgradeOuter = (closePartIndex?: number) => {
    if (!outer || !outer.hadNaiveInner) return
    const nextFence = outer.ch.repeat(outer.maxSameCharLen + 1)
    parts[outer.linePartIndex] = `${outer.indent}${nextFence}${outer.info}`
    if (closePartIndex !== undefined) {
      const closeMatch = FENCE_RE.exec(parts[closePartIndex])
      if (closeMatch) {
        parts[closePartIndex] = `${closeMatch[1]}${nextFence}${closeMatch[3]}`
      }
    }
    changed = true
  }

  for (let i = 0; i < parts.length; i += 2) {
    const match = FENCE_RE.exec(parts[i])
    if (!match) continue

    const indent = match[1]
    const fence = match[2]
    const ch = fence[0] as '`' | '~'
    const len = fence.length
    const info = match[3]
    const trimmedInfo = info.trim()

    if (trimmedInfo && ch === '`' && info.includes('`')) continue

    if (plainOuter) {
      if (!trimmedInfo && ch === plainOuter.ch && len >= plainOuter.len) {
        plainOuter = null
      }
      continue
    }

    if (!outer) {
      if (trimmedInfo) {
        const lang = (trimmedInfo.split(/[,\s]/, 1)[0] ?? '').toLowerCase()
        if (MARKDOWN_FENCE_LANGS.has(lang)) {
          outer = { linePartIndex: i, indent, ch, len, info, hadNaiveInner: false, maxSameCharLen: len }
          innerStack = []
        } else {
          plainOuter = { ch, len }
        }
      } else {
        plainOuter = { ch, len }
      }
      continue
    }

    if (!trimmedInfo) {
      const top = innerStack.at(-1)
      if (top && ch === top.ch && len >= top.len) {
        if (ch === outer.ch) outer.maxSameCharLen = Math.max(outer.maxSameCharLen, len)
        innerStack.pop()
      } else if (innerStack.length === 0 && ch === outer.ch && len >= outer.len) {
        upgradeOuter(i)
        outer = null
      }
      continue
    }

    if (ch === outer.ch && len >= outer.len) {
      outer.hadNaiveInner = true
      outer.maxSameCharLen = Math.max(outer.maxSameCharLen, len)
    }
    innerStack.push({ ch, len })
  }

  if (outer) upgradeOuter()
  return changed ? parts.join('') : source
}

export function splitMessageBlocks(source: string): MessageBlock[] {
  if (!source) return []
  const normalized = normalizeNestedMarkdownFences(source)

  const { children } = unified().use(remarkParse).use(remarkGfm).parse(normalized)
  if (children.some((child) => child.type === 'definition' || child.type === 'footnoteDefinition')) {
    return [{ kind: 'markdown', source: normalized }]
  }

  const blocks: MessageBlock[] = []
  for (let index = 0; index < children.length; index++) {
    const child = children[index]
    const start = index === 0 ? 0 : child.position?.start.offset
    const end = index + 1 < children.length ? children[index + 1].position?.start.offset : normalized.length
    if (start === undefined || end === undefined) return [{ kind: 'markdown', source: normalized }]
    if (child.type === 'code') {
      blocks.push({ kind: 'code', lang: child.lang ?? '', code: child.value })
    } else {
      blocks.push({ kind: 'markdown', source: normalized.slice(start, end) })
    }
  }
  return blocks.length ? blocks : [{ kind: 'markdown', source: normalized }]
}
