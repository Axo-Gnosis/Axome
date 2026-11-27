import test from 'node:test'
import assert from 'node:assert/strict'
import type { ChatContent } from '$lib/types/backend'
import type { ToolStep } from '../../turns'
import { toolHeader } from './toolHeader'
import { readOutput, readPath, readSelector } from './readOutput'

const text = (text: string): ChatContent => ({ type: 'text', text })
const image: ChatContent = { type: 'image', mimeType: 'image/png', data: 'AA==' }
const ledger = { path: '/actual/source.ts', access: 'read', sha256: 'recorded' }
const files = [{ ledger }]
function call(args: Record<string, unknown>, output: string | ChatContent[], details: unknown = null): ToolStep {
  const content = typeof output === 'string' ? [text(output)] : output
  return {
    id: 'tool',
    entryId: 'entry',
    timestamp: null,
    endedAt: null,
    kind: 'tool',
    callId: 'call',
    name: 'read',
    arguments: args,
    status: 'done',
    output: content,
    details,
    result: {
      kind: 'toolResult',
      callId: 'call',
      toolName: 'read',
      content,
      details,
      isError: false,
      deliverables: [],
      durationMs: null
    }
  }
}
const E = (path: string, totalLines: number, lines: [number, number][], extra: Record<string, unknown> = {}) => ({
  path,
  totalLines,
  lines,
  outline: false,
  truncated: false,
  notice: null,
  ...extra
})
const ctx = (text: string, lineNumber: number | null) => ({ kind: 'ctx', text, lineNumber })
const gap = { kind: 'gap' }

test('read exposes exact numbered source separately from attached context', () => {
  const attachment = text('Attached project instructions\nnot source\nnot source')
  const step = call({ path: 'requested.ts' }, [text('requested.ts (2 lines)\n1\t\tfirst  \n2\t second'), attachment], {
    files,
    reads: [E('requested.ts', 2, [[1, 2]])]
  })
  const output = step.output
  const read = readOutput(step)!
  assert.deepEqual(read.leading, [])
  assert.equal(read.sources.length, 1)
  assert.deepEqual(read.sources[0].rows, [ctx('\tfirst  ', 1), ctx(' second', 2)])
  assert.equal(read.sources[0].notice, null)
  assert.deepEqual(read.sources[0].attachments, [attachment])
  assert.strictEqual(step.output, output)
  const header = toolHeader(step, '/actual')
  assert.equal(header.path, ledger.path)
  assert.equal(header.line, 1)
  assert.equal(header.pathLabel, 'source.ts:1-2')
})

test('ranged reads keep their own line numbers with gaps between ranges', () => {
  const step = call({ path: 'a.ts:5-6,9' }, 'a.ts:5-6,9 (10 lines)\n5\tx\n6\ty\n9\tz', {
    files,
    reads: [
      E('a.ts', 10, [
        [5, 6],
        [9, 9]
      ])
    ]
  })
  assert.deepEqual(readOutput(step)!.sources[0].rows, [ctx('x', 5), ctx('y', 6), gap, ctx('z', 9)])
  const header = toolHeader(step, '/actual')
  assert.equal(header.line, 5)
  assert.equal(header.pathLabel, 'source.ts:5-6,9')
})

test('read separates the recorded notice from numbered rows', () => {
  const footer = '[Showing lines 1-2 of 3000. Continue with a.ts:3-]'
  const source = `a.ts (3000 lines)\n1\ta\n2\tb\n\n${footer}`
  const cut = readOutput(
    call({ path: 'a.ts' }, source, { reads: [E('a.ts', 3000, [[1, 2]], { truncated: true, notice: footer })] })
  )!
  assert.equal(cut.sources[0].notice, footer)
  assert.deepEqual(cut.sources[0].rows, [ctx('a', 1), ctx('b', 2)])
  for (const notice of [
    '[Output limit reached before line 1. Continue with b.ts:1-5]',
    "[Line 1 is 60.0KB, exceeds 50.0KB limit. Use bash: sed -n '1p' b.ts | head -c 51200]"
  ]) {
    const empty = readOutput(
      call({ path: 'b.ts' }, `b.ts (5 lines)\n\n${notice}`, { reads: [E('b.ts', 5, [], { truncated: true, notice })] })
    )!
    assert.equal(empty.sources[0].notice, notice)
    assert.deepEqual(empty.sources[0].rows, [])
  }
})

