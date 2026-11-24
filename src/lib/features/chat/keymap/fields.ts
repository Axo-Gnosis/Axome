import { parseShortcutStroke } from '$lib/features/command-palette/shortcuts/spec'

/** The kind of text field a key lands in: none, single-line, or multi-line (textarea, rich input). */
export type FieldKind = 'none' | 'line' | 'multiline'

export function fieldKind(target: Element): FieldKind {
  if (target instanceof HTMLTextAreaElement || (target instanceof HTMLElement && target.isContentEditable))
    return 'multiline'
  if (target instanceof HTMLInputElement && !['checkbox', 'radio', 'button', 'submit', 'reset'].includes(target.type))
    return 'line'
  return 'none'
}

/** Keys every text field needs for typing and moving its caret. */
const CARET_KEYS = new Set(['space', 'backspace', 'delete', 'arrowleft', 'arrowright', 'home', 'end'])
/** Keys only a multi-line field needs: line movement and new lines. */
const LINE_KEYS = new Set(['arrowup', 'arrowdown', 'enter'])

/**
 * Whether a focused field keeps this stroke for text editing. Kept strokes only reach scopes that
 * understand text fields (`KeyScope.text`), so a binding like `j` for Next never eats typing.
 */
export function reservedByField(stroke: string, field: FieldKind): boolean {
  if (field === 'none') return false
  const parsed = parseShortcutStroke(stroke)
  if (!parsed || parsed.alt || parsed.ctrl) return false
  const { key } = parsed
  return key.length === 1 || CARET_KEYS.has(key) || (field === 'multiline' && LINE_KEYS.has(key))
}
