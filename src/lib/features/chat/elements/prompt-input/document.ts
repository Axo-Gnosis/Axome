import { Schema, type Node } from 'prosemirror-model'
import type { ChatCommand, ChatModel, ChatPart } from '$lib/types/backend'

// Marks chips this composer serialized; chip HTML without it (a web page, another window) pastes as text.
export const chipOrigin = crypto.randomUUID()

export const promptSchema = new Schema({
  nodes: {
    doc: { content: 'paragraph' },
    paragraph: { content: 'inline*', whitespace: 'pre', toDOM: () => ['p', 0], parseDOM: [{ tag: 'p' }] },
    text: { group: 'inline' },
    // One atomic inline node per chip: it moves, deletes and copies as a single character.
    // Not selectable, so a click places the caret beside it rather than selecting it.
    part: {
      inline: true,
      group: 'inline',
      atom: true,
      selectable: false,
      attrs: { part: {} },
      toDOM: (node) => [
        'span',
        { 'data-prompt-part': JSON.stringify(node.attrs.part), 'data-prompt-origin': chipOrigin }
      ],
      parseDOM: [
        {
          tag: 'span[data-prompt-part]',
          getAttrs: (dom) => {
            if ((dom as HTMLElement).dataset.promptOrigin !== chipOrigin) return false
            try {
              return { part: JSON.parse((dom as HTMLElement).dataset.promptPart ?? '') as ChatPart }
            } catch {
              return false
            }
          }
        }
      ]
    }
  }
})

export function partsDocument(parts: ChatPart[]): Node {
  return promptSchema.node(
    'doc',
    null,
    promptSchema.node(
      'paragraph',
      null,
      parts.flatMap((part) =>
        part.type === 'text' ? (part.text ? [promptSchema.text(part.text)] : []) : [promptSchema.node('part', { part })]
      )
    )
  )
}

export function documentParts(doc: Node): ChatPart[] {
  const parts: ChatPart[] = []
  doc.firstChild?.forEach((node) => {
    parts.push(node.isText ? { type: 'text', text: node.text! } : (node.attrs.part as ChatPart))
  })
  return parts
}

export type Builtin = Extract<ChatCommand, { kind: 'builtin' }>
export type Field = { type?: string; enum?: unknown[]; format?: string; description?: string; default?: unknown }
export function commandFields(command: Builtin): [string, Field][] {
  return Object.entries((command.parameters.properties ?? {}) as Record<string, Field>)
}
export function offeredCommand(command: ChatCommand): boolean {
  return command.kind !== 'builtin' || !commandFields(command).some(([, field]) => field.format === 'entry')
}
export type Choice = { value: string; label: string; detail?: string }
/** The values a field accepts, offered as a list; null for free-form fields. */
export function fieldChoices(field: Field, models: ChatModel[]): Choice[] | null {
  if (field.enum) return field.enum.map((option) => ({ value: String(option), label: String(option) }))
  if (field.format === 'model')
    return models.map((model) => ({ value: model.selector, label: model.name, detail: model.selector }))
  if (field.type === 'boolean')
    return [
      { value: 'true', label: 'true' },
      { value: 'false', label: 'false' }
    ]
  return null
}

/** The composer's text with each chip as one `\ufffc`; offset `i` is document position `i + 1`. */
export function documentText(doc: Node): string {
  return doc.textBetween(1, doc.content.size - 1, '', '\ufffc')
}
export type CommandArgument = { name: string; field: Field; from: number; to: number; text: string; last: boolean }
export type CommandLine = {
  command: Builtin
  /** End of `/name`. */
  nameEnd: number
  args: CommandArgument[]
  /** Text past the last argument the command takes. */
  extra: { from: number; to: number; text: string } | null
  missing: string[]
  /** The argument under the caret, empty when the caret sits where the next one would start. */
  slot: CommandArgument | null
}
/**
 * Reads `/name arg…` as a built-in command. Arguments are whitespace-separated and fill the
 * command's fields in order; a final free-text field takes the rest of the line.
 */
export function parseCommandLine(text: string, caret: number, commands: ChatCommand[]): CommandLine | null {
  const head = /^\/(\S+)(?=\s)/.exec(text)
  const command = head ? commands.find((item) => item.name === head[1]) : undefined
  if (!head || command?.kind !== 'builtin' || !offeredCommand(command)) return null
  const fields = commandFields(command)
  const tokens = [...text.slice(head[0].length).matchAll(/\S+/g)].map((match) => ({
    from: head[0].length + match.index,
    to: head[0].length + match.index + match[0].length
  }))
  const args: CommandArgument[] = []
  let extra: CommandLine['extra'] = null
  for (const [index, token] of tokens.entries()) {
    const field = fields[index]
    if (!field) {
      extra = { from: token.from, to: tokens.at(-1)!.to, text: text.slice(token.from, tokens.at(-1)!.to) }
      break
    }
    const last = index === fields.length - 1
    const to = last && field[1].type === 'string' && fieldChoices(field[1], []) === null ? tokens.at(-1)!.to : token.to
    args.push({ name: field[0], field: field[1], from: token.from, to, text: text.slice(token.from, to), last })
    if (to !== token.to) break
  }
  let slot = args.find((arg) => arg.from <= caret && caret <= arg.to) ?? null
  if (!slot && caret > head[0].length && !/\S/.test(text[caret] ?? '')) {
    const index = args.filter((arg) => arg.to < caret).length
    const field = fields[index]
    if (field)
      slot = { name: field[0], field: field[1], from: caret, to: caret, text: '', last: index === fields.length - 1 }
  }
  return {
    command,
    nameEnd: head[0].length,
    args,
    extra,
    missing: fields.slice(args.length).map(([name]) => name),
    slot
  }
}
export function commandArguments(
  command: Builtin,
  values: Record<string, string>
): { args: Record<string, unknown>; invalid?: string; message?: string } {
  const args: Record<string, unknown> = {}
  const required = (command.parameters.required ?? []) as string[]
  for (const [name, field] of commandFields(command)) {
    const value = values[name]?.trim()
    if (!value) {
      if (required.includes(name)) return { args, invalid: name, message: `${name} is required.` }
      continue
    }
    const parsed =
      field.type === 'integer'
        ? Number(value)
        : field.type === 'boolean'
          ? value === 'true'
            ? true
            : value === 'false'
              ? false
              : undefined
          : (field.enum?.find((option) => String(option) === value) ?? value)
    if (
      (field.type === 'integer' && !Number.isSafeInteger(parsed)) ||
      (field.type === 'boolean' && parsed === undefined) ||
      (field.enum && !field.enum.includes(parsed))
    ) {
      return { args, invalid: name, message: `Choose a valid ${name}.` }
    }
    args[name] = parsed
  }
  return { args }
}
