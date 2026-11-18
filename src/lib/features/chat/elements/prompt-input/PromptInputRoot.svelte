<script lang="ts">
  import { mount, unmount, onMount, untrack, type Snippet } from 'svelte'
  import { EditorState, Plugin, TextSelection } from 'prosemirror-state'
  import { Decoration, DecorationSet, EditorView } from 'prosemirror-view'
  import { keymap } from 'prosemirror-keymap'
  import { history, undo, redo } from 'prosemirror-history'
  import { deleteSelection, selectAll } from 'prosemirror-commands'
  import { IconButton } from '$lib/components/ui/button'
  import Icon, { type IconData } from '$lib/icons/Icon.svelte'
  import type { ChatCommand, ChatKeywordMatch, ChatModel, ChatPart, FolderEntry } from '$lib/types/backend'
  import {
    ArrowUp02Icon,
    File01Icon,
    Folder01Icon,
    Layers01Icon,
    Navigation03Icon,
    Queue01Icon,
    SlashIcon,
    SparklesIcon
  } from '@hugeicons/core-free-icons'
  import { notify } from '$lib/features/notifications/state.svelte'
  import PartChip from '$lib/features/chat/parts/PartChip.svelte'
  import {
    CONTENT_LIMIT,
    CONTENT_LIMIT_MESSAGE,
    contentBytes,
    hasContent,
    invokedCommand,
    partsText
  } from '$lib/features/chat/parts/parts'
  import { logicalKey } from '$lib/utils/keyboardEvent'
  import { backend } from '$lib/api/backend'
  import { ensureProjectFiles, getProjectFilePaths } from '$lib/features/files/projectFiles.svelte'
  import { rankPaths } from '$lib/features/command-palette/commands/files.svelte'
  import { basename, dirname, resolveProjectFile, splitRemotePath } from '$lib/utils/paths'
  import {
    promptSchema,
    partsDocument,
    documentParts,
    documentText,
    commandFields,
    commandArguments,
    fieldChoices,
    offeredCommand,
    parseCommandLine,
    chipOrigin,
    type CommandArgument,
    type CommandLine
  } from './document'
  import PromptSuggestions, { type Suggestion } from './PromptSuggestions.svelte'
  import { DND_MIME, type DragPayload } from '$lib/features/dnd/payload'
  import { promptInputDropTarget } from '$lib/features/dnd/adapters/prompt-input'
  import { getErrorMessage } from '$lib/utils/client-error'
  import { blobBase64, fileReference, joinHostPath, onHost, readAttachment } from './attachments'

  let {
    parts = $bindable([]),
    disabled = false,
    placeholder,
    commands,
    models = [],
    running = false,
    owner,
    onSubmit,
    onFollowUp,
    onCommand,
    onMode,
    onCancel,
    keywords = [],
    keywordMatches,
    start,
    end,
    hint = null
  }: {
    parts?: ChatPart[]
    disabled?: boolean
    placeholder: string
    commands: ChatCommand[]
    models?: ChatModel[]
    running?: boolean
    owner: { folderPath: string; sessionId: string | null }
    onSubmit: () => unknown
    onFollowUp?: () => unknown
    onCommand?: (name: string, args: Record<string, unknown>) => Promise<string | null>
    /** Enters the mode, then submits `parts` (the message without its `/name`) when non-empty. */
    onMode?: (name: string, parts: ChatPart[]) => Promise<string | null>
    onCancel?: () => void
    /** Names of the session's keywords; a text containing none skips the `keywordMatches` round trip. */
    keywords?: string[]
    keywordMatches?: (texts: string[]) => Promise<ChatKeywordMatch[][]>
    start?: Snippet
    end?: Snippet
    /** Shown above the input as the last fallback while the draft has content. */
    hint?: { title: string; text: string } | null
  } = $props()
  const id = $props.id()
  let host: HTMLDivElement
  let view: EditorView | undefined
  let lastParts: ChatPart[] = []
  let validation = $state('')
  // The argument a failed send flagged; cleared by the next edit.
  let invalid = $state<string | null>(null)
  let submitting = $state(false)
  let altHeld = $state(false)
  const sendMode = $derived<'send' | 'steer' | 'followUp'>(
    running ? (altHeld && onFollowUp ? 'followUp' : 'steer') : 'send'
  )
  const canSend = $derived(!disabled && !submitting && hasContent(parts))
  const skill = $derived(invokedCommand(parts, commands, 'skill'))
  // Only a composer that can enter modes treats `/<mode>` as one; elsewhere it is plain text.
  const mode = $derived(onMode ? invokedCommand(parts, commands, 'mode') : null)
  type Query =
    | { kind: 'file' | 'command'; text: string; from: number; to: number }
    | { kind: 'argument'; text: string; from: number; to: number; arg: CommandArgument }
  let query = $state.raw<Query | null>(null)
  // The built-in command being typed, re-read on every editor change.
  let line = $state.raw<CommandLine | null>(null)
  let dismissed = ''
  let active = $state(0)
  let files = $state.raw<string[]>([])
  let fileFolder = ''
  type ProjectPaths = { folder: string; source: readonly string[]; known: Set<string>; paths: string[] }
  let projectPaths: ProjectPaths | null = null
  /** The project's files plus their ancestor folders, rebuilt only when the project file list changes. */
  function projectPathsFor(folder: string): ProjectPaths {
    const source = getProjectFilePaths(folder)
    if (projectPaths?.folder === folder && projectPaths.source === source) return projectPaths
    const known = new Set(source)
    for (const path of source) {
      let end = path.lastIndexOf('/')
      while (end > 0) {
        known.add(path.slice(0, end) + '/')
        end = path.lastIndexOf('/', end - 1)
      }
    }
    projectPaths = { folder, source, known, paths: [...known].sort() }
    return projectPaths
  }
  // A mention typed as `/…`, `~/…` or `../…` lists that folder on the chat's host instead of the project.
  const outsidePrefix = /^(\/|~\/|\.\.\/)/
  let outside = $state.raw<{ typed: string; entries: FolderEntry[] } | null>(null)
  let outsideKey = ''
  const catalog = $derived(commands.filter(offeredCommand))
  type Item = Suggestion & { command?: ChatCommand; path?: string; value?: string }
  const catalogRank: Record<ChatCommand['kind'], number> = { builtin: 0, mode: 1, skill: 2 }
  const catalogGroup: Record<ChatCommand['kind'], string> = { builtin: 'Commands', mode: 'Modes', skill: 'Skills' }
  const catalogIcon: Record<ChatCommand['kind'], IconData> = {
    builtin: SlashIcon,
    mode: Layers01Icon,
    skill: SparklesIcon
  }
  const items = $derived.by((): Item[] => {
    if (query?.kind === 'argument') {
      const { arg, text } = query
      const typed = text.toLowerCase()
      return (fieldChoices(arg.field, models) ?? [])
        .filter((choice) => choice.label.toLowerCase().includes(typed) || choice.value.toLowerCase().includes(typed))
        .map((choice) => ({
          key: choice.value,
          group: arg.name,
          label: choice.label,
          detail: choice.detail,
          value: choice.value
        }))
    }
    if (query?.kind === 'file' && outsidePrefix.test(query.text)) {
      const typed = query.text.slice(0, query.text.lastIndexOf('/') + 1)
      const leaf = query.text.slice(typed.length).toLowerCase()
      if (outside?.typed !== typed) return []
      return outside.entries
        .filter((entry) => entry.name.toLowerCase().startsWith(leaf))
        .toSorted((a, b) => Number(b.is_dir) - Number(a.is_dir) || a.name.localeCompare(b.name))
        .map((entry) => ({
          key: entry.path,
          group: 'Files',
          label: entry.name + (entry.is_dir ? '/' : ''),
          detail: typed,
          icon: entry.is_dir ? Folder01Icon : File01Icon,
          path: fileReference(entry.path, owner.folderPath, entry.is_dir),
          complete: entry.is_dir ? `${typed}${entry.name}/` : undefined
        }))
    }
    if (query?.kind === 'file')
      return rankPaths(files, query.text)
        .slice(0, 80)
        .map(({ path }) => {
          const bare = path.replace(/\/$/, '')
          const directory = bare !== path
          return {
            key: path,
            group: 'Files',
            label: basename(bare) + (directory ? '/' : ''),
            detail: dirname(bare) || undefined,
            icon: directory ? Folder01Icon : File01Icon,
            path,
            complete: directory ? path : undefined
          }
        })
    if (query)
      return catalog
        .filter((item) => item.name.startsWith(query!.text))
        .sort((a, b) => catalogRank[a.kind] - catalogRank[b.kind])
        .map((item) => ({
          key: item.name,
          group: catalogGroup[item.kind],
          label: item.name,
          icon: catalogIcon[item.kind],
          detail: item.description,
          fields:
            item.kind === 'builtin'
              ? commandFields(item).map(([name]) => name)
              : item.argumentHint
                ? [item.argumentHint]
                : undefined,
          blocked: running && item.policy === 'when_idle' ? 'when idle' : undefined,
          command: item
        }))
    return []
  })
  const open = $derived(!disabled && !!query)
  // What the line above the input explains: the argument under the caret, the command, or the skill or mode being typed.
  const context = $derived.by(() => {
    if (line?.slot) {
      const required = (line.command.parameters.required ?? []) as string[]
      return {
        title: line.slot.name,
        note: required.includes(line.slot.name) ? undefined : 'optional',
        text: line.slot.field.description ?? ''
      }
    }
    if (line)
      return {
        title: '/' + line.command.name,
        note: running && line.command.policy === 'when_idle' ? 'when idle' : undefined,
        text: line.command.description
      }
    const invoked = skill ?? mode
    if (invoked)
      return { title: '/' + invoked.name, note: invoked.argumentHint ?? undefined, text: invoked.description }
    if (hint && hasContent(parts)) return { title: hint.title, note: undefined, text: hint.text }
    return null
  })
  function getParts(): ChatPart[] {
    return view ? documentParts(view.state.doc) : parts
  }
  // A new draft starts a new history, so a sent message's attachments aren't kept for undo.
  function load(next: ChatPart[]) {
    staleKeywords()
    view!.updateState(
      EditorState.create({ schema: promptSchema, doc: partsDocument(next), plugins: view!.state.plugins })
    )
    validation = ''
    invalid = null
    updateQuery()
  }
  function setParts(next: ChatPart[]): void {
    parts = next
    lastParts = parts
    if (view) load(next)
  }
  function clear(): void {
    setParts([])
  }
  export function focus(insert?: string): void {
    if (disabled || !view) return
    view.focus()
    if (insert) view.dispatch(view.state.tr.insertText(insert))
  }
  export function insertPart(part: ChatPart): void {
    if (!view || disabled) return
    if (contentBytes([...getParts(), part]) > CONTENT_LIMIT) {
      notify.error('Attachment too large', CONTENT_LIMIT_MESSAGE)
      return
    }
    const { from, to } = view.state.selection
    if (part.type === 'text') view.dispatch(view.state.tr.insertText(part.text, from, to).scrollIntoView())
    else placeChip(part, from, to)
    view.focus()
  }
  function placeChip(part: ChatPart, from: number, to: number) {
    const tr = view!.state.tr.replaceWith(from, to, promptSchema.node('part', { part }))
    // A chip reads as a word: keep a space after it so the next typed text doesn't run into it.
    if (!/\s/.test(tr.doc.textBetween(from + 1, Math.min(from + 2, tr.doc.content.size - 1))))
      tr.insertText(' ', from + 1)
    view!.dispatch(tr.scrollIntoView())
  }
  async function submit(followUp = false): Promise<void> {
    if (disabled || submitting) return
    const fail = (message: string, name: string | null, caret: number) => {
      validation = message
      invalid = name
      view!.dispatch(view!.state.tr.setSelection(TextSelection.create(view!.state.doc, caret + 1)))
      view!.focus()
    }
    const settle = async (start: () => Promise<string | null>, end: number) => {
      submitting = true
      const error = await start().finally(() => {
        submitting = false
      })
      if (!view) return
      if (error === null) {
        clear()
        focus()
      } else fail(error, null, end)
    }
    if (view && line) {
      const { command } = line
      const end = documentText(view.state.doc).length
      if (getParts().some((part) => part.type !== 'text'))
        return fail(`/${command.name} can't take attachments.`, null, end)
      if (running && command.policy === 'when_idle') return fail(`/${command.name} is available when idle.`, null, end)
      if (line.extra) return fail(`/${command.name} doesn't take "${line.extra.text}".`, null, line.extra.to)
      const result = commandArguments(command, Object.fromEntries(line.args.map((arg) => [arg.name, arg.text])))
      if (result.invalid)
        return fail(
          result.message ?? '',
          result.invalid,
          line.args.find((arg) => arg.name === result.invalid)?.to ?? end
        )
      const run = onCommand
      if (!run) return
      return settle(() => run(command.name, result.args), end)
    } else if (view && mode && onMode) {
      const end = documentText(view.state.doc).length
      if (running && mode.policy === 'when_idle') return fail(`/${mode.name} is available when idle.`, null, end)
      // The rest of the message, chips included, is what the mode starts with.
      const [first, ...others] = getParts()
      const text = first?.type === 'text' ? first.text.slice(mode.name.length + 1).replace(/^\s+/, '') : ''
      const rest: ChatPart[] = text ? [{ type: 'text', text }, ...others] : others
      const enter = onMode
      const name = mode.name
      return settle(() => enter(name, rest), end)
    } else {
      submitting = true
      try {
        await (running && followUp && onFollowUp ? onFollowUp() : onSubmit())
      } finally {
        submitting = false
      }
    }
  }
  /** Accepts the highlighted item; `descend` (Tab) types a folder's path into the mention to list it instead. */
  function choose(index = active, descend = false) {
    const item = items[index]
    if (!item || !query || !view) return
    if (item.blocked) {
      validation = `/${item.label} is available when idle.`
      return
    }
    const { from, to } = query
    if (descend && item.complete !== undefined) {
      view.dispatch(view.state.tr.insertText('@' + item.complete, from, to))
      return focus()
    }
    // Separate the inserted name or value from what follows, unless whitespace already does.
    const space = /\s/.test(view.state.doc.textBetween(to, Math.min(to + 1, view.state.doc.content.size - 1)))
      ? ''
      : ' '
    if (query.kind === 'argument' || item.command) {
      const inserted = query.kind === 'argument' ? item.value! : '/' + item.command!.name
      const onward = !(query.kind === 'argument' && query.arg.last)
      const tr = view.state.tr.insertText(inserted + (onward ? space : ''), from, to)
      // Land on the next argument, stepping over whitespace that was already there.
      if (onward && !space) tr.setSelection(TextSelection.create(tr.doc, tr.selection.from + 1))
      view.dispatch(tr)
    } else if (item.path) {
      placeChip({ type: 'file', path: item.path }, from, to)
    }
    focus()
  }
  async function loadFiles() {
    const folder = owner.folderPath
    const prefix = query?.kind === 'file' ? query.text : ''
    if (outsidePrefix.test(prefix))
      return void listOutside(folder, prefix.slice(0, prefix.lastIndexOf('/') + 1), prefix)
    const directory = prefix.includes('/') ? prefix.slice(0, prefix.lastIndexOf('/')) : ''
    const key = `${folder}:${directory}`
    if (fileFolder === key) return
    fileFolder = key
    let base: ProjectPaths
    try {
      await ensureProjectFiles(folder)
      base = projectPathsFor(folder)
    } catch {
      notify.error('File list unavailable', 'Could not load project paths.')
      return
    }
    // A folder still being typed may not exist yet; it adds nothing.
    const entries = await backend.files.readDir(folder, directory, false).catch(() => [])
    const extra = entries.map((entry) => entry.path + (entry.is_dir ? '/' : '')).filter((path) => !base.known.has(path))
    if (owner.folderPath === folder && fileFolder === key) {
      files = extra.length ? [...base.paths, ...extra.sort()].sort() : base.paths
    }
  }
  async function listOutside(folder: string, typed: string, prefix: string) {
    const hidden = prefix.slice(typed.length).startsWith('.')
    const key = `${folder}\0${typed}\0${hidden}`
    if (outsideKey === key) return
    outsideKey = key
    let entries: FolderEntry[] = []
    try {
      const server = splitRemotePath(folder)?.server
      const base = typed.startsWith('~/')
        ? await backend.folders.home(server)
        : typed.startsWith('/')
          ? server
            ? `axome://${server}/`
            : '/'
          : folder
      entries = await backend.folders.listEntries(joinHostPath(base, typed.replace(/^~\//, '')), hidden)
    } catch {
      /* A folder that doesn't exist lists nothing; the panel says "No matches". */
    }
    if (outsideKey === key) outside = { typed, entries }
  }
  // Keyword highlights: axo matches the text; offsets are document positions. Every document
  // change drops them (their positions are stale) and invalidates any match in flight.
  let keywordRanges: { from: number; to: number }[] = []
  let keywordGeneration = 0
  let keywordTimer: ReturnType<typeof setTimeout> | undefined
  /** Clears the highlights before the next state is drawn and queues a debounced match; returns whether any were shown. */
  function staleKeywords(): boolean {
    const shown = keywordRanges.length > 0
    const generation = ++keywordGeneration
    keywordRanges = []
    clearTimeout(keywordTimer)
    keywordTimer = setTimeout(() => void matchKeywords(generation), 150)
    return shown
  }
  async function matchKeywords(generation: number) {
    if (!view) return
    const segments = documentText(view.state.doc).split('\ufffc')
    const names = keywords.map((name) => name.toLowerCase())
    // Only saves round trips: axo decides what matches.
    const candidate =
      !!keywordMatches && segments.some((segment) => names.some((name) => segment.toLowerCase().includes(name)))
    let ranges: { from: number; to: number }[] = []
    if (candidate) {
      try {
        const matches = await keywordMatches!(segments)
        let offset = 0
        segments.forEach((segment, index) => {
          for (const match of matches[index] ?? [])
            ranges.push({ from: offset + 1 + match.start, to: offset + 1 + match.end })
          offset += segment.length + 1
        })
      } catch {
        ranges = [] // Highlighting is advisory; a failed match shows none.
      }
    }
    if (!view || generation !== keywordGeneration || (!ranges.length && !keywordRanges.length)) return
    keywordRanges = ranges
    view.dispatch(view.state.tr.setMeta('keywords', true))
  }
  function updateQuery() {
    if (!view) return
    const { selection, doc } = view.state
    const caret = selection.from - 1
    const full = documentText(doc)
    line = parseCommandLine(full, caret, catalog)
    if (!selection.empty) {
      query = null
      return
    }
    const text = full.slice(0, caret)
    const slash = /^\/(\S*)$/.exec(text)
    const mention = /(?:^|\s)@([^\s]*)$/.exec(text)
    const slot = line?.slot
    const choices = slot && fieldChoices(slot.field, models)
    const key = `${selection.from}:${text}`
    const previous = query?.text
    query =
      key === dismissed
        ? null
        : slot && choices && !choices.some((choice) => choice.value === slot.text)
          ? { kind: 'argument', text: slot.text, from: slot.from + 1, to: slot.to + 1, arg: slot }
          : slash
            ? {
                kind: 'command',
                text: slash[1],
                from: 1,
                to: selection.from + /^\S*/.exec(full.slice(caret))![0].length
              }
            : mention
              ? { kind: 'file', text: mention[1], from: selection.from - mention[1].length - 1, to: selection.from }
              : null
    if (query?.text !== previous)
      active = Math.max(
        0,
        items.findIndex((item) => !item.blocked)
      )
    if (query?.kind === 'file') void loadFiles()
  }
  function decorations(state: EditorState): DecorationSet {
    const decorations: Decoration[] = []
    // Chips a selection covers light up like selected text; the browser can't paint the
    // selection of a non-editable element itself.
    const { from, to, empty } = state.selection
    if (!empty)
      state.doc.nodesBetween(from, to, (node, pos) => {
        if (node.type.name === 'part')
          decorations.push(Decoration.node(pos, pos + node.nodeSize, { 'data-selected': '' }))
      })
    // A mode's `/name` reads like a command name; keywords axo matched read as accent.
    const invoked = onMode ? invokedCommand(documentParts(state.doc), commands, 'mode') : null
    if (invoked) decorations.push(Decoration.inline(1, invoked.name.length + 2, { class: 'font-medium text-bright' }))
    for (const range of keywordRanges)
      decorations.push(Decoration.inline(range.from, range.to, { class: 'font-medium text-accent' }))
    // The built-in command being typed is styled: its name, each argument as a pill, the arguments
    // still to come as placeholders, and anything invalid in red. The text itself stays plain.
    const parsed = parseCommandLine(documentText(state.doc), state.selection.from - 1, catalog)
    if (!parsed) return DecorationSet.create(state.doc, decorations)
    const pill = 'rounded-sm px-0.5 box-decoration-clone'
    decorations.push(Decoration.inline(1, parsed.nameEnd + 1, { class: 'font-medium text-bright' }))
    for (const arg of parsed.args) {
      decorations.push(
        Decoration.inline(arg.from + 1, arg.to + 1, {
          class: `${pill} ${arg.name === invalid ? 'bg-danger-bg text-danger-bright' : 'bg-raised text-fg'}`
        })
      )
    }
    if (parsed.extra)
      decorations.push(
        Decoration.inline(parsed.extra.from + 1, parsed.extra.to + 1, {
          class: `${pill} bg-danger-bg text-danger-bright`
        })
      )
    if (parsed.missing.length) {
      const missing = parsed.missing
      decorations.push(
        Decoration.widget(
          state.doc.content.size - 1,
          () => {
            const hint = document.createElement('span')
            hint.className = 'pointer-events-none select-none'
            for (const name of missing) {
              const placeholder = hint.appendChild(document.createElement('span'))
              placeholder.className = `ms-1.5 rounded-sm border px-1 text-sm ${name === invalid ? 'border-danger-border text-danger' : 'border-edge text-subtle'}`
              placeholder.textContent = name
            }
            return hint
          },
          { side: 1, key: `missing:${missing.join(' ')}:${invalid}` }
        )
      )
    }
    return DecorationSet.create(state.doc, decorations)
  }
  function keydown(event: KeyboardEvent): boolean {
    if (event.isComposing || disabled || event.defaultPrevented) return false
    const key = logicalKey(event)
    let handled = false
    if (key === 'Escape' && open) {
      if (view)
        dismissed = `${view.state.selection.from}:${documentText(view.state.doc).slice(0, view.state.selection.from - 1)}`
      query = null
      handled = true
    } else if (key === 'Escape' && onCancel) {
      onCancel()
      handled = true
    } else if (
      open &&
      items.length &&
      !event.altKey &&
      !event.ctrlKey &&
      !event.metaKey &&
      !(key === 'Tab' && event.shiftKey) &&
      ['ArrowDown', 'ArrowUp', 'Enter', 'Tab'].includes(key)
    ) {
      if (key === 'Enter' || key === 'Tab') choose(active, key === 'Tab')
      else active = (active + (key === 'ArrowDown' ? 1 : -1) + items.length) % items.length
      handled = true
    } else if (key === 'Enter' && !event.shiftKey && !event.ctrlKey && !event.metaKey) {
      void submit(event.altKey)
      handled = true
    }
    if (handled) {
      event.preventDefault()
      event.stopPropagation()
    }
    return handled
  }
  /** Steps over, or deletes, the chip beside the caret in one keypress, as with a character. */
  function chipKey(direction: -1 | 1, remove = false): boolean {
    if (!view) return false
    const { selection } = view.state
    if (!selection.empty) return false
    const node = direction < 0 ? selection.$from.nodeBefore : selection.$from.nodeAfter
    if (node?.type.name !== 'part') return false
    const from = direction < 0 ? selection.from - node.nodeSize : selection.from
    view.dispatch(
      remove
        ? view.state.tr.delete(from, from + node.nodeSize)
        : view.state.tr.setSelection(TextSelection.create(view.state.doc, selection.from + direction * node.nodeSize))
    )
    return true
  }
  async function receive(data: DataTransfer) {
    const attachments = [...data.files]
    if (attachments.length) {
      if (attachments.reduce((size, file) => size + file.size, contentBytes(getParts())) > CONTENT_LIMIT) {
        notify.error('Attachment too large', CONTENT_LIMIT_MESSAGE)
        return
      }
      for (const file of attachments) {
        const bytes = await blobBase64(file)
        const stamp = new Date().toISOString().replace(/[-:]/g, '').replace('T', '-').slice(0, 15)
        const extension =
          file.type === 'image/jpeg' ? 'jpg' : file.type.startsWith('image/') ? file.type.slice(6) : 'bin'
        insertPart({ type: 'content', name: file.name || `paste-${stamp}.${extension}`, bytes })
      }
      return
    }
    const text = data.getData('text/plain').replace(/\r\n?/g, '\n')
    if (text) {
      if (text.length > 1000 || text.split('\n').length > 8) insertPart({ type: 'paste', text })
      else focus(text)
    }
  }
  onMount(() => {
    view = new EditorView(host, {
      state: EditorState.create({
        schema: promptSchema,
        doc: partsDocument(parts),
        plugins: [
          history(),
          new Plugin({ props: { decorations } }),
          keymap({
            'Mod-z': undo,
            'Mod-Shift-z': redo,
            'Mod-y': redo,
            'Mod-a': selectAll,
            ArrowLeft: () => chipKey(-1),
            ArrowRight: () => chipKey(1),
            Backspace: (state, dispatch) => chipKey(-1, true) || deleteSelection(state, dispatch),
            Delete: (state, dispatch) => chipKey(1, true) || deleteSelection(state, dispatch),
            'Shift-Enter': (state, dispatch) => {
              dispatch?.(state.tr.insertText('\n'))
              return true
            }
          })
        ]
      }),
      // Selected text takes the same fill as a selected chip. ProseMirror's separator <img> keeps the
      // caret placeable after a trailing chip; it must not render.
      attributes: {
        role: 'textbox',
        'aria-multiline': 'true',
        'aria-label': placeholder,
        class:
          'min-h-14 max-h-60 overflow-y-auto whitespace-pre-wrap break-words px-4 pt-3 pb-2 text-base leading-relaxed text-fg outline-none selection:bg-selection selection:text-bright [&_img.ProseMirror-separator]:inline [&_img.ProseMirror-separator]:border-0 [&_img.ProseMirror-separator]:m-0'
      },
      editable: () => !disabled,
      dispatchTransaction(transaction) {
        if (transaction.docChanged) {
          validation = ''
          invalid = null
          staleKeywords()
        }
        view!.updateState(view!.state.apply(transaction))
        if (transaction.docChanged) {
          // Read back: a bound $state target stores a proxy, and the sync effect compares identity.
          parts = documentParts(view!.state.doc)
          lastParts = parts
        }
        updateQuery()
      },
      handleKeyDown: (_view, event) => keydown(event),
      // ProseMirror prevents every Escape; one no composer layer consumes must reach the chat
      // keymap (stop the run), so skip ProseMirror's handling for it. An edit's Escape (onCancel)
      // and an open suggestion list's stay with `keydown`.
      handleDOMEvents: { keydown: (_view, event) => event.key === 'Escape' && !open && !onCancel },
      handlePaste: (_view, event) => {
        if (!event.clipboardData) return false
        // Text copied from this composer carries its chips; let ProseMirror paste that slice as-is.
        if (event.clipboardData.getData('text/html').includes(chipOrigin)) return false
        void receive(event.clipboardData).catch(() => notify.error('Paste failed', 'Could not read clipboard content.'))
        return true
      },
      clipboardTextSerializer: (slice) =>
        slice.content.textBetween(0, slice.content.size, '\n', (leaf) => partsText([leaf.attrs.part as ChatPart])),
      // A click on a chip puts the caret on the nearer side of it, as on a character.
      handleClickOn: (view, _pos, node, nodePos, event, direct) => {
        if (!direct || node.type.name !== 'part') return false
        const box = (view.nodeDOM(nodePos) as HTMLElement).getBoundingClientRect()
        const side = event.clientX > box.left + box.width / 2 ? node.nodeSize : 0
        view.dispatch(view.state.tr.setSelection(TextSelection.create(view.state.doc, nodePos + side)))
        return true
      },
      handleDrop: (_view, event, _slice, moved) => {
        if (moved || !event.dataTransfer) return false
        if (event.dataTransfer.types.includes(DND_MIME.AXOME_ITEM)) return true // the root's drop target turns Axome items into chips
        const position = view!.posAtCoords({ left: event.clientX, top: event.clientY })
        if (position) view!.dispatch(view!.state.tr.setSelection(TextSelection.create(view!.state.doc, position.pos)))
        void receive(event.dataTransfer).catch(() => notify.error('Drop failed', 'Could not read dropped content.'))
        return true
      },
      nodeViews: {
        part(node) {
          const dom = document.createElement('span')
          dom.contentEditable = 'false'
          // The chip paints its own selected state; the browser's highlight would cover only its text.
          dom.className = 'select-none'
          const component = mount(PartChip, {
            target: dom,
            props: { part: node.attrs.part as ChatPart, owner, interactive: false }
          })
          return {
            dom,
            ignoreMutation: () => true,
            destroy: () => {
              void unmount(component)
            }
          }
        }
      }
    })
    lastParts = parts
    return () => {
      clearTimeout(keywordTimer)
      keywordGeneration++
      view?.destroy()
      view = undefined
    }
  })
  // A changed keyword set (toolbox loaded, a keyword toggled, the session created) re-matches the draft.
  const keywordKey = $derived(`${!!keywordMatches}\n${keywords.join('\n')}`)
  $effect(() => {
    void keywordKey
    untrack(() => {
      if (staleKeywords()) view?.dispatch(view.state.tr.setMeta('keywords', true))
    })
  })
  async function dropPayload(payload: DragPayload, x: number, y: number) {
    const position = view?.posAtCoords({ left: x, top: y })
    if (position && view) view.dispatch(view.state.tr.setSelection(TextSelection.create(view.state.doc, position.pos)))
    // Paths axo's host can open are referenced; local files dropped on a remote chat are copied in.
    for (const item of payload.items) {
      if (item.kind === 'file') {
        const path = resolveProjectFile(item.folderPath, item.path)
        if (onHost(path, owner.folderPath))
          insertPart({ type: 'file', path: fileReference(path, owner.folderPath, item.isDir) })
        else notify.error("Can't attach from here", "Drop a file from this chat's machine or from your computer.")
      } else if (item.kind === 'os-files') {
        for (const path of item.paths) {
          if (onHost(path, owner.folderPath)) {
            insertPart({ type: 'file', path: fileReference(path, owner.folderPath) })
            continue
          }
          try {
            const bytes = await readAttachment('/', path, CONTENT_LIMIT - contentBytes(getParts()))
            insertPart({ type: 'content', name: path.split('/').pop() || 'attachment', bytes })
          } catch (error) {
            notify.error("Can't attach from here", getErrorMessage(error))
          }
        }
      }
    }
  }
  $effect(() => {
    const next = parts
    if (view && next !== lastParts) {
      lastParts = next
      if (!partsDocument(next).eq(view.state.doc)) load(next)
    }
  })
  $effect(() => {
    void disabled
    view?.setProps({ editable: () => !disabled })
    if (view) {
      view.dom.setAttribute('aria-label', placeholder)
      view.dom.setAttribute('aria-autocomplete', 'list')
      if (open) view.dom.setAttribute('aria-controls', `${id}-options`)
      else view.dom.removeAttribute('aria-controls')
      if (open && items[active]) view.dom.setAttribute('aria-activedescendant', `${id}-options-${active}`)
      else view.dom.removeAttribute('aria-activedescendant')
    }
  })
</script>

<!-- Capture before the composer consumes Enter; Alt is transient, never a saved send mode. -->
<svelte:window
  onkeydowncapture={(event) => (altHeld = event.altKey)}
  onkeyupcapture={(event) => (altHeld = event.altKey)}
  onblur={() => (altHeld = false)}
/>

<div
  data-slot="prompt-input-root"
  role="group"
  aria-label="Message composer"
  tabindex="-1"
  class="relative flex flex-col rounded-xl border border-edge bg-surface transition-colors outline-none focus-within:border-accent"
  {@attach promptInputDropTarget({ id, canAcceptDrop: () => !disabled, onDrop: dropPayload })}
>
  {#if open}
    <PromptSuggestions
      id={`${id}-options`}
      label={query?.kind === 'argument'
        ? `${query.arg.name} values`
        : query?.kind === 'file'
          ? 'Project paths'
          : 'Commands'}
      {items}
      {active}
      onActive={(index) => {
        active = index
      }}
      onPick={(index) => choose(index)}
    />
  {/if}
  {#if context || validation}
    <div class="flex min-w-0 items-baseline gap-2 border-b border-edge px-4 py-2 text-sm">
      {#if validation}
        <span role="alert" class="truncate text-danger">{validation}</span>
      {:else if context}
        <span class="shrink-0 font-medium text-bright">{context.title}</span>
        {#if context.note}<span class="shrink-0 text-xs text-subtle">{context.note}</span>{/if}
        <span class="min-w-0 truncate text-muted">{context.text}</span>
      {/if}
    </div>
  {/if}
  <div class="relative">
    <div bind:this={host}></div>
    {#if !parts.length}<span class="pointer-events-none absolute top-3 left-4 text-base text-subtle">{placeholder}</span
      >{/if}
  </div>
  <div class="flex items-center gap-1.5 px-2.5 pt-1 pb-2.5">
    {#if start}{@render start()}{/if}
    <div class="ms-auto flex min-w-0 items-center gap-1.5">
      {#if end}{@render end()}{/if}
      <IconButton
        size="md"
        tooltip={sendMode === 'send' ? 'Send' : sendMode === 'steer' ? 'Steer' : 'Follow-up'}
        shortcut={sendMode === 'followUp' ? 'Alt+Enter' : 'Enter'}
        tooltipSide="top"
        disabled={!canSend}
        class={canSend ? 'bg-accent-dim text-ground hover:bg-accent hover:text-ground' : ''}
        onclick={(event) => void submit(event.altKey)}
      >
        <Icon
          icon={sendMode === 'send' ? ArrowUp02Icon : sendMode === 'steer' ? Navigation03Icon : Queue01Icon}
          size={14}
        />
      </IconButton>
    </div>
  </div>
</div>
