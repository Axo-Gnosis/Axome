/** An element's offset within the scroll viewport, with surviving ancestors as fallbacks. */
export interface ScrollAnchor {
  candidates: { element: HTMLElement; offset: number }[]
}

export function captureScrollAnchor(viewport: HTMLElement, content: HTMLElement): ScrollAnchor | null {
  const bounds = viewport.getBoundingClientRect()
  let element: HTMLElement | null = null
  // The top sample can land in a gap between messages. Try just below it before giving up.
  for (const offset of [8, 24, 48]) {
    const hit = viewport.ownerDocument.elementFromPoint(
      bounds.left + bounds.width / 2,
      bounds.top + offset
    ) as HTMLElement | null
    if (hit && hit !== content && content.contains(hit)) {
      element = hit
      break
    }
  }
  if (!element) return null
  const candidates: ScrollAnchor['candidates'] = []
  while (element && element !== content) {
    const offset = element.getBoundingClientRect().top - bounds.top
    candidates.push({ element, offset })
    element = element.parentElement
  }
  return candidates.length ? { candidates } : null
}

/** Anchor the first visible message/work section without moving its existing screen position. */
function captureExpansionAnchor(viewport: HTMLElement, content: HTMLElement): ScrollAnchor | null {
  const bounds = viewport.getBoundingClientRect()
  const blocks = content.querySelectorAll<HTMLElement>(
    '[data-slot="user-block"], :is([data-slot="agent-block"], [data-slot="turn-work"]) > [data-slot="message-response"], [data-slot="work-summary"], [data-slot="work-block"], [data-slot="background-band"]'
  )
  for (const block of blocks) {
    const rect = block.getBoundingClientRect()
    if (rect.bottom <= bounds.top || rect.top >= bounds.bottom) continue
    if (block.dataset.slot === 'work-block' && rect.top < bounds.top) {
      // Keep the reading point inside expanded work if it survives. Otherwise reveal the
      // owning work header, not a neighboring message or an ancestor far above the viewport.
      const reading = captureScrollAnchor(viewport, content)
      return {
        candidates: [
          ...(reading?.candidates.filter(({ element }) => element !== block && block.contains(element)) ?? []),
          { element: block, offset: 0 }
        ]
      }
    }
    return { candidates: [{ element: block, offset: rect.top - bounds.top }] }
  }
  return null
}

/** A layout change is not a new reading position. Keep the same anchor until explicit navigation. */
export class ExpansionAnchor {
  private anchor: ScrollAnchor | null = null

  capture(viewport: HTMLElement, content: HTMLElement): ScrollAnchor | null {
    if (!this.anchor?.candidates.some(({ element }) => element.isConnected && content.contains(element))) {
      this.anchor = captureExpansionAnchor(viewport, content)
    }
    return this.anchor
  }

  reset(): void {
    this.anchor = null
  }
}

/** A single toggle holds its header still; collapsing from deep inside output reveals that header. */
export function captureToggleAnchor(viewport: HTMLElement, target: HTMLElement): ScrollAnchor {
  return {
    candidates: [
      {
        element: target,
        offset: Math.max(0, target.getBoundingClientRect().top - viewport.getBoundingClientRect().top)
      }
    ]
  }
}

export function restoreScrollAnchor(viewport: HTMLElement, anchor: ScrollAnchor): void {
  const candidate = anchor.candidates.find(
    ({ element }) => element.isConnected && element.getClientRects().length > 0 && viewport.contains(element)
  )
  if (!candidate) return
  const delta = candidate.element.getBoundingClientRect().top - viewport.getBoundingClientRect().top - candidate.offset
  if (Math.abs(delta) > 0.5) viewport.scrollTop += delta
}