test('read falls back when a truncation notice is missing or does not match', () => {
  const body = 'a.ts (3000 lines)\n1\ta\n2\tb'
  const args = { path: 'a.ts' }
  assert.equal(
    readOutput(
      call(args, `${body}\n\n[An unexpected footer]`, {
        reads: [
          E('a.ts', 3000, [[1, 2]], { truncated: true, notice: '[Showing lines 1-2 of 3000. Continue with a.ts:3-]' })
        ]
      })
    ),
    null
  )
  assert.equal(readOutput(call(args, body, { reads: [E('a.ts', 3000, [[1, 2]], { truncated: true })] })), null)
})

test('outlines fold bodies into gap rows', () => {
  const footer = '[Outline: 38 of 40 lines folded. Read what you need with a selector, e.g. big.rs:2-39]'
  const step = call({ path: 'big.rs' }, `big.rs (40 lines, outline)\n1\timpl X {\n… 2-39\n40\t}\n\n${footer}`, {
    files,
    reads: [E('big.rs', 40, [], { outline: true, notice: footer })]
  })
  const source = readOutput(step)!.sources[0]
  assert.deepEqual(source.rows, [ctx('impl X {', 1), gap, ctx('}', 40)])
  assert.equal(source.notice, footer)
  assert.equal(toolHeader(step, '/actual').pathLabel, 'source.ts')
})

test('several paths become sections that own the errors, skipped paths and images after them', () => {
  const enoent = text('b.ts: ENOENT: no such file or directory')
  const skipped = text('[d.ts: not read, output limit reached; read it in another call]')
  const step = call(
    { path: ['a.ts', 'b.ts', 'c.ts', 'd.ts'] },
    [text('a.ts (1 line)\n1\tx'), enoent, image, text('c.ts (1 line)\n1\ty'), skipped],
    { reads: [E('a.ts', 1, [[1, 1]]), E('c.ts', 1, [[1, 1]])] }
  )
  const read = readOutput(step)!
  assert.equal(read.sources.length, 2)
  assert.deepEqual(read.sources[0].attachments, [enoent, image])
  assert.deepEqual(read.sources[1].rows, [ctx('y', 1)])
  assert.deepEqual(read.sources[1].attachments, [skipped])
  const header = toolHeader(step, '/repo')
  assert.equal(header.summary, '4 files')
  assert.equal(header.path, null)
})

test('directories list entries without line numbers', () => {
  const listed = readOutput(
    call({ path: 'src' }, 'src/ (2 entries)\nsub/\nx', {
      reads: [{ path: 'src', directory: true, entries: 2, notice: null }]
    })
  )!
  assert.equal(listed.sources[0].directory, true)
  assert.deepEqual(listed.sources[0].rows, [ctx('sub/', null), ctx('x', null)])
  const more = '[1 more entries; use find to search]'
  const capped = readOutput(
    call({ path: 'src' }, `src/ (3 entries)\na/\nb\n\n${more}`, {
      reads: [{ path: 'src', directory: true, entries: 3, notice: more }]
    })
  )!
  assert.equal(capped.sources[0].notice, more)
  assert.deepEqual(capped.sources[0].rows, [ctx('a/', null), ctx('b', null)])
  assert.equal(
    readOutput(
      call({ path: 'src' }, 'src/ (3 entries)\na/\nb', {
        reads: [{ path: 'src', directory: true, entries: 3, notice: null }]
      })
    ),
    null
  )
  assert.ok(
    readOutput(
      call({ path: 'one' }, 'one/ (1 entry)\nx', {
        reads: [{ path: 'one', directory: true, entries: 1, notice: null }]
      })
    )
  )
})

