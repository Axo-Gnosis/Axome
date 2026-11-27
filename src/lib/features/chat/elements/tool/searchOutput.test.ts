import test from 'node:test'
import assert from 'node:assert/strict'
import type { ToolStep } from '../../turns'
import { searchOutput, type SearchOutputModel } from './searchOutput'
import { toolHeader } from './toolHeader'

function call(
  name: 'grep' | 'find',
  source: string,
  details: unknown = null,
  args: Record<string, unknown> = {}
): ToolStep {
  const output = [{ type: 'text' as const, text: source }]
  return {
    id: 'tool',
    entryId: 'entry',
    timestamp: null,
    endedAt: null,
    kind: 'tool',
    callId: 'call',
    name,
    arguments: { pattern: '(a+)+$', limit: 2, ...args },
    status: 'done',
    output,
    details,
    result: {
      kind: 'toolResult',
      callId: 'call',
      toolName: name,
      content: output,
      details,
      isError: false,
      deliverables: [],
      durationMs: null
    }
  }
}
function lines(output: SearchOutputModel) {
  return output.parts.flatMap((part) => (part.type === 'text' ? part.lines : []))
}
function visible(output: SearchOutputModel): string {
  return output.parts
    .map((part) => (part.type === 'text' ? part.lines.map((row) => row.text).join('') + part.suffix : ''))
    .join('')
}

test('grep links file headers and the match and context rows grouped under them', () => {
  const source = 'src/a.ts\n1:hit\n2-ctx\n3:hit\n--\n9:far\n\nb c.ts\n4:x\n'
  const step = call('grep', source)
  const output = searchOutput(step)
  assert.equal(visible(output), source)
  assert.deepEqual(
    lines(output).map((row) => row.link),
    [
      { path: 'src/a.ts', length: 8 },
      { path: 'src/a.ts', line: 1, length: 1, matches: 1 },
      { path: 'src/a.ts', line: 2, length: 1 },
      { path: 'src/a.ts', line: 3, length: 1, matches: 1 },
      undefined,
      { path: 'src/a.ts', line: 9, length: 1, matches: 1 },
      undefined,
      { path: 'b c.ts', length: 6 },
      { path: 'b c.ts', line: 4, length: 1, matches: 1 }
    ]
  )
  assert.equal(toolHeader(step, '/repo').meta, '4 matches')
})

test('a group under an unsafe header stays raw without hiding its neighbours', () => {
  const source = '../x.ts\n1:hit\n\ngood.ts\n2:hit\nx:1'
  const output = searchOutput(call('grep', source))
  assert.equal(visible(output), source)
  assert.deepEqual(
    lines(output).map((row) => row.link),
    [
      undefined,
      undefined,
      undefined,
      { path: 'good.ts', length: 7 },
      { path: 'good.ts', line: 2, length: 1, matches: 1 },
      undefined
    ]
  )
})

test('grep links capped output and keeps the recorded notice separate', () => {
  const notice =
    '[Capped at 10 matches per file in b.ts. Grep one file for up to 50, add a line range (path:N-M), or narrow the pattern]'
  const capped = call('grep', `b.ts\n1:x\n\n${notice}`, { cappedFiles: ['b.ts'], notice }, { path: ['src', '!*.lock'] })
  const output = searchOutput(capped)
  assert.equal(lines(output)[1].link?.line, 1)
  assert.equal(output.parts[0].type === 'text' && output.parts[0].suffix, `\n\n${notice}`)
})

test('files mode links paths and counts files against the limit notice', () => {
  const step = call('grep', 'a.ts\nsrc/b.ts', null, { mode: 'files' })
  assert.deepEqual(
    lines(searchOutput(step)).map((row) => row.link),
    [
      { path: 'a.ts', length: 4 },
      { path: 'src/b.ts', length: 8 }
    ]
  )
  assert.equal(toolHeader(step, '/repo').meta, '2 files')
  const notice = '[2 files limit reached. Use limit=4 for more, or refine pattern]'
  const limited = searchOutput(
    call('grep', `a.ts\nsrc/b.ts\n\n${notice}`, { matchLimitReached: 2, notice }, { mode: 'files' })
  )
  assert.equal(lines(limited)[0].link?.path, 'a.ts')
  assert.equal(visible(limited), `a.ts\nsrc/b.ts\n\n${notice}`)
})

test('count mode links each path and sums its matches', () => {
  const step = call('grep', 'a.ts:2\nb.ts:12', null, { mode: 'count' })
  assert.deepEqual(
    lines(searchOutput(step)).map((row) => row.link),
    [
      { path: 'a.ts', length: 4, matches: 2 },
      { path: 'b.ts', length: 4, matches: 12 }
    ]
  )
  assert.equal(toolHeader(step, '/repo').meta, '14 matches')
})

test('find preserves exact paths and separates the recorded notice', () => {
  const filename = '[50.0KB limit reached]'
  const source = ` nested folder /\nnested/ a.ts \n${filename}`
  const output = searchOutput(call('find', source))
  assert.equal(visible(output), source)
  assert.deepEqual(
    lines(output).map((row) => row.link?.path),
    [' nested folder /', 'nested/ a.ts ', filename]
  )
  const notice = '[2 results limit reached. Use limit=4 for more, or refine pattern]'
  const withNotice = `${source}\n\n${notice}`
  const verified = searchOutput(call('find', withNotice, { resultLimitReached: 2, notice }))
  assert.equal(visible(verified), withNotice)
  assert.deepEqual(
    lines(verified).map((row) => row.link?.path),
    [' nested folder /', 'nested/ a.ts ', filename]
  )
  const error = call('find', source)
  error.status = 'error'
  error.result!.isError = true
  assert.ok(lines(searchOutput(error)).every((row) => !row.link))
})

test('search keeps output raw when a cut notice is missing or does not match the ending', () => {
  const notice = '[2 results limit reached. Use limit=4 for more, or refine pattern]'
  const source = `a.ts\nb.ts\n\n${notice}`
  const mismatch = `${source}\nextra`
  const wrongEnding = searchOutput(call('find', mismatch, { resultLimitReached: 2, notice }))
  assert.equal(visible(wrongEnding), mismatch)
  assert.ok(lines(wrongEnding).every((row) => !row.link))

  const missing = searchOutput(call('find', source, { resultLimitReached: 2 }))
  assert.equal(visible(missing), source)
  assert.ok(lines(missing).every((row) => !row.link))
})

test('preview keeps the first twelve logical lines and verified trailing notices; copy keeps the entire recording', () => {
  const body = ['file.ts', ...Array.from({ length: 15 }, (_, index) => `${index + 1}:${index + 1}`)].join('\n')
  const notice = '[Some lines truncated to 500 chars. Use read tool to see full lines]'
  const source = `${body}\n\n${notice}`
  const step = call('grep', source, { linesTruncated: true, notice })
  const preview = searchOutput(step, 12)
  assert.equal(preview.hidden, 4)
  assert.equal(lines(preview).length, 12)
  assert.equal(lines(preview)[1].link?.line, 1)
  assert.equal(lines(preview).at(-1)?.link?.line, 11)
  assert.equal(visible(preview), `${body.split('\n').slice(0, 12).join('\n')}\n\n${notice}`)
  assert.equal(visible(searchOutput(step)), source)
  assert.equal(searchOutput(step).hidden, 0)
})
