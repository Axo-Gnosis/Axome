import test from 'node:test'
import assert from 'node:assert/strict'
import { memoryNote } from '$lib/features/memory/notes'
import type { ToolStep } from '../../turns'
import { parseMemorySearch } from './memoryOutput'

test('memory search reads flattened hits in both casings and counts notes by URL', () => {
  const url = 'memory://main/Axome/Pause menu.md'
  const links = [{ url: 'memory://main/Axome/HUD.md' }]
  const row = { title: 'Pause menu', category: 'decision', snippet: 'The pause menu overlays the HUD.', url, line: 4 }
  const other = { ...row, title: 'HUD', url: 'memory://main/Axome/HUD.md' }
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
    details: {
      hits: [
        { row_type: 'observation', entity_id: 9, ...row, links },
        { rowType: 'entity', entityId: 9, ...row, links },
        { rowType: 'entity', entityId: 10, ...other, links: [] }
      ]
    }
  }
  assert.deepEqual(parseMemorySearch(call), {
    hits: [
      { rowType: 'observation', ...row, links },
      { rowType: 'entity', ...row, links },
      { rowType: 'entity', ...other, links: [] }
    ],
    notes: 2
  })
  assert.equal(parseMemorySearch({ ...call, details: {} }), null)
})

test('memory note URLs keep the bank and nested note path without the Markdown extension', () => {
  assert.deepEqual(memoryNote('memory://main/Axome/Pause menu.md'), { bank: 'main', note: 'Axome/Pause menu' })
  assert.equal(memoryNote('https://example.org/Axome/Pause menu.md'), null)
})
