import { backend } from '$lib/api/backend'
import type { Range } from './textForm'

export interface TypedRun {
  before: string
  after: string
}

/**
 * Extracts the contiguous typed text run around `caret` in `text`, bounded by
 * non-typed `breaks` (resolved `@ref`s and paste ranges). Returns `null` when
 * the caret sits inside a break (including inside any `pastes` range).
 */
export function runAroundCaret(text: string, caret: number, breaks: Range[], pastes: Range[] = []): TypedRun | null {
  for (const paste of pastes) {
    if (caret > paste.from && caret < paste.to) return null
  }
  let start = 0
  let end = text.length
  for (const span of breaks) {
    if (caret > span.from && caret < span.to) return null
    if (span.to <= caret && span.to > start) start = span.to
    if (span.from >= caret && span.from < end) end = span.from
  }
  return {
    before: text.slice(start, caret),
    after: text.slice(caret, end)
  }
}

/**
 * Serializes `predict_complete` requests so at most one is in flight at a time,
 * coalescing rapid keystrokes to the latest draft and discarding stale replies.
 */
export function createPredictRequester(onResult: (run: TypedRun, text: string | null) => void) {
  let generation = 0
  let inFlight = false
  let queued: { run: TypedRun; generation: number } | null = null

  async function pump() {
    if (inFlight) return
    inFlight = true
    try {
      while (queued) {
        const next = queued
        queued = null
        let text: string | null = null
        try {
          const suggestion = await backend.predict.complete(next.run.before, next.run.after)
          text = suggestion?.text ?? null
        } catch {
          text = null
        }
        if (next.generation === generation) {
          onResult(next.run, text)
        }
      }
    } finally {
      inFlight = false
    }
  }

  return {
    request(run: TypedRun) {
      if (!run.before) {
        this.clear()
        return
      }
      queued = { run, generation: ++generation }
      void pump()
    },
    clear() {
      generation++
      queued = null
    }
  }
}
