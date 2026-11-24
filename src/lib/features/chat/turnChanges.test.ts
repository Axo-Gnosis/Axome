import test from 'node:test'
import assert from 'node:assert/strict'
import type { ToolStep } from './turns'
import { applySquashedChanges, summarizeTurnChanges, turnFileChanges } from './turnChanges'

type Change = { path: string; after: { sha256: string; bytes: number; binary: boolean } } & Record<string, unknown>

function toolStep(id: string, name: string, changes: Change | Change[], status: 'done' | 'error' = 'done'): ToolStep {
  const details = {
    files: (Array.isArray(changes) ? changes : [changes]).map((change) => ({
      ledger: { path: change.path, sha256: change.after.sha256, access: 'modify' },
      change
    }))
  }
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
    details,
    result: {
      kind: 'toolResult',
      callId: id,
      toolName: name,
      content: [],
      details,
      isError: status === 'error',
      durationMs: 1,
      deliverables: []
    }
  }
}

function mutation(
  path: string,
  before: string,
  after: string,
  diff?: { additions: number; deletions: number },
  key = path
) {
  return {
    path,
    key,
    created: false,
    before: { sha256: before, bytes: 5, binary: false },
    after: { sha256: after, bytes: 6, binary: false },
    diff
  }
}

function turnFixture() {
  const path = '/repo/src/a.ts'
  const alias = '/repo/src/a-link.ts'
  const reverted = '/repo/src/reverted.ts'
  const steps = [
    toolStep('1', 'edit', mutation(path, 's0', 's1', { additions: 2, deletions: 0 })),
    toolStep('2', 'edit', mutation(alias, 's1', 's2', { additions: 3, deletions: 1 }, path), 'error'),
    toolStep('3', 'write', mutation(reverted, 'r0', 'r1', { additions: 1, deletions: 1 }), 'error'),
    toolStep('4', 'write', mutation(reverted, 'r1', 'r0', { additions: 1, deletions: 1 })),
    toolStep(
      '6',
      'multi_file_extension',
      [
        {
          path: '/home/u/.local/share/axome/axo/sessions/s/local/plan.md',
          key: 'local://plan.md',
          url: 'local://plan.md',
          created: true,
          after: { sha256: 'p1', bytes: 20, binary: false },
          diff: { additions: 4, deletions: 0 }
        },
        mutation('/repo/src/other.ts', 'f0', 'f1')
      ],
      'error'
    )
  ]

  return turnFileChanges(steps, '/repo')
}

test('turn changes retain ordered mutations from any tool but drop net reverts', () => {
  const result = turnFixture()
  assert.deepEqual(
    result.raw.map((change) => change.key),
    [
      '/repo/src/a.ts',
      '/repo/src/a.ts',
      '/repo/src/reverted.ts',
      '/repo/src/reverted.ts',
      'local://plan.md',
      '/repo/src/other.ts'
    ]
  )
  assert.equal(result.files.length, 3)
  assert.equal(result.needsBlobSquash, true)

  assert.equal(result.files[0].displayPath, 'src/a-link.ts')
  assert.equal(result.files[0].beforeSha, 's0')
  assert.equal(result.files[0].afterSha, 's2')
  assert.equal(result.files[0].mutations, 2)
  assert.equal(result.files[0].additions, null)
  assert.equal(result.files[0].unfinished, true)
  assert.equal(result.files[1].displayPath, 'local://plan.md')
  assert.equal(result.files[1].created, true)
  assert.equal(result.files[1].additions, 4)
  assert.equal(result.files[1].unfinished, true)
  assert.equal(result.files[2].displayPath, 'src/other.ts')
  assert.equal(result.files[2].deletions, null)
  assert.deepEqual(summarizeTurnChanges(result.files), {
    additions: 4,
    deletions: 0,
    hasUnknownCounts: true,
    unfinished: 3
  })
})

test('squash keeps unfinished annotations by canonical key only for retained files', () => {
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
    '/repo',
    turnFixture().files
  )
  assert.equal(merged[0].displayPath, 'src/a.ts')
  assert.equal(merged[0].unfinished, true)
  assert.deepEqual(summarizeTurnChanges(merged), {
    additions: 5,
    deletions: 1,
    hasUnknownCounts: false,
    unfinished: 1
  })
})
