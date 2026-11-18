import test from 'node:test'
import assert from 'node:assert/strict'
import type { ToolStep } from '../../turns'
import { toolHeader } from './toolHeader'
import { parseSearchDocs, parseWebFetch, parseWebSearch } from './webOutput'

function step(name: string, args: unknown, details: unknown): ToolStep {
  return {
    id: 'tool',
    entryId: 'entry',
    timestamp: null,
    endedAt: null,
    kind: 'tool',
    callId: 'call',
    name,
    arguments: args,
    status: 'done',
    output: [{ type: 'text', text: '' }],
    details,
    result: {
      kind: 'toolResult',
      callId: 'call',
      toolName: name,
      content: [{ type: 'text', text: '' }],
      details,
      isError: false,
      deliverables: [],
      durationMs: 120
    }
  }
}

test('parses web_search, web_fetch, and search_docs details and headers', () => {
  const ws = step(
    'web_search',
    { query: 'rust reqwest' },
    {
      provider: 'parallel',
      hits: [{ title: 'Reqwest', url: 'https://docs.rs/reqwest/latest' }],
      fallbackFrom: [{ provider: 'tavily', reason: 'timeout' }],
      notes: []
    }
  )
  const wsParsed = parseWebSearch(ws)
  assert.equal(wsParsed?.provider, 'parallel')
  assert.equal(wsParsed?.fallbackCount, 1)
  assert.equal(wsParsed?.hits.length, 1)
  assert.equal(toolHeader(ws, '/repo').meta, '1 result · parallel · fallback · 120ms')

  const wf = step(
    'web_fetch',
    { url: 'https://docs.rs/reqwest/latest/reqwest/' },
    {
      provider: 'parallel',
      url: 'https://docs.rs/reqwest/latest/reqwest/',
      savedLocalUrl: 'local://web/docs-rs-123.md',
      truncated: true
    }
  )
  const wfParsed = parseWebFetch(wf)
  assert.equal(wfParsed?.savedLocalUrl, 'local://web/docs-rs-123.md')
  assert.equal(toolHeader(wf, '/repo').meta, 'parallel · saved · 120ms')

  const sd = step(
    'search_docs',
    { query: 'Client', source: 'https://docs.rs/reqwest/llms.txt' },
    {
      mode: 'search',
      source: 'https://docs.rs/reqwest/llms.txt',
      loose: false,
      hits: [
        {
          source: 'https://docs.rs/reqwest/llms.txt',
          crumb: 'Client > Builder',
          path: 'doc://docs-rs-abc/0001-client.md',
          docUrl: 'doc://docs-rs-abc/0001-client.md',
          startLine: 10,
          endLine: 25,
          webUrl: 'https://docs.rs/reqwest/client.md#builder',
          snippet: 'Builds an HTTP client.'
        }
      ]
    }
  )
  const sdParsed = parseSearchDocs(sd)
  assert.equal(sdParsed?.mode, 'search')
  assert.equal(toolHeader(sd, '/repo').meta, '1 section · 120ms')
})
