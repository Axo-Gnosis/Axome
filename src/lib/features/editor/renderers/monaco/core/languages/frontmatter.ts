// Completion, hover and diagnostics for the YAML frontmatter of axo agent,
// mode, keyword and skill files. Hand-written line scanning: Monaco has no
// YAML worker here and the frontmatter grammar these files use is flat.
// axo still validates on load; these markers only catch mistakes early.

import type * as MonacoModule from 'monaco-editor'
import { modelHostPath } from '$lib/features/editor/renderers/monaco/text/modelCache'

type Monaco = typeof MonacoModule
type Model = MonacoModule.editor.ITextModel

type FrontmatterKind = 'agent' | 'mode' | 'keyword' | 'skill'

interface FrontmatterKey {
  description: string
  values?: readonly string[]
  boolean?: boolean
  children?: Readonly<Record<string, FrontmatterKey>>
}

const BASH_VALUES = ['full', 'readOnly'] as const

/** Frontmatter keys per definition kind; drives completion, hover and diagnostics. */
const FRONTMATTER_SCHEMA: Readonly<Record<FrontmatterKind, Readonly<Record<string, FrontmatterKey>>>> = {
  agent: {
    name: { description: "The agent's unique catalog name." },
    description: { description: 'One line for listings; may be empty.' },
    runAs: {
      description: 'Root persona, spawned agent, or both (the default).',
      values: ['root', 'subagent', 'any']
    },
    hidden: {
      description: "Leaves the agent out of the spawn tool's listing; spawning by name and team membership still work.",
      boolean: true
    },
    model: { description: 'A model selector (`[@role | provider/model][:thinking]`).' },
    tools: { description: 'Tool allowlist over built-in and host tools; absent keeps every tool.' },
    contextFiles: {
      description:
        '`"nested"` loads no startup files and attaches files below the project root on first touch; `false` disables context files; a map overrides names in `settings.contextFiles`; absent keeps the setting (`subagents.contextFiles`, default `"nested"`, for spawned agents).'
    },
    sections: { description: 'Prompt sections by name: `false` removes the section, `true` changes nothing.' },
    output: {
      description: "JSON Schema of a child's result, returned through `yield`; ignored for root sessions."
    },
    maxTurns: {
      description: 'Model requests per child run before the wrap-up turn; absent uses `subagents.maxTurns`.'
    },
    bash: {
      description: '`readOnly` confines `bash` writes, here and in every descendant this agent spawns.',
      values: BASH_VALUES
    },
    writes: {
      description:
        'Where `write` and `edit` may write: scheme prefixes like `local://` and directories; `[]` allows nothing, absent allows everything. Descendants inherit it as a ceiling.'
    },
    members: { description: 'The ordered roster of a team; its presence makes this file a team definition.' }
  },
  mode: {
    name: { description: "The mode's unique name; `/name` enters it." },
    description: { description: 'One line for listings.' },
    'argument-hint': { description: 'Placeholder a host shows after `/name`.' },
    'disable-model-invocation': {
      description: "`true` keeps the mode out of the model's reach; `/name` still works.",
      boolean: true
    },
    model: { description: 'A model selector applied on entry.' },
    tools: {
      description: 'Tool limits while the mode is active.',
      children: {
        allow: { description: 'When set, only these tools are allowed.' },
        deny: { description: 'Tools that are never allowed.' }
      }
    },
    bash: {
      description: "`readOnly` confines `bash` writes to temp dirs and the session's `local/`.",
      values: BASH_VALUES
    },
    writes: {
      description:
        'Where `write` and `edit` may write while the mode is active: scheme prefixes like `local://` and directories; `[]` allows nothing, absent allows everything.'
    },
    result: {
      description: 'How the model finishes the mode; absent means only the host ends it.',
      children: {
        schema: { description: 'JSON Schema of the result; defaults to `{"type":"string"}`.' },
        review: { description: 'Whether the host reviews the result before the mode ends.', boolean: true },
        then: { description: "Appended to `yield`'s result when the mode ends through it, so the run continues." }
      }
    },
    axome: {
      description: 'Axome settings for this mode; axo keeps them for the host.',
      children: {
        plan: { description: "`true` shows the mode's reviewed result in Axome's Plan view.", boolean: true }
      }
    }
  },
  keyword: {
    name: { description: 'The exact (case-sensitive) word that triggers it.' },
    description: { description: 'One line for listings.' },
    requiresTools: {
      description: 'Tools that must be listed and allowed in the current mode for the keyword to fire.'
    },
    model: { description: 'A model selector applied as if the user ran `/model <selector>`.' }
  },
  skill: {
    name: { description: "The skill's name; absent takes its directory's root-relative path joined by `:`." },
    description: { description: 'What the skill does and when to use it; a skill without one is dropped.' },
    'disable-model-invocation': {
      description: '`true` hides the skill from the prompt; only `/name` starts it.',
      boolean: true
    },
    'argument-hint': { description: 'Placeholder text shown after `/name` until arguments are typed.' }
  }
}

