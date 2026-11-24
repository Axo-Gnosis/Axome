import { logicalKey } from '$lib/utils/keyboardEvent'

export interface ShortcutStroke {
  ctrl: boolean
  shift: boolean
  alt: boolean
  key: string
}

export function splitShortcutSequence(spec: string | undefined): string[][] {
  if (!spec) return []
  return spec
    .trim()
    .split(/\s+/)
    .filter(Boolean)
    .map(splitShortcutParts)
    .filter((parts) => parts.length > 0)
}

export function splitShortcutParts(spec: string): string[] {
  const trimmed = spec.trim()
  if (!trimmed) return []
  // A lone `+` is the key itself, not a separator.
  if (trimmed === '+') return [trimmed]
  const lastPlus = trimmed.lastIndexOf('+', trimmed.length - 2)
  if (lastPlus < 0) return [trimmed]
  const head = trimmed.slice(0, lastPlus)
  const tail = trimmed.slice(lastPlus + 1)
  const mods = head
    .split('+')
    .map((part) => part.trim())
    .filter(Boolean)
  return [...mods, tail.trim()]
}

export function parseShortcutStroke(spec: string): ShortcutStroke | null {
  const parts = splitShortcutParts(spec)
  if (parts.length === 0) return null
  const stroke: ShortcutStroke = { ctrl: false, shift: false, alt: false, key: '' }
  for (const rawPart of parts) {
    const part = rawPart.toLowerCase()
    if (part === 'ctrl' || part === 'control' || part === 'cmd' || part === 'meta') stroke.ctrl = true
    else if (part === 'shift') stroke.shift = true
    else if (part === 'alt' || part === 'option') stroke.alt = true
    else stroke.key = normalizeShortcutKey(rawPart)
  }
  return stroke.key ? stroke : null
}

export function parseShortcut(spec: string): ShortcutStroke[] {
  return spec
    .trim()
    .split(/\s+/)
    .map(parseShortcutStroke)
    .filter((stroke): stroke is ShortcutStroke => stroke !== null)
}

export function normalizeShortcut(spec: string): string {
  return parseShortcut(spec).map(normalizeStroke).join(' ')
}

/** Aliases and display labels users may type, mapped to the stored key name. */
const KEY_ALIASES: Record<string, string> = {
  esc: 'escape',
  return: 'enter',
  plus: '+',
  minus: '-',
  up: 'arrowup',
  down: 'arrowdown',
  left: 'arrowleft',
  right: 'arrowright',
  '↑': 'arrowup',
  '↓': 'arrowdown',
  '←': 'arrowleft',
  '→': 'arrowright',
  pgup: 'pageup',
  pgdn: 'pagedown'
}

const KEY_LABELS: Record<string, string> = {
  escape: 'Esc',
  enter: 'Enter',
  space: 'Space',
  arrowup: '↑',
  arrowdown: '↓',
  arrowleft: '←',
  arrowright: '→',
  pageup: 'PgUp',
  pagedown: 'PgDn'
}

export function normalizeShortcutKey(key: string): string {
  // The space bar's `key` is ' ', which trimming would erase.
  if (key === ' ') return 'space'
  const lower = key.trim().toLowerCase()
  return KEY_ALIASES[lower] ?? lower
}

/** The normalized stroke (`alt+ctrl+shift+key`) a keydown matches bindings by; null for a bare modifier. */
export function eventStroke(e: KeyboardEvent): string | null {
  // Resolve via `logicalKey` so WebKitGTK's `'Unidentified'` regression
  // on Shift-modified non-character keys (Tab, Enter, …) doesn't drop
  // user-bound shortcuts on the floor.
  const raw = logicalKey(e)
  if (raw === 'Control' || raw === 'Meta' || raw === 'Shift' || raw === 'Alt') return null
  const mods: string[] = []
  if (e.altKey) mods.push('alt')
  if (e.ctrlKey || e.metaKey) mods.push('ctrl')
  if (e.shiftKey && raw !== '+') mods.push('shift')
  const key = normalizeShortcutKey(raw.length === 1 ? raw.toLowerCase() : raw)
  if (!key) return null
  return [...mods, key].join('+')
}

export function normalizeStroke(stroke: ShortcutStroke): string {
  const mods: string[] = []
  if (stroke.alt) mods.push('alt')
  if (stroke.ctrl) mods.push('ctrl')
  if (stroke.shift) mods.push('shift')
  return [...mods, normalizeShortcutKey(stroke.key)].join('+')
}

export function formatShortcut(spec: string | undefined): string {
  return splitShortcutSequence(spec)
    .map((stroke) => stroke.map(formatShortcutPart).join('+'))
    .join(' ')
}

export function formatShortcutPart(part: string): string {
  const lower = part.toLowerCase()
  if (lower === 'ctrl' || lower === 'control' || lower === 'cmd' || lower === 'meta') return 'Ctrl'
  if (lower === 'shift') return 'Shift'
  if (lower === 'alt' || lower === 'option') return 'Alt'
  const label = KEY_LABELS[normalizeShortcutKey(part)]
  if (label) return label
  if (/^f\d+$/i.test(part)) return part.toUpperCase()
  if (part.length === 1) return part.toUpperCase()
  return part[0].toUpperCase() + part.slice(1)
}

export function splitShortcut(spec: string | undefined): string[] {
  const [firstStroke] = splitShortcutSequence(spec)
  return firstStroke?.map(formatShortcutPart) ?? []
}

export function shortcutEquals(a: string, b: string): boolean {
  return normalizeShortcut(a) === normalizeShortcut(b)
}
