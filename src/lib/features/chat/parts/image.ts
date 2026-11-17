import type { ChatPart } from '$lib/types/backend'
import { mediaMimeType } from '$lib/features/editor/languageMap'

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
