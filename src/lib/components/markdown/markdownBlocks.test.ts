import test from 'node:test'
import assert from 'node:assert/strict'
import { normalizeNestedMarkdownFences, splitMessageBlocks } from './markdownBlocks'

test('splitMessageBlocks extracts top-level fenced code blocks alongside markdown slices', () => {
  const source = 'Intro\n\n```markdown\n# Plan\n```\n\nOutro\n'
  assert.deepEqual(splitMessageBlocks(source), [
    { kind: 'markdown', source: 'Intro\n\n' },
    { kind: 'code', lang: 'markdown', code: '# Plan' },
    { kind: 'markdown', source: 'Outro\n' }
  ])
})

test('splitMessageBlocks keeps empty-language code blocks and reference-definition docs intact', () => {
  assert.deepEqual(splitMessageBlocks('```\nhello\n```\n'), [{ kind: 'code', lang: '', code: 'hello' }])
  const withRef = '```ts\nx\n```\n\n[x]: https://e.x\n'
  assert.deepEqual(splitMessageBlocks(withRef), [{ kind: 'markdown', source: withRef }])
})

test('splitMessageBlocks keeps naive nested 3-backtick fences inside markdown blocks intact', () => {
  const source = ['```markdown', '```mermaid', 'flowchart TD', '  A --> B', '```', '```'].join('\n')
  assert.deepEqual(splitMessageBlocks(source), [
    {
      kind: 'code',
      lang: 'markdown',
      code: ['```mermaid', 'flowchart TD', '  A --> B', '```'].join('\n')
    }
  ])
})

test('normalizeNestedMarkdownFences handles streaming, multiple inner blocks, CRLF, and non-markdown fences', () => {
  const streaming = '```markdown\r\n```mermaid\r\nA --> B'
  assert.equal(normalizeNestedMarkdownFences(streaming), '````markdown\r\n```mermaid\r\nA --> B')

  const validFour = '````markdown\n```ts\nconst x = 1\n```\n````\n'
  assert.equal(normalizeNestedMarkdownFences(validFour), validFour)

  const insideRust = '```rust\nlet s = "```markdown\\n```ts\\n";\n```\n'
  assert.equal(normalizeNestedMarkdownFences(insideRust), insideRust)
})
