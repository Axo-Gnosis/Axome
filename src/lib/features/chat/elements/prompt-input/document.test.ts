import test from 'node:test'
import assert from 'node:assert/strict'
import type { ChatCommand } from '$lib/types/backend'
import { commandArguments, parseCommandLine, type Builtin } from './document'

const builtin = (name: string, properties: Record<string, unknown>, required: string[] = []): ChatCommand => ({
  kind: 'builtin',
  name,
  description: '',
  policy: 'alongside',
  parameters: { type: 'object', properties, required }
})
const commands: ChatCommand[] = [
  builtin('shake', { mode: { type: 'string', enum: ['elide', 'images'] } }, ['mode']),
  builtin('note', { level: { type: 'integer' }, text: { type: 'string' } }),
  builtin('navigate', { entry: { type: 'string', format: 'entry' } }),
  { kind: 'skill', name: 'greet', description: '', policy: 'queued', argumentHint: null }
]
const parse = (text: string, caret = text.length) => parseCommandLine(text, caret, commands)

test('command lines are only offered built-ins followed by whitespace', () => {
  assert.equal(parse('/shake'), null)
  assert.equal(parse('/greet Alice'), null)
  assert.equal(parse('/navigate x'), null)
  assert.equal(parse('hello /shake images'), null)
})

test('arguments fill fields in order and the caret offers the next one', () => {
  const line = parse('/shake ')!
  assert.deepEqual(line.args, [])
  assert.deepEqual(line.missing, ['mode'])
  assert.deepEqual(
    { ...line.slot, field: undefined },
    { name: 'mode', field: undefined, from: 7, to: 7, text: '', last: true }
  )
  assert.deepEqual([parse('/shake im')!.slot?.from, parse('/shake im')!.slot?.to], [7, 9])
})

test('text past the last argument is flagged', () => {
  const line = parse('/shake images and more')!
  assert.deepEqual(
    line.args.map((arg) => arg.text),
    ['images']
  )
  assert.deepEqual(line.extra, { from: 14, to: 22, text: 'and more' })
})

test('a trailing free-text field takes the rest of the line', () => {
  const line = parse('/note 3 keep  this  verbatim')!
  assert.deepEqual(
    line.args.map((arg) => arg.text),
    ['3', 'keep  this  verbatim']
  )
  assert.equal(line.extra, null)
  const values = Object.fromEntries(line.args.map((arg) => [arg.name, arg.text]))
  assert.deepEqual(commandArguments(commands[1] as Builtin, values).args, { level: 3, text: 'keep  this  verbatim' })
})
