import test from 'node:test'
import assert from 'node:assert/strict'
import type { ChatPart } from '$lib/types/backend'
import { partsText } from '$lib/features/chat/parts/parts'
import { offsetPosition, readText, writeText } from './textForm'

const image: ChatPart = { type: 'content', name: 'shot 1.png', bytes: '' }
const files = new Set(['src/a.ts', 'my dir/b.ts', 'src/'])
const resolver = () => {
  let used = false
  return (ref: string): ChatPart | null => {
    if (ref === 'shot 1.png' && !used) return ((used = true), image)
    return files.has(ref) ? { type: 'file', path: ref } : null
  }
}

test('a draft survives the text view unchanged', () => {
  const draft: ChatPart[] = [
    { type: 'text', text: '/review compare ' },
    { type: 'file', path: 'src/a.ts' },
    { type: 'text', text: ' with ' },
    { type: 'file', path: 'my dir/b.ts' },
    { type: 'text', text: '. Log:\n' },
    { type: 'paste', text: 'line 1\n@src/a.ts stays paste\nline 3' },
    { type: 'text', text: ' ' },
    image
  ]
  const { text, pastes } = writeText(draft)
  assert.deepEqual(readText(text, pastes, resolver()).parts, draft)
  // The chat input lets chips touch text and each other; opening the text view must not drop them.
  const glued: ChatPart[] = [
    { type: 'text', text: 'look:' },
    image,
    { type: 'file', path: 'src/a.ts' },
    { type: 'text', text: "'s ok, see" },
    { type: 'file', path: 'src/' }
  ]
  assert.deepEqual(readText(writeText(glued).text, [], resolver()).parts, glued)
})

test('whatever becomes a chip, the text is preserved', () => {
  const texts = [
    'see @src/a.ts. and @src/missing.ts, mail a@b.c',
    '@"my dir/b.ts" @"src/a.ts" @"shot 1.png" @"shot 1.png"',
    '(@src/a.ts) @src/a.tsx @src/ @',
    ''
  ]
  for (const text of texts) assert.equal(partsText(readText(text, [], resolver()).parts), text)
  const { parts, refs } = readText(texts[0], [], resolver())
  assert.deepEqual(parts[1], { type: 'file', path: 'src/a.ts' })
  assert.deepEqual(parts[2], { type: 'text', text: '. and @src/missing.ts, mail a@b.c' })
  assert.deepEqual(
    refs.map((ref) => [ref.ref, !!ref.part]),
    [
      ['src/a.ts', true],
      ['src/missing.ts', false]
    ]
  )
  // An attachment is one thing: its second token stays text.
  assert.equal(readText(texts[1], [], resolver()).parts.filter((part) => part.type === 'content').length, 1)
})

test('text offsets map to composer positions', () => {
  const draft: ChatPart[] = [
    { type: 'text', text: 'ab ' },
    { type: 'file', path: 'src/a.ts' },
    { type: 'text', text: ' c' }
  ]
  // "ab @src/a.ts c": positions 1..4 are text, the chip is 4..5, then text again.
  assert.deepEqual(
    [0, 3, 5, 12, 14].map((offset) => offsetPosition(draft, offset)),
    [1, 4, 5, 5, 7]
  )
})
