import { getContext, setContext } from 'svelte'
import { eventStroke } from '$lib/features/command-palette/shortcuts/spec'
import { chatCommandsFor } from './bindings.svelte'
import type { ChatCommandId } from './commands'
import { fieldKind, reservedByField, type FieldKind } from './fields'

export interface KeyContext {
  stroke: string
  /** The text field the key landed in. */
  field: FieldKind
  /** Whether that field keeps the stroke for typing (see `reservedByField`). */
  reserved: boolean
}

/** Returns false to pass the key on; anything else consumes it. */
export type KeyHandler = (event: KeyboardEvent, context: KeyContext) => boolean | void

export interface KeyScope {
  /** Also receive strokes a focused text field keeps; the handlers decide what typing still owns. */
  text?: boolean
  commands?: Partial<Record<ChatCommandId, KeyHandler>>
  /** Commands whose keys this scope's own widget handles: dispatch ends here and the key stays native. */
  native?: readonly ChatCommandId[]
  /** Printable keys no command claimed, typed outside any text field. */
  type?: (key: string) => boolean | void
  /** Where dispatch starts while focus rests on the scope element itself (the transcript's highlight). */
  origin?: () => Element | null
}

const NATIVE_CONTROL = 'button, a[href], summary, select, input, [role="button"]'

/**
 * The chat view's one keyboard listener. Scopes register on elements; a keydown walks from the
 * focused element out through the scopes around it, offering each the commands its stroke is bound
 * to (chat keymap, user-rebindable), innermost first. Unclaimed keys stay native.
 */
export class ChatKeys {
  #scopes = new WeakMap<Element, () => KeyScope>()

  /** Attachment: makes the element a key scope. */
  scope(get: () => KeyScope) {
    return (element: HTMLElement) => {
      this.#scopes.set(element, get)
      element.dataset.keyScope = ''
      return () => {
        if (this.#scopes.get(element) === get) this.#scopes.delete(element)
        delete element.dataset.keyScope
      }
    }
  }

  /** Attachment for the chat view's root: capture phase, so scopes run before any native handling. */
  listen = (root: HTMLElement) => {
    const onKeyDown = (event: KeyboardEvent) => this.#dispatch(event)
    root.addEventListener('keydown', onKeyDown, { capture: true })
    return () => root.removeEventListener('keydown', onKeyDown, { capture: true })
  }

  #dispatch(event: KeyboardEvent) {
    if (event.defaultPrevented || event.isComposing || !(event.target instanceof Element)) return
    const stroke = eventStroke(event)
    if (!stroke) return
    const target = event.target
    const field = fieldKind(target)
    // Enter and Space on buttons, links and checkboxes keep their native activation.
    if ((stroke === 'enter' || stroke === 'space') && target.matches(NATIVE_CONTROL) && field === 'none') return
    const context: KeyContext = { stroke, field, reserved: reservedByField(stroke, field) }
    const commands = chatCommandsFor(stroke)
    const chain = this.#chain(target)
    for (const scope of chain) {
      if (context.reserved && !scope.text) continue
      for (const name of commands) {
        if (scope.native?.includes(name)) return
        const handler = scope.commands?.[name]
        if (handler && handler(event, context) !== false) return consume(event)
      }
    }
    const printable = event.key.length === 1 && !event.ctrlKey && !event.altKey && !event.metaKey
    if (!printable || field !== 'none' || target.matches(NATIVE_CONTROL)) return
    for (const scope of chain) if (scope.type && scope.type(event.key) !== false) return consume(event)
  }

  #chain(target: Element): KeyScope[] {
    const chain: KeyScope[] = []
    for (let element = target.closest('[data-key-scope]'); element;) {
      const scope = this.#scopes.get(element)?.()
      if (scope) {
        const origin = element === target && !chain.length ? scope.origin?.() : null
        if (origin && origin !== element && element.contains(origin)) return this.#chain(origin)
        chain.push(scope)
      }
      element = element.parentElement?.closest('[data-key-scope]') ?? null
    }
    return chain
  }
}

function consume(event: KeyboardEvent) {
  event.preventDefault()
  event.stopPropagation()
}

const KEY = Symbol('chat-keys')

export function setChatKeys(keys: ChatKeys): ChatKeys {
  return setContext(KEY, keys)
}

export function getChatKeys(): ChatKeys {
  const keys = getContext<ChatKeys | undefined>(KEY)
  if (!keys) throw new Error('Chat key scopes need a ChatKeys router from the chat view')
  return keys
}
