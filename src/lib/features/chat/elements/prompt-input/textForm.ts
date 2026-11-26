import type { ChatPart } from '$lib/types/backend'
import { partsText } from '$lib/features/chat/parts/parts'

export interface Range {
  from: number
  to: number
}
/** One `@…` token outside a paste: its range, what it names, and the part it stands for (null: plain text). */
export interface Ref extends Range {
  ref: string
  part: ChatPart | null
}

/** A message as the text the model receives, with where its pastes sit. */
export function writeText(parts: ChatPart[]): { text: string; pastes: Range[] } {
  let text = ''
  const pastes: Range[] = []
  for (const part of parts) {
    const chunk = partsText([part])
    if (part.type === 'paste' && chunk) pastes.push({ from: text.length, to: text.length + chunk.length })
    text += chunk
  }
  return { text, pastes }
}

// `@"a b"` only when quoting was needed, so a part always serializes back to the token it came from.
const REF = /@(?:"([^"\n]*[ \t][^"\n]*)"|(\S+))/g
const TRAILING = /[.,;:!?)\]}>'"]+$/

/**
 * The parts of an unquoted token that can end a reference, longest first: a chip runs straight into
 * the next one (`@a.ts@b.ts`), a possessive (`@a.ts's`) or the sentence's punctuation (`@a.ts.`).
 */
function* refEnds(token: string) {
  const trailing = TRAILING.exec(token)?.index ?? token.length
  for (let end = token.length - 1; end > 0; end--)
    if (end >= trailing || token[end] === '@' || token[end] === "'") yield token.slice(0, end)
}

/**
 * Text back to parts. `pastes` are sorted, non-empty, non-overlapping ranges kept as paste parts;
 * everywhere else each `@ref` becomes the part `resolve` returns, or stays text. Always
 * `partsText(result.parts) === text`.
 */
export function readText(
  text: string,
  pastes: Range[],
  resolve: (ref: string) => ChatPart | null
): { parts: ChatPart[]; refs: Ref[] } {
  const parts: ChatPart[] = []
  const refs: Ref[] = []
  const push = (chunk: string) => {
    if (!chunk) return
    const last = parts.at(-1)
    if (last?.type === 'text') parts[parts.length - 1] = { type: 'text', text: last.text + chunk }
    else parts.push({ type: 'text', text: chunk })
  }
  const scan = (from: number, to: number) => {
    const segment = text.slice(from, to)
    const pattern = new RegExp(REF)
    let done = 0
    for (let match; (match = pattern.exec(segment));) {
      const start = match.index
      const quoted = match[1] !== undefined
      let ref = quoted ? match[1] : match[2]
      let part = resolve(ref)
      if (!quoted && !part) {
        for (const shorter of refEnds(ref))
          if ((part = resolve(shorter))) {
            ref = shorter
            break
          }
        if (!part) ref = ref.replace(TRAILING, '') || ref
      }
      const end = start + ref.length + (quoted ? 3 : 1)
      // Resume right after the reference, so a chip glued to the next one reads both.
      pattern.lastIndex = end
      // A chip may sit against any text (`look:@shot.png`); an unresolved `@` inside a word (`a@b.c`) is no reference.
      const loose = start > 0 && start !== done && /\w/.test(segment[start - 1])
      if (!part) {
        if (!loose) refs.push({ from: from + start, to: from + end, ref, part })
        continue
      }
      refs.push({ from: from + start, to: from + end, ref, part })
      push(segment.slice(done, start))
      parts.push(part)
      done = end
    }
    push(segment.slice(done))
  }
  let at = 0
  for (const paste of pastes) {
    scan(at, paste.from)
    parts.push({ type: 'paste', text: text.slice(paste.from, paste.to) })
    at = paste.to
  }
  scan(at, text.length)
  return { parts, refs }
}

/** The composer document position for a text offset; an offset inside a chip's token lands after the chip. */
export function offsetPosition(parts: ChatPart[], offset: number): number {
  let position = 1
  for (const part of parts) {
    const length = partsText([part]).length
    if (part.type === 'text') {
      if (offset <= length) return position + offset
      position += length
    } else {
      if (offset <= 0) return position
      position += 1
      if (offset < length) return position
    }
    offset -= length
  }
  return position
}
