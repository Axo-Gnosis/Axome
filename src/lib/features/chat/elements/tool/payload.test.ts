import test from 'node:test'
import assert from 'node:assert/strict'
import { blockedCall, editDiff, fileRecords, parseExtensionOrigin } from './payload'

test('blocked markers distinguish anonymous blocks from malformed or absent details', () => {
  for (const origin of ['extension lab/gate', 'extension gate (scope abc123)', 'host policy']) {
    assert.deepEqual(blockedCall({ blocked: { origin, extra: true }, unrelated: true }), { origin })
  }
  assert.deepEqual(blockedCall({ blocked: { origin: null } }), { origin: null })
  assert.deepEqual(blockedCall({ blocked: {} }), { origin: null })
  for (const details of [
    null,
    {},
    { blocked: null },
    { blocked: [] },
    { blocked: true },
    { blocked: 'extension gate' }
  ]) {
    assert.equal(blockedCall(details), null)
  }
  for (const origin of [3, false, {}, []]) assert.equal(blockedCall({ blocked: { origin } }), null)
})

test('extension origin labels carry the ID and whether it is scoped, not the scope key', () => {
  assert.deepEqual(parseExtensionOrigin('extension lab/gate'), { id: 'lab/gate', scoped: false })
  assert.deepEqual(parseExtensionOrigin('extension gate (scope abc123)'), { id: 'gate', scoped: true })
  for (const label of [
    null,
    'host policy',
    'extension ',
    'extension two words',
    'extension  (scope abc123)',
    'extension gate (scope )',
    'extension gate (scope two words)',
    'extension gate (scope abc123',
    'extension gate (scope abc123) trailing'
  ]) {
    assert.equal(parseExtensionOrigin(label), null)
  }
})

const ledger = { path: '/repo/a.ts', sha256: 'def', access: 'modify' }
const blob = { sha256: 'abc', bytes: 6, binary: false }
const base = {
  path: ledger.path,
  key: ledger.path,
  created: false,
  before: blob,
  after: { ...blob, sha256: ledger.sha256, bytes: 5 }
}
const patch = '--- /repo/a.ts\n+++ /repo/a.ts\n@@ -1 +1 @@\n-before\n+after'
const change = { ...base, diff: { additions: 1, deletions: 1, firstChangedLine: 1, patch } }

test('file records retain ordered repeats, raw changes, and ledger-only accesses', () => {
  const read = { ...ledger, path: '/repo/b.ts', access: 'read' }
  const first = { ledger, change }
  const files = fileRecords({ files: [first, { ledger: read }, first] })
  assert.deepEqual(
    files.map((file) => file.ledger.path),
    [ledger.path, read.path, ledger.path]
  )
  assert.strictEqual(files[0].change?.raw, change)
  assert.strictEqual(files[2].change?.raw, change)
  assert.deepEqual(files[1], { ledger: read, change: null })
  const parsed = editDiff(files[0].change)
  assert.equal(parsed?.added, 1)
  assert.equal(parsed?.removed, 1)
})

test('malformed siblings and legacy envelopes cannot supply file records or a mismatched change', () => {
  for (const details of [undefined, null, {}, { files: null }, { files: {} }, { files: [] }, { ledger, change }]) {
    assert.deepEqual(fileRecords(details), [])
  }
  assert.deepEqual(
    fileRecords({
      files: [
        null,
        { change },
        { ledger: { ...ledger, access: 'unknown' }, change },
        { ledger: { ...ledger, path: '\n' }, change },
        { ledger: { ...ledger, sha256: 7 }, change },
        { ledger, change: { ...change, path: '/repo/other.ts' } },
        { ledger, change: { ...change, before: null } }
      ]
    }),
    [
      { ledger, change: null },
      { ledger, change: null }
    ]
  )
})

test('unchanged and truncated diffs and binary or omitted snapshots retain their recorded attributes', () => {
  const unchanged = { ...base, after: blob, diff: { additions: 0, deletions: 0 } }
  const truncated = { ...base, diff: { additions: 100, deletions: 0, patchTruncated: true } }
  const binary = { ...base, after: { ...base.after, binary: true } }
  const omitted = {
    ...base,
    before: { ...base.before, omitted: 'tooLarge' },
    after: { ...base.after, omitted: 'ioError' }
  }
  const files = fileRecords({
    files: [unchanged, truncated, binary, omitted].map((change) => ({ ledger, change }))
  })
  assert.deepEqual(editDiff(files[0].change), { rows: [], added: 0, removed: 0 })
  assert.equal(files[1].change?.diff?.patchTruncated, true)
  assert.equal(editDiff(files[1].change), null)
  assert.equal(files[2].change?.after.binary, true)
  assert.equal(files[3].change?.before?.omitted, 'tooLarge')
  assert.equal(files[3].change?.after.omitted, 'ioError')
})
