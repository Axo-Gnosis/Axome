import assert from 'node:assert/strict'
import test from 'node:test'
import type { ToolStep } from '$lib/features/chat/turns'
import { browserHeaderSummary, parseBrowserOutput } from './browserOutput'

function browserCall(arguments_: Record<string, unknown>, details?: Record<string, unknown>): ToolStep {
  return {
    id: 'b1',
    entryId: 'entry-1',
    timestamp: null,
    endedAt: null,
    kind: 'tool',
    callId: 'c1',
    name: 'browser',
    arguments: arguments_,
    status: 'done',
    output: [{ type: 'text', text: 'ok' }],
    details: details ?? null,
    result: {
      kind: 'toolResult',
      callId: 'c1',
      toolName: 'browser',
      content: [{ type: 'text', text: 'ok' }],
      details: details ?? null,
      isError: false,
      deliverables: [],
      durationMs: 120
    }
  }
}

test('formats single browser action with URL and error counts', () => {
  const call = browserCall(
    { action: 'open', url: 'http://localhost:5173/dashboard' },
    {
      action: 'open',
      steps: ['open'],
      tab: 'main',
      url: 'http://localhost:5173/dashboard',
      title: 'Dashboard',
      consoleErrors: [{ level: 'error', text: 'TypeError' }],
      failedRequests: [{ method: 'GET', url: 'http://localhost:5173/api', status: 500, failure: null }],
      hasScreenshot: false
    }
  )

  const header = browserHeaderSummary(call)
  assert.match(header.summary, /^open localhost:5173/)
  assert.equal(header.detail, '1 console error · 1 failed request')

  const parsed = parseBrowserOutput(call)
  assert.ok(parsed)
  assert.equal(parsed.title, 'Dashboard')
  assert.equal(parsed.consoleErrors.length, 1)
  assert.equal(parsed.failedRequests[0]?.status, 500)
})

test('formats batched browser steps cleanly', () => {
  const call = browserCall(
    {
      steps: [
        { action: 'open', url: 'http://localhost:5173' },
        { action: 'click', target: '@e4' },
        { action: 'screenshot' }
      ]
    },
    {
      action: 'open',
      steps: ['open', 'click', 'screenshot'],
      tab: 'preview',
      url: 'http://localhost:5173',
      title: 'App',
      consoleErrors: [],
      failedRequests: [],
      hasScreenshot: true
    }
  )

  const header = browserHeaderSummary(call)
  assert.equal(header.summary, '3 steps (open -> click -> screenshot)')
  assert.equal(header.detail, 'preview · screenshot')
})
