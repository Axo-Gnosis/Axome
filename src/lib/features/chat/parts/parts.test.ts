import test from 'node:test'
import assert from 'node:assert/strict'
import { prependQueuedParts } from './parts'

test('prependQueuedParts loads directly into an empty draft and prepends with a newline otherwise', () => {
  assert.deepEqual(prependQueuedParts([{ type: 'text', text: 'queued' }], []), [{ type: 'text', text: 'queued' }])
  assert.deepEqual(prependQueuedParts([{ type: 'text', text: '   ' }], [{ type: 'text', text: 'queued' }]), [
    { type: 'text', text: '   \nqueued' }
  ])
  assert.deepEqual(prependQueuedParts([{ type: 'text', text: 'queued' }], [{ type: 'text', text: '   \n' }]), [
    { type: 'text', text: 'queued' }
  ])
  assert.deepEqual(
    prependQueuedParts(
      [
        { type: 'text', text: 'alt' },
        { type: 'file', path: 'a.ts' }
      ],
      [{ type: 'text', text: 'current' }]
    ),
    [
      { type: 'text', text: 'alt' },
      { type: 'file', path: 'a.ts' },
      { type: 'text', text: '\ncurrent' }
    ]
  )
  assert.deepEqual(prependQueuedParts([{ type: 'text', text: 'alt' }], [{ type: 'file', path: 'b.ts' }]), [
    { type: 'text', text: 'alt\n' },
    { type: 'file', path: 'b.ts' }
  ])
})
