import { getContext, setContext } from 'svelte'
import { parseShortcutStroke } from '$lib/features/command-palette/shortcuts/spec'
import type { FieldKind } from './fields'
import type { KeyContext, KeyHandler, KeyScope } from './router.svelte'

/** What a stop does with the navigation commands; all optional. */
export interface StopActions {
  /** Enter, or Enter inside its field (`editing`). */
  activate?: (editing: boolean) => void
  /** Space: select without confirming. */
  toggle?: () => void
  /** Side arrows. */
  step?: (direction: -1 | 1) => void
  /** The field this stop edits in place: typing on the highlighted stop starts editing it. */
  field?: () => HTMLInputElement | HTMLTextAreaElement | null
  /** Enter inside a multi-line field activates instead of adding a line. */
  submitOnEnter?: boolean
}

const STOPS = '[data-nav-stop]'

/** Highlight for a stop: the design system's keyboard-focus ring, set via `data-nav-active`. */
export const NAV_ACTIVE = 'data-[nav-active]:shadow-focus-ring'

/** Highlight for a stop wrapping a field: the field looks focused (its child's `focus:border-accent`). */
export const NAV_FIELD_ACTIVE = '[&[data-nav-active]>*]:border-accent'

/**
 * Keyboard position inside a transcript. Stops are elements marked with `stop()` (turns, and the
 * items of open questions and reviews), in document order. Focus rests on the transcript element and
 * the highlight is drawn from state, so it never depends on the browser deciding to show focus; real
 * focus only enters a stop's field while editing it. Scrolling goes through the conversation's
 * `reveal`, which moves to a turn exactly as clicking its rail dot does; the dot is a turn's only
 * highlight.
 */
export class TranscriptNav {
  active = $state<string | null>(null)
  /** The highlight shows while focus is in the transcript and the last move came from the keyboard. */
  shown = $state(false)
  #focused = $state(false)
  #root: HTMLElement | null = null
  #reveal: (element: HTMLElement) => void = () => {}
  #actions = new Map<string, () => StopActions>()
  #lastIndex = -1
  readonly #exit: () => void

  constructor(exit: () => void) {
    this.#exit = exit
  }

  get visible(): boolean {
    return this.shown && this.#focused
  }

