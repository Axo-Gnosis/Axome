import test from 'node:test'
import assert from 'node:assert/strict'
import { editDiff } from './payload'

test('an authoritative empty display diff still wins over a patch', () => {
  const details = { diff: '', patch: '@@ -1 +1 @@\n-before\n+after' }
  assert.deepEqual(editDiff(details), { rows: [], added: 0, removed: 0 })
})