/** Keys axo accepts on an agent without acting on them; never suggested, never flagged. */
const AGENT_TOLERATED: ReadonlySet<string> = new Set(['blocking', 'spawns', 'thinkingLevel'])
/** The only keys of a team definition (an agent file with `members`). */
const TEAM_KEYS: ReadonlySet<string> = new Set(['name', 'description', 'members'])

const KIND_PATHS: readonly [FrontmatterKind, RegExp][] = [
  ['agent', /(?:^|\/)\.?axome\/agents\/[^/]+\.md$/],
  ['mode', /(?:^|\/)\.?axome\/modes\/.+\.md$/],
  ['keyword', /(?:^|\/)\.?axome\/keywords\/.+\.md$/],
  ['skill', /(?:^|\/)(?:\.?axome|\.agents)\/skills\/.+\/SKILL\.md$/]
]

function frontmatterKind(path: string): FrontmatterKind | null {
  const normalized = path.replace(/\\/g, '/')
  return KIND_PATHS.find(([, pattern]) => pattern.test(normalized))?.[0] ?? null
}

function modelKind(model: Model): FrontmatterKind | null {
  const path = modelHostPath(model.uri)
  return path ? frontmatterKind(path) : null
}

/** The model's lines through the closing `---`; `null` without a frontmatter block opening on line 1. */
function frontmatterLines(model: Model): string[] | null {
  if (model.getLineContent(1) !== '---') return null
  const lines = ['---']
  const count = model.getLineCount()
  for (let number = 2; number <= count; number++) {
    const line = model.getLineContent(number)
    lines.push(line)
    if (line === '---') return lines
  }
  return null
}

const KEY_LINE = /^([A-Za-z_][\w-]*)[ \t]*:(?=\s|$)/

interface KeyEntry {
  /** 0-based line index. */
  line: number
  indent: number
  key: string
  /** `null` for a top-level key. */
  parent: string | null
  /** 0-based offset of the text after the colon. */
  rest: number
}

function isTeam(kind: FrontmatterKind, entries: readonly KeyEntry[]): boolean {
  return kind === 'agent' && entries.some((entry) => entry.parent === null && entry.key === 'members')
}

interface ScanState {
  parent: string | null
  childIndent: number | null
  /** The parent holds a block scalar (`|`/`>`), so its indented lines are text. */
  opaque: boolean
}

/** Top-level keys and the direct children of top-level keys in lines [1, to). */
function scan(lines: readonly string[], to: number): { entries: KeyEntry[]; state: ScanState } {
  const entries: KeyEntry[] = []
  const state: ScanState = { parent: null, childIndent: null, opaque: false }
  for (let index = 1; index < to; index++) {
    const line = lines[index]
    const trimmed = line.trimStart()
    if (trimmed === '' || trimmed.startsWith('#')) continue
    const indent = line.length - trimmed.length
    const match = KEY_LINE.exec(trimmed)
    if (indent === 0) {
      state.childIndent = null
      state.parent = match ? match[1] : null
      if (!match) continue
      state.opaque = /^[|>]/.test(trimmed.slice(match[0].length).trimStart())
      entries.push({ line: index, indent, key: match[1], parent: null, rest: match[0].length })
      continue
    }
    if (state.parent === null || state.opaque) continue
    state.childIndent ??= indent
    if (indent !== state.childIndent || !match) continue
    entries.push({ line: index, indent, key: match[1], parent: state.parent, rest: indent + match[0].length })
  }
  return { entries, state }
}

function keySpec(kind: FrontmatterKind, entry: Pick<KeyEntry, 'key' | 'parent'>): FrontmatterKey | undefined {
  const schema = FRONTMATTER_SCHEMA[kind]
  if (entry.parent === null) return Object.hasOwn(schema, entry.key) ? schema[entry.key] : undefined
  const children = Object.hasOwn(schema, entry.parent) ? schema[entry.parent].children : undefined
  return children && Object.hasOwn(children, entry.key) ? children[entry.key] : undefined
}

