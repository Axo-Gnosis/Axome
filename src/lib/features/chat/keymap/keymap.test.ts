import { test } from 'node:test'
import assert from 'node:assert/strict'
import { eventStroke, formatShortcut, normalizeShortcut } from '$lib/features/command-palette/shortcuts/spec'
import { CHAT_COMMANDS, chatBindingsOverlap, chatCommandNames } from './commands'
import { reservedByField } from './fields'

test('text fields keep typing and caret keys; commands keep the rest', () => {
  const cases: [stroke: string, field: 'none' | 'line' | 'multiline', reserved: boolean][] = [
    ['j', 'line', true],
    ['shift+j', 'multiline', true],
    ['space', 'line', true],
    ['backspace', 'line', true],
    ['arrowleft', 'line', true],
    ['home', 'multiline', true],
    // A single line has no lines to move between: up/down and Enter go to navigation.
    ['arrowup', 'line', false],
    ['enter', 'line', false],
    ['arrowup', 'multiline', true],
    ['enter', 'multiline', true],
    ['shift+enter', 'multiline', true],
    ['escape', 'multiline', false],
    ['tab', 'line', false],
    ['pageup', 'multiline', false],
    ['alt+t', 'multiline', false],
    ['ctrl+enter', 'multiline', false],
    ['ctrl++', 'line', false],
    ['+', 'line', true],
    ['j', 'none', false]
  ]
  for (const [stroke, field, reserved] of cases)
    assert.equal(reservedByField(stroke, field), reserved, `${stroke} in ${field}`)
})

test('keydowns encode to strokes that format back', () => {
  const cases: [event: Partial<KeyboardEvent>, stroke: string | null, label?: string][] = [
    // WebKitGTK reports Shift+Tab as `Unidentified`; the physical code recovers it.
    [{ key: 'Unidentified', code: 'Tab', shiftKey: true }, 'shift+tab', 'Shift+Tab'],
    [{ key: ' ', code: 'Space' }, 'space', 'Space'],
    // `+` already implies Shift on most layouts, so Shift is not part of the stroke.
    [{ key: '+', code: 'Equal', shiftKey: true }, '+', '+'],
    [{ key: 'k', code: 'KeyK', metaKey: true }, 'ctrl+k', 'Ctrl+K'],
    [{ key: 'Shift', code: 'ShiftLeft', shiftKey: true }, null]
  ]
  for (const [event, stroke, label] of cases) {
    assert.equal(eventStroke(event as KeyboardEvent), stroke, `${event.key}`)
    if (stroke) assert.equal(formatShortcut(stroke), label, stroke)
  }
})

test('default chat bindings never collide where both can fire', () => {
  for (const [index, a] of chatCommandNames.entries()) {
    for (const b of chatCommandNames.slice(index + 1)) {
      if (!chatBindingsOverlap(CHAT_COMMANDS[a].context, CHAT_COMMANDS[b].context)) continue
      const shared = CHAT_COMMANDS[a].keys.filter((key) =>
        CHAT_COMMANDS[b].keys.some((other) => normalizeShortcut(other) === normalizeShortcut(key))
      )
      assert.deepEqual(shared, [], `${a} and ${b}`)
    }
  }
})
