import type { ChatCommand, ChatPart } from '$lib/types/backend'

/** Whether a message has something to send: non-blank text or any chip. */
export function hasContent(parts: ChatPart[]): boolean {
  return parts.some((part) => (part.type === 'text' || part.type === 'paste' ? part.text.trim() !== '' : true))
}

/**
 * Prepends withdrawn queued parts ahead of an existing composer draft with a newline
 * separator when the draft already contains content (omitting empty drafts).
 */
export function prependQueuedParts(queued: ChatPart[], draft: ChatPart[]): ChatPart[] {
  if (!hasContent(draft)) return [...queued]
  const combined: ChatPart[] = [...queued, { type: 'text', text: '\n' }, ...draft]
  const normalized: ChatPart[] = []
  for (const part of combined) {
    const prev = normalized.at(-1)
    if (part.type === 'text' && prev?.type === 'text') {
      normalized[normalized.length - 1] = { type: 'text', text: prev.text + part.text }
    } else if (part.type !== 'text' || part.text.length > 0) {
      normalized.push(part)
    }
  }
  return normalized
}

export function contentBytes(parts: ChatPart[]): number {
  return parts.reduce(
    (total, part) =>
      total +
      (part.type === 'content'
        ? Math.floor((part.bytes.length * 3) / 4) - (part.bytes.endsWith('==') ? 2 : part.bytes.endsWith('=') ? 1 : 0)
        : 0),
    0
  )
}
// Mirrors axome_protocol::chat::MAX_CONTENT_BYTES.
export const CONTENT_LIMIT = 20 * 1024 * 1024
export const CONTENT_LIMIT_MESSAGE = 'A message can contain at most 20 MiB of content.'

/** The catalog command of `kind` a message invokes: its first text part starts with `/<name>` followed by whitespace or the end. */
export function invokedCommand<K extends ChatCommand['kind']>(
  parts: ChatPart[],
  commands: ChatCommand[],
  kind: K
): Extract<ChatCommand, { kind: K }> | null {
  const first = parts[0]
  const name = first?.type === 'text' ? /^\/(\S+)(?=\s|$)/.exec(first.text)?.[1] : undefined
  const command = name ? commands.find((item) => item.name === name) : undefined
  return command?.kind === kind ? (command as Extract<ChatCommand, { kind: K }>) : null
}

/** Whether pasted text is drawn collapsed (a chip, a folded region) rather than inline. */
export function isLargePaste(text: string): boolean {
  return text.length > 2000 || text.split('\n').length > 20
}

/** What a chip's `@ref` names: a file's path, a saved attachment's url, an attachment's name; null for text. */
export function partRef(part: ChatPart): string | null {
  switch (part.type) {
    case 'text':
    case 'paste':
      return null
    case 'file':
      return part.path
    case 'saved':
      return part.url
    case 'content':
      return part.name
  }
}

/** A part as the model reads it: text as is, a chip as `@ref`, quoted when the ref holds whitespace. */
export function partText(part: ChatPart): string {
  if (part.type === 'text' || part.type === 'paste') return part.text
  const ref = partRef(part)!
  return /\s/.test(ref) ? `@"${ref}"` : `@${ref}`
}

export function partsText(parts: ChatPart[]): string {
  return parts.map(partText).join('')
}

export function partLabel(part: ChatPart): string {
  switch (part.type) {
    case 'text':
      return part.text.split('\n', 1)[0]
    case 'paste':
      return 'Pasted text'
    case 'file':
      return part.path.replace(/\/+$/, '').split('/').pop() || part.path
    case 'saved':
      return part.url.split('/').pop() || part.url
    case 'content':
      return part.name
  }
}
