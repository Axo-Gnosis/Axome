import test from 'node:test'
import assert from 'node:assert/strict'
import type { ToolStep, Turn } from '../turns'
import { memoryUses } from './memoryUses'

function call(id: string, name: string, args: Record<string, unknown>): ToolStep {
  return {
    id,
    entryId: id,
    timestamp: null,
    endedAt: null,
    kind: 'tool',
    callId: id,
    name,
    arguments: args,
    status: 'done',
    result: null,
    output: [],
    details: { hits: [{ row_type: 'entity', entity_id: 1, title: 'HUD', snippet: '', url: 'memory://main/HUD.md' }] }
  }
}

test('memory uses keep each note once at its first call, upgraded to edited by any later change', () => {
  const pause = 'memory://main/Axome/Pause menu.md'
  const turn = {
    steps: [
      call('search', 'memory_search', { query: 'pause' }),
      call('read', 'read', { path: pause }),
      call('file', 'read', { path: '/repo/README.md' }),
      call('prefs', 'read', { path: 'memory://me/Preferences.md' }),
      call('edit', 'edit', { path: pause })
    ]
  } as unknown as Turn
  assert.deepEqual(memoryUses([turn]), [
    { kind: 'search', stepId: 'search', query: 'pause', notes: 1 },
    { kind: 'note', stepId: 'read', url: pause, note: 'Axome/Pause menu', access: 'edited' },
    { kind: 'note', stepId: 'prefs', url: 'memory://me/Preferences.md', note: 'Preferences', access: 'read' }
  ])
})
