import test from 'node:test'
import assert from 'node:assert/strict'
import type { Step } from './turns'
import { applySquashedChanges, summarizeTurnChanges, turnFileChanges } from './turnChanges'

function toolStep(
  id: string,
  name: 'edit' | 'write',
  change: Record<string, unknown>,
  status: 'done' | 'error' = 'done'
): Step {
  return {
    id,
    entryId: id,
    timestamp: null,
    endedAt: null,
    kind: 'tool',
    callId: id,
    name,
    arguments: {},
    status,
    output: [],
    details: { change },
    result: {
      kind: 'toolResult',
      callId: id,
      toolName: name,
      content: [],
      details: { change },
      isError: status === 'error',
      durationMs: 1,
      deliverables: []
    }
  }
}

test('turnFileChanges squashes multi-mutation files, drops net reverts, and keeps local:// urls', () => {
  const steps: Step[] = [
    toolStep('1', 'edit', {
      path: '/repo/src/a.ts',
      key: '/repo/src/a.ts',
      created: false,
      before: { sha256: 's0', bytes: 10, binary: false },
      after: { sha256: 's1', bytes: 12, binary: false },
      diff: { additions: 2, deletions: 0 }
    }),
    toolStep('2', 'edit', {
      path: '/repo/src/a.ts',
      key: '/repo/src/a.ts',
      created: false,
      before: { sha256: 's1', bytes: 12, binary: false },
      after: { sha256: 's2', bytes: 15, binary: false },
      diff: { additions: 3, deletions: 0 }
    }),
    toolStep('3', 'write', {
      path: '/repo/src/reverted.ts',
      key: '/repo/src/reverted.ts',
      created: false,
      before: { sha256: 'r0', bytes: 5, binary: false },
      after: { sha256: 'r1', bytes: 6, binary: false },
      diff: { additions: 1, deletions: 1 }
    }),
    toolStep('4', 'write', {
      path: '/repo/src/reverted.ts',
      key: '/repo/src/reverted.ts',
      created: false,
      before: { sha256: 'r1', bytes: 6, binary: false },
      after: { sha256: 'r0', bytes: 5, binary: false },
      diff: { additions: 1, deletions: 1 }
    }),
    toolStep('5', 'write', {
      path: '/home/u/.local/share/axome/axo/sessions/s/local/plan.md',
      key: 'local://plan.md',
      url: 'local://plan.md',
      created: true,
      after: { sha256: 'p1', bytes: 20, binary: false },
      diff: { additions: 4, deletions: 0 }
    }),
    toolStep(
      '6',
      'edit',
      {
        path: '/repo/src/failed.ts',
        key: '/repo/src/failed.ts',
        created: false,
        before: { sha256: 'f0', bytes: 5, binary: false },
        after: { sha256: 'f1', bytes: 6, binary: false }
      },
      'error'
    )
  ]

  const result = turnFileChanges(steps, '/repo')
  assert.equal(result.raw.length, 5)
  assert.equal(result.files.length, 2)
  assert.equal(result.needsBlobSquash, true)

  assert.equal(result.files[0].displayPath, 'src/a.ts')
  assert.equal(result.files[0].beforeSha, 's0')
  assert.equal(result.files[0].afterSha, 's2')
  assert.equal(result.files[0].mutations, 2)
  assert.equal(result.files[0].additions, null)

  assert.equal(result.files[1].displayPath, 'local://plan.md')
  assert.equal(result.files[1].created, true)
  assert.equal(result.files[1].additions, 4)
})

test('applySquashedChanges and summarizeTurnChanges produce net totals', () => {
  const merged = applySquashedChanges(
    [
      {
        path: '/repo/src/a.ts',
        key: '/repo/src/a.ts',
        url: null,
        created: false,
        beforeSha: 's0',
        afterSha: 's2',
        binary: false,
        omitted: false,
        mutations: 2,
        discontinuous: false,
        additions: 5,
        deletions: 1
      }
    ],
    '/repo'
  )
  assert.equal(merged[0].displayPath, 'src/a.ts')
  assert.deepEqual(summarizeTurnChanges(merged), {
    additions: 5,
    deletions: 1,
    hasUnknownCounts: false
  })
})
