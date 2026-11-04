import type { ChatCommand, ChatPart } from '$lib/types/backend'
import { mediaMimeType } from '$lib/features/editor/languageMap'

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

/** Whether a message has something to send: non-blank text or any chip. */
export function hasContent(parts: ChatPart[]): boolean {
  return parts.some((part) => (part.type === 'text' || part.type === 'paste' ? part.text.trim() !== '' : true))
}

/** The skill a message invokes: its first text part starts with `/<skill>` followed by whitespace or the end. */
export function invokedSkill(
  parts: ChatPart[],
  commands: ChatCommand[]
): Extract<ChatCommand, { kind: 'skill' }> | null {
  const first = parts[0]
  const name = first?.type === 'text' ? /^\/(\S+)(?=\s|$)/.exec(first.text)?.[1] : undefined
  const command = name ? commands.find((item) => item.name === name) : undefined
  return command?.kind === 'skill' ? command : null
}

function reference(path: string): string {
  return /\s/.test(path) ? `@"${path}"` : `@${path}`
}

export function partsText(parts: ChatPart[]): string {
  return parts
    .map((part) => {
      switch (part.type) {
        case 'text':
        case 'paste':
          return part.text
        case 'file':
          return reference(part.path)
        case 'saved':
          return reference(part.url)
        case 'content':
          return reference(part.name)
      }
    })
    .join('')
}

export function partLabel(part: ChatPart): string {
  switch (part.type) {
    case 'text':
    case 'paste':
      return part.text.split('\n', 1)[0] || 'Pasted text'
    case 'file':
      return part.path.replace(/\/+$/, '').split('/').pop() || part.path
    case 'saved':
      return part.url.split('/').pop() || part.url
    case 'content':
      return part.name
  }
}

export function imageMime(part: ChatPart): string | null {
  const name = part.type === 'content' ? part.name : part.type === 'saved' ? part.url : ''
  const mime = mediaMimeType(name)
  if (mime.startsWith('image/')) return mime
  if (part.type !== 'content') return null
  // Clipboard files may have no extension. Only decode the signature, not the image.
  let signature: string
  try {
    signature = atob(part.bytes.slice(0, 48))
  } catch {
    return null
  }
  if (signature.startsWith('\x89PNG\r\n\x1a\n')) return 'image/png'
  if (signature.startsWith('\xff\xd8\xff')) return 'image/jpeg'
  if (signature.startsWith('GIF87a') || signature.startsWith('GIF89a')) return 'image/gif'
  if (signature.startsWith('RIFF') && signature.slice(8, 12) === 'WEBP') return 'image/webp'
  if (signature.startsWith('BM')) return 'image/bmp'
  if (signature.slice(4, 8) === 'ftyp' && ['avif', 'avis'].includes(signature.slice(8, 12))) return 'image/avif'
  return null
}

export function isImagePart(part: ChatPart): boolean {
  return part.type === 'saved' ? part.image : part.type === 'content' && imageMime(part) !== null
}