  /** Attachment: marks a navigation stop. */
  stop(id: string, actions: () => StopActions = () => ({})) {
    return (element: HTMLElement) => {
      element.dataset.navStop = id
      this.#actions.set(id, actions)
      $effect(() => {
        element.toggleAttribute('data-nav-active', this.visible && this.active === id)
      })
      return () => {
        if (this.#actions.get(id) === actions) this.#actions.delete(id)
        element.removeAttribute('data-nav-active')
      }
    }
  }

  /** Attachment for the transcript element, from the conversation that owns its scrolling. */
  mount(reveal: (element: HTMLElement) => void) {
    return (root: HTMLElement) => {
      this.#root = root
      this.#reveal = reveal
      const events = new AbortController()
      const { signal } = events
      root.addEventListener(
        'focusin',
        (event) => {
          this.#focused = true
          const stop = event.target instanceof Element && event.target !== root ? event.target.closest(STOPS) : null
          if (stop instanceof HTMLElement && root.contains(stop)) this.#mark(stop)
        },
        { signal }
      )
      // Read after the fact: focusout also fires while a render removes the focused element.
      root.addEventListener(
        'focusout',
        () => queueMicrotask(() => (this.#focused = root.contains(document.activeElement))),
        { signal }
      )
      // A click moves the position silently. Stops are not focusable, so the click itself leaves
      // focus on the transcript (or the field clicked into) and the keyboard carries on from there.
      root.addEventListener(
        'pointerdown',
        (event) => {
          this.shown = false
          const stop = event.target instanceof Element ? event.target.closest(STOPS) : null
          if (stop instanceof HTMLElement && root.contains(stop)) this.#mark(stop)
        },
        { signal }
      )
      return () => {
        events.abort()
        if (this.#root === root) this.#root = null
      }
    }
  }

  activeElement(): HTMLElement | null {
    if (!this.#root || this.active === null) return null
    return this.#root.querySelector<HTMLElement>(`[data-nav-stop="${CSS.escape(this.active)}"]`)
  }

  /** Moves the highlight to `stop` and the keyboard to the transcript; `edit` puts focus in its field. */
  focus(stop: HTMLElement, { edit = false, quiet = false } = {}) {
    if (!this.#root) return
    this.#mark(stop)
    this.shown = !quiet
    const field = edit ? this.#actions.get(stop.dataset.navStop!)?.().field?.() : null
    ;(field ?? this.#root).focus({ preventScroll: true })
    this.#reveal(stop)
  }

  /**
   * From outside: highlight the first stop matching `selector` inside the element `within` selects,
   * else the last stop. Nothing moves when `within` is given but absent.
   */
  enter(within?: string, selector = STOPS) {
    const container = within ? this.#root?.querySelector(within) : null
    if (within && !container) return
    const stop = container?.querySelector<HTMLElement>(selector) ?? container?.querySelector<HTMLElement>(STOPS)
    const target = stop && this.#visible(stop) ? stop : this.#stops().at(-1)
    if (target) this.focus(target)
  }

  /** The transcript's key scope: navigation commands, editing in place, and leaving. */
  readonly keys: KeyScope = {
    text: true,
    origin: () => this.activeElement(),
    commands: {
      previous: this.#moveKey(-1),
      next: this.#moveKey(1),
      first: this.#edgeKey(0),
      last: this.#edgeKey(-1),
      activate: (event, context) => {
        if (this.#foreign(event, context)) return false
        const field = this.#editing(event)
        const { activate, submitOnEnter } = this.#current()
        if (!activate || (field && context.reserved && !submitOnEnter)) return false
        activate(!!field)
      },
      toggle: (event, context) => {
        const { toggle } = this.#current()
        if (context.field !== 'none' || !toggle) return false
        toggle()
      },
      decrease: this.#stepKey(-1),
      increase: this.#stepKey(1),
      leave: (event, context) => {
        if (this.#foreign(event, context)) return false
        if (this.#editing(event)) {
          this.#root?.focus({ preventScroll: true })
          return
        }
        const parent = this.activeElement()?.parentElement?.closest<HTMLElement>(STOPS)
        if (parent && this.#root?.contains(parent)) this.focus(parent)
        else this.#exit()
      }
    },
    type: (key) => {
      const field = this.#current().field?.()
      if (!field) return false
      field.focus({ preventScroll: true })
      const start = field.selectionStart ?? field.value.length
      field.setRangeText(key, start, field.selectionEnd ?? start, 'end')
      field.dispatchEvent(new Event('input', { bubbles: true }))
    }
  }

  /** The highlighted stop's actions. */
  #current(): StopActions {
    return this.active === null ? {} : (this.#actions.get(this.active)?.() ?? {})
  }

  /** The highlighted stop's own field when focus is in it (the stop is being edited); null on the transcript. */
  #editing(event: KeyboardEvent) {
    const target = event.target
    if (!(target instanceof HTMLElement) || target === this.#root) return null
    const field = this.#current().field?.()
    return field && field === target ? field : undefined
  }

  /** Keys in text fields that are not the highlighted stop's (an inline message edit) stay theirs. */
  #foreign(event: KeyboardEvent, context: KeyContext): boolean {
    return context.field !== 'none' && this.#editing(event) === undefined
  }

  #moveKey(delta: -1 | 1): KeyHandler {
    return (event, context) => {
      if (this.#foreign(event, context)) return false
      const field = this.#editing(event)
      if (field && context.reserved && !atEdge(field, context.field, parseShortcutStroke(context.stroke)?.key))
        return false
      this.#move(delta)
    }
  }

  /** Home/End: the stop at `index` (negative counts from the end); fields keep the keys. */
  #edgeKey(index: number): KeyHandler {
    return (event, context) => {
      if (context.field !== 'none') return false
      const stop = this.#stops().at(index)
      if (stop) this.focus(stop)
    }
  }

  #stepKey(direction: -1 | 1): KeyHandler {
    return (event, context) => {
      const { step } = this.#current()
      if (context.field !== 'none' || !step) return false
      step(direction)
    }
  }

  #mark(stop: HTMLElement) {
    this.active = stop.dataset.navStop ?? null
    // Assistive tech follows the highlight the way it would follow focus.
    if (stop.id) this.#root?.setAttribute('aria-activedescendant', stop.id)
    else this.#root?.removeAttribute('aria-activedescendant')
    const index = this.#stops().indexOf(stop)
    if (index >= 0) this.#lastIndex = index
  }

  #visible(stop: Element): boolean {
    return stop.getClientRects().length > 0
  }

  #stops(): HTMLElement[] {
    if (!this.#root) return []
    return Array.from(this.#root.querySelectorAll<HTMLElement>(STOPS)).filter((stop) => this.#visible(stop))
  }

  #move(delta: -1 | 1) {
    const stops = this.#stops()
    const current = this.activeElement()
    let at = current ? stops.indexOf(current) : -1
    // The highlighted stop went away (a question was answered): continue from where it was.
    if (at < 0 && this.#lastIndex >= 0) at = Math.min(this.#lastIndex, stops.length) - (delta > 0 ? 1 : 0)
    else if (at < 0) at = delta > 0 ? -1 : stops.length
    const next = stops[at + delta]
    if (next) this.focus(next)
    else if (delta > 0) this.#exit()
  }
}

/** Whether the caret sits at the edge a vertical arrow would leave a multi-line field through. */
function atEdge(field: HTMLInputElement | HTMLTextAreaElement, kind: FieldKind, key: string | undefined): boolean {
  if (kind !== 'multiline') return true
  const { selectionStart, selectionEnd, value } = field
  if (selectionStart !== selectionEnd) return false
  if (key === 'arrowup') return !value.slice(0, selectionStart ?? 0).includes('\n')
  if (key === 'arrowdown') return !value.slice(selectionEnd ?? 0).includes('\n')
  return false
}

const KEY = Symbol('transcript-nav')

export function setTranscriptNav(nav: TranscriptNav): TranscriptNav {
  return setContext(KEY, nav)
}

export function getTranscriptNav(): TranscriptNav {
  const nav = getContext<TranscriptNav | undefined>(KEY)
  if (!nav) throw new Error('Transcript stops need the conversation’s TranscriptNav')
  return nav
}
