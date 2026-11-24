import test from 'node:test'
import assert from 'node:assert/strict'
import { memoryNote } from '$lib/features/memory/notes'
import type { ToolStep } from '../../turns'
import { parseMemorySearch } from './memoryOutput'

test('memory search reads flattened snake_case hits and linked note URLs', () => {
  const url = 'memory://main/Axome/Pause menu.md'
  const links = [{ url: 'memory://main/Axome/HUD.md' }]
  const row = { title: 'Pause menu', category: 'decision', snippet: 'The pause menu overlays the HUD.', url, line: 4 }
  const call: ToolStep = {
    id: 'tool',
    entryId: 'entry',
    timestamp: null,
    endedAt: null,
    kind: 'tool',
    callId: 'call',
    name: 'memory_search',
    arguments: { query: 'pause menu' },
    status: 'done',
    output: [],
    result: null,
    details: { hits: [{ row_type: 'observation', entity_id: 9, ...row, links }] }
  }
  assert.deepEqual(parseMemorySearch(call), {
    hits: [{ rowType: 'observation', entityId: 9, ...row, links }],
    notes: 1
  })
  assert.equal(parseMemorySearch({ ...call, details: {} }), null)
})

test('memory note URLs keep the bank and nested note path without the Markdown extension', () => {
  assert.deepEqual(memoryNote('memory://main/Axome/Pause menu.md'), { bank: 'main', note: 'Axome/Pause menu' })
  assert.equal(memoryNote('https://example.org/Axome/Pause menu.md'), null)
})
