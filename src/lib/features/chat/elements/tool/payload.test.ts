import test from 'node:test'
import assert from 'node:assert/strict'
import { editDiff, fileChange } from './payload'

test('fileChange and editDiff parse details.change from edit and write results', () => {
  const details = {
    change: {
      path: '/repo/a.ts',
      key: '/repo/a.ts',
      created: false,
      before: { sha256: 'abc', bytes: 6, binary: false },
      after: { sha256: 'def', bytes: 5, binary: false },
      diff: {
        additions: 1,
        deletions: 1,
        firstChangedLine: 1,
        patch: '--- /repo/a.ts\n+++ /repo/a.ts\n@@ -1 +1 @@\n-before\n+after'
      }
    }
  }
  const change = fileChange(details)
  assert.ok(change)
  assert.equal(change.diff?.additions, 1)
  assert.equal(change.diff?.deletions, 1)
  const parsed = editDiff(fileChange(details))
  assert.equal(parsed?.added, 1)
  assert.equal(parsed?.removed, 1)
})

test('editDiff returns empty result for an unchanged edit and null for truncated patches', () => {
  const base = {
    path: '/repo/a.ts',
    key: '/repo/a.ts',
    created: false,
    before: { sha256: 'abc', bytes: 5, binary: false },
    after: { sha256: 'abc', bytes: 5, binary: false }
  }
  assert.deepEqual(editDiff(fileChange({ change: { ...base, diff: { additions: 0, deletions: 0 } } })), {
    rows: [],
    added: 0,
    removed: 0
  })
  assert.equal(
    editDiff(fileChange({ change: { ...base, diff: { additions: 100, deletions: 0, patchTruncated: true } } })),
    null
  )
})