/** The scalar after a key's colon, without quotes or a trailing comment; `null` when empty. */
function scalar(line: string, rest: number): { text: string; quoted: boolean; start: number; end: number } | null {
  const after = line.slice(rest)
  const start = rest + after.length - after.trimStart().length
  const value = line.slice(start)
  if (value === '' || value.startsWith('#')) return null
  const quote = value[0]
  if (quote === '"' || quote === "'") {
    const close = value.indexOf(quote, 1)
    const end = close === -1 ? line.length : start + close + 1
    return { text: line.slice(start + 1, close === -1 ? line.length : end - 1), quoted: true, start, end }
  }
  const text = value.split(/\s#/, 1)[0].trimEnd()
  return { text, quoted: false, start, end: start + text.length }
}

interface FrontmatterProblem {
  /** 0-based line index; columns are 0-based, end exclusive. */
  line: number
  start: number
  end: number
  severity: 'warning' | 'error'
  message: string
}

function diagnoseFrontmatter(kind: FrontmatterKind, lines: readonly string[]): FrontmatterProblem[] {
  const end = lines.length - 1
  const problems: FrontmatterProblem[] = []
  const entries = scan(lines, end).entries
  const team = isTeam(kind, entries)
  for (const entry of entries) {
    const spec = keySpec(kind, entry)
    // Only agents and teams reject unknown keys; modes and keywords keep them for hosts, skills ignore them.
    const unknown =
      kind === 'agent' &&
      entry.parent === null &&
      (team ? !TEAM_KEYS.has(entry.key) : !spec && !AGENT_TOLERATED.has(entry.key))
    if (unknown) {
      problems.push({
        line: entry.line,
        start: entry.indent,
        end: entry.indent + entry.key.length,
        severity: 'warning',
        message: `Unknown key "${entry.key}"`
      })
      continue
    }
    if (!spec) continue
    const value = scalar(lines[entry.line], entry.rest)
    if (!value) continue
    const message =
      spec.values && !spec.values.includes(value.text)
        ? `Expected one of: ${spec.values.join(', ')}`
        : spec.boolean && (value.quoted || (value.text !== 'true' && value.text !== 'false'))
          ? 'Expected true or false'
          : null
    if (message) problems.push({ line: entry.line, start: value.start, end: value.end, severity: 'error', message })
  }
  return problems
}

/** The scan state at `line` when it lies inside a terminated frontmatter block. */
function blockAt(lines: readonly string[], line: number) {
  const end = lines.length - 1
  if (line < 1 || line >= end) return null
  return { end, ...scan(lines, line) }
}

function blockContext(model: Model, lineNumber: number) {
  const kind = modelKind(model)
  if (!kind) return null
  const lines = frontmatterLines(model)
  if (!lines) return null
  const lineIndex = lineNumber - 1
  const block = blockAt(lines, lineIndex)
  return block ? { kind, lines, lineIndex, block } : null
}

/** The parent of an indented key on a line whose indent is `indent`, given the scan state above it. */
function childParent(state: ScanState, indent: number): string | null {
  if (state.parent === null || state.opaque) return null
  return state.childIndent === null || state.childIndent === indent ? state.parent : null
}

const MARKER_OWNER = 'axo-frontmatter'
const DIAGNOSTIC_DELAY_MS = 300
let registered = false

/** Register frontmatter support for markdown models once per Monaco instance. */
export function registerFrontmatterSupport(monaco: Monaco): void {
  if (registered) return
  registered = true

  monaco.languages.registerCompletionItemProvider('markdown', {
    triggerCharacters: [':', ' '],
    provideCompletionItems(model, position) {
      const context = blockContext(model, position.lineNumber)
      if (!context) return { suggestions: [] }
      const { kind, lines, lineIndex, block } = context

      const line = lines[lineIndex]
      const before = line.slice(0, position.column - 1)
      const after = line.slice(position.column - 1)
      const wordTail = /^[\w-]*/.exec(after)?.[0] ?? ''
      const range = (partial: string) => ({
        startLineNumber: position.lineNumber,
        endLineNumber: position.lineNumber,
        startColumn: position.column - partial.length,
        endColumn: position.column + wordTail.length
      })

      const valueMatch = /^([ \t]*)([A-Za-z_][\w-]*)[ \t]*:([ \t]*)([\w-]*)$/.exec(before)
      if (valueMatch) {
        const [, indent, key, gap, partial] = valueMatch
        const parent = indent === '' ? null : childParent(block.state, indent.length)
        if (indent !== '' && parent === null) return { suggestions: [] }
        const spec = keySpec(kind, { key, parent })
        const values = spec?.values ?? (spec?.boolean ? ['true', 'false'] : null)
        if (!values) return { suggestions: [] }
        return {
          suggestions: values.map((value) => ({
            label: value,
            kind: monaco.languages.CompletionItemKind.EnumMember,
            insertText: gap === '' ? ` ${value}` : value,
            range: range(partial)
          }))
        }
      }

      const keyMatch = /^([ \t]*)([\w-]*)$/.exec(before)
      if (!keyMatch || after.slice(wordTail.length).trim() !== '') return { suggestions: [] }
      const [, indent, partial] = keyMatch
      let keys: Readonly<Record<string, FrontmatterKey>> | undefined
      let parent: string | null = null
      if (indent === '') {
        keys = FRONTMATTER_SCHEMA[kind]
      } else {
        parent = childParent(block.state, indent.length)
        if (parent === null) return { suggestions: [] }
        keys = keySpec(kind, { key: parent, parent: null })?.children
      }
      if (!keys) return { suggestions: [] }

      const entries = scan(lines, block.end).entries
      const present = new Set(
        entries.filter((entry) => entry.parent === parent && entry.line !== lineIndex).map((entry) => entry.key)
      )
      const team = indent === '' && isTeam(kind, entries)
      return {
        suggestions: Object.entries(keys)
          .filter(([key]) => !team || TEAM_KEYS.has(key))
          .filter(([key]) => !present.has(key))
          .map(([key, spec]) => ({
            label: key,
            kind: monaco.languages.CompletionItemKind.Property,
            insertText: `${key}: `,
            documentation: { value: spec.description },
            range: range(partial),
            command:
              spec.values || spec.boolean ? { id: 'editor.action.triggerSuggest', title: 'Suggest values' } : undefined
          }))
      }
    }
  })

  monaco.languages.registerHoverProvider('markdown', {
    provideHover(model, position) {
      const context = blockContext(model, position.lineNumber)
      if (!context) return null
      const { kind, lines, lineIndex, block } = context

      const line = lines[lineIndex]
      const trimmed = line.trimStart()
      const indent = line.length - trimmed.length
      const match = KEY_LINE.exec(trimmed)
      if (!match) return null
      const key = match[1]
      const column = position.column - 1
      if (column < indent || column > indent + key.length) return null
      const parent = indent === 0 ? null : childParent(block.state, indent)
      if (indent !== 0 && parent === null) return null
      const spec = keySpec(kind, { key, parent })
      if (!spec) return null
      return {
        range: {
          startLineNumber: position.lineNumber,
          endLineNumber: position.lineNumber,
          startColumn: indent + 1,
          endColumn: indent + key.length + 1
        },
        contents: [{ value: spec.description }]
      }
    }
  })

  for (const model of monaco.editor.getModels()) watchModel(monaco, model)
  monaco.editor.onDidCreateModel((model) => watchModel(monaco, model))
}

function watchModel(monaco: Monaco, model: Model): void {
  const kind = modelKind(model)
  if (!kind) return
  let timer: ReturnType<typeof setTimeout> | undefined

  const validate = () => {
    timer = undefined
    const lines = model.getLanguageId() === 'markdown' ? frontmatterLines(model) : null
    const problems = lines ? diagnoseFrontmatter(kind, lines) : []
    monaco.editor.setModelMarkers(
      model,
      MARKER_OWNER,
      problems.map((problem) => ({
        severity: problem.severity === 'error' ? monaco.MarkerSeverity.Error : monaco.MarkerSeverity.Warning,
        message: problem.message,
        startLineNumber: problem.line + 1,
        endLineNumber: problem.line + 1,
        startColumn: problem.start + 1,
        endColumn: problem.end + 1
      }))
    )
  }

  const content = model.onDidChangeContent(() => {
    clearTimeout(timer)
    timer = setTimeout(validate, DIAGNOSTIC_DELAY_MS)
  })
  const language = model.onDidChangeLanguage(validate)
  model.onWillDispose(() => {
    clearTimeout(timer)
    content.dispose()
    language.dispose()
    monaco.editor.setModelMarkers(model, MARKER_OWNER, [])
  })
  validate()
}