test('anything the entries do not prove falls back to raw output', () => {
  const one = [E('a.ts', 1, [[1, 1]])]
  assert.equal(readOutput(call({ path: 'a.ts' }, 'a.ts (1 line)\n1\tx', { files })), null)
  assert.equal(readOutput(call({ path: 'a.ts' }, 'a.ts (1 line)\nx', { reads: one })), null)
  assert.equal(
    readOutput(call({ path: 'a.ts' }, 'a.ts (3 lines)\n1\tx\n2\ty', { reads: [E('a.ts', 2, [[1, 2]])] })),
    null
  )
  assert.equal(
    readOutput(call({ path: 'a.ts' }, 'a.ts (1 line)\n1\tx', { reads: [...one, E('b.ts', 1, [[1, 1]])] })),
    null
  )
  const failed = call({ path: 'a.ts' }, 'a.ts (1 line)\n1\tx', { reads: one })
  failed.status = 'error'
  assert.equal(readOutput(failed), null)
  assert.equal(readOutput(call({ path: 'p.png' }, [text('Read image file [image/png]'), image], { reads: [] })), null)
  assert.equal(
    readOutput(
      call({ path: 'a.ts' }, 'a.ts (1 line)\n1\tx\n\n[9 more lines in file. Use offset=2 to continue.]', { reads: one })
    ),
    null
  )
})

test('selectors split at the last colon only when the suffix is a line selector', () => {
  assert.deepEqual(readSelector('src/a.rs:5-16,960-973'), { base: 'src/a.rs', selector: '5-16,960-973', line: 5 })
  assert.equal(readSelector('x.log:-60')?.line, undefined)
  for (const literal of ['C:\\x', 'a:b', 'a.ts:']) assert.equal(readSelector(literal), null)
  assert.equal(readSelector('agent://a/transcript:1+5')?.base, 'agent://a/transcript')
  assert.equal(readPath(call({ path: 'local://s.ts:3' }, 'one')), 'local://s.ts')
})

test('a running ranged read labels its requested selector', () => {
  const step = call({ path: 'a.ts:20-30' }, [])
  step.status = 'running'
  step.result = null
  const header = toolHeader(step, '/repo')
  assert.equal(header.line, 20)
  assert.equal(header.pathLabel, 'a.ts:20-30')
})

test('tool headers count all recorded mutations without guessing partial totals or cancellation', () => {
  const file = (path: string, diff?: { additions: number; deletions: number }) => ({
    ledger: { path, access: 'modify', sha256: 'after' },
    change: {
      path,
      key: path,
      created: false,
      before: { sha256: 'before', bytes: 4, binary: false },
      after: { sha256: 'after', bytes: 5, binary: false },
      diff
    }
  })
  const a = file('/repo/a.ts', { additions: 22, deletions: 20 })
  const b = file('/repo/b.ts', { additions: 12, deletions: 11 })
  const c = file('/repo/c.ts', { additions: 7, deletions: 7 })
  const failed = (name: string, output: string, files: unknown[]) => {
    const step = call({ path: '/wrong.ts' }, output, { files })
    step.name = name
    step.status = 'error'
    step.result!.isError = true
    return toolHeader(step, '/repo')
  }
  const multiple = failed('multi_file_extension', 'failed after edits', [a, b, c])
  assert.equal(multiple.summary, '3 files')
  assert.equal(multiple.path, null)
  assert.deepEqual(multiple.lines, { added: 41, removed: 38 })
  assert.equal(multiple.status?.label, 'Failed')
  assert.equal(failed('multi_file_extension', 'failed', [a, file('/repo/unknown.ts')]).lines, null)

  const stopped = failed('edit', 'Operation aborted', [a])
  assert.equal(stopped.verb, 'Edit stopped')
  assert.equal(stopped.status, null)
  assert.equal(stopped.meta, 'saved before it stopped')
  assert.equal(stopped.unfinished, 'You stopped this edit after it had saved the file. The change below is in a.ts.')
  assert.equal(
    failed('edit', 'Operation aborted', [{ ledger: a.ledger, change: {} }]).unfinished,
    'This call did not finish. Its recorded file accesses are shown below.'
  )
  assert.equal(
    failed('edit', 'Operation aborted while opening the file', [a]).unfinished,
    'This call did not finish. The recorded file changes below were saved before it ended.'
  )
})
