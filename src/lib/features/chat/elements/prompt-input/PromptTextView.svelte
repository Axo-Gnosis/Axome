<!--
  PromptTextView: the composer as a Monaco editor over the message's text (what the model receives).
  The text is the truth: every edit is read back into parts, where an `@ref` naming a held attachment
  or a known path is a chip again and everything else stays text. Paste boundaries are the only state
  outside the text: decorations that shrink with deletions and never grow.
-->
<script lang="ts">
  import { onMount } from 'svelte'
  import type * as Monaco from 'monaco-editor'
  import type { ChatPart } from '$lib/types/backend'
  import { AXOME_THEME_NAME } from '$lib/features/editor/renderers/monaco/core/monacoTheme'
  import { CODE_LINE_HEIGHT, getCodeFontFamily, getCodeFontSize } from '$lib/features/settings/state/typography.svelte'
  import { ensureProjectFiles } from '$lib/features/files/projectFiles.svelte'
  import { isSchemeUrl } from '$lib/features/workbench/links/parseLink'
  import { notify } from '$lib/features/notifications/state.svelte'
  import { getErrorMessage } from '$lib/utils/client-error'
  import { chatShortcut } from '$lib/features/chat/keymap/bindings.svelte'
  import { getChatKeys, type KeyScope } from '$lib/features/chat/keymap/router.svelte'
  import { isLargePaste, partsText } from '$lib/features/chat/parts/parts'
  import { openPart } from '$lib/features/chat/parts/open'
  import { KeyHint } from '../key-hint'
  import {
    outsideEntries,
    outsideMentions,
    outsidePrefix,
    projectMentionPaths,
    projectMentions,
    projectPathsFor,
    type Mention
  } from './mentions'
  import { PROMPT_LANGUAGE, promptModels, registerPromptLanguage } from './promptLanguage'
  import { readText, writeText, type Range, type Ref } from './textForm'

  let {
    parts,
    open,
    owner,
    disabled,
    running,
    canFollowUp,
    onParts,
    onSend,
    onClose,
    onFiles
  }: {
    /** The draft when the view opens; later outside changes arrive through `load`. */
    parts: ChatPart[]
    /** Where the caret starts, as a text offset; `unfold` leaves the paste on that line open. */
    open: { caret: number; unfold: boolean }
    owner: { folderPath: string; sessionId: string | null }
    disabled: boolean
    running: boolean
    canFollowUp: boolean
    onParts: (parts: ChatPart[]) => void
    onSend: (followUp: boolean) => void
    onClose: () => void
    /** Files pasted into the editor (images); they come back through `insertPart`. */
    onFiles: (data: DataTransfer) => void
  } = $props()

  const keys = getChatKeys()
  let container: HTMLDivElement
  let editor = $state.raw<Monaco.editor.IStandaloneCodeEditor>()
  let monaco: typeof Monaco
  let model: Monaco.editor.ITextModel
  let regions: Monaco.editor.IEditorDecorationsCollection
  let marks: Monaco.editor.IEditorDecorationsCollection
  let refs: Ref[] = []
  // What an `@ref` can stand for: attachments by their token, and paths known to exist beyond the project list.
  // A token can name several attachments (a withdrawn queued message keeps its own `image.png`): each token takes the next.
  const attachments = new Map<string, ChatPart[]>()
  const trusted = new Set<string>()
  let projectLoaded = false
  let filling = false
  let listed: { key: string; paths: string[] } | null = null

  function hold(list: ChatPart[]) {
    for (const part of list) {
      if (part.type === 'file') trusted.add(part.path)
      const token = part.type === 'content' ? part.name : part.type === 'saved' ? part.url : null
      if (token !== null) attachments.set(token, [...(attachments.get(token) ?? []), part])
    }
  }
  const pasteRanges = () => regions.getRanges().filter((range) => !range.isEmpty())
  /** A paste folds from its first line to the line before its last, so the caret after it stays visible and outside it. */
  function folds() {
    return pasteRanges().flatMap((range) =>
      range.endLineNumber - 1 > range.startLineNumber
        ? [{ start: range.startLineNumber, end: range.endLineNumber - 1 }]
        : []
    )
  }
  function fold(lines: number[]) {
    if (lines.length) editor!.trigger('prompt', 'editor.fold', { selectionLines: lines.map((line) => line - 1) })
  }
  async function mentions(text: string): Promise<Mention[]> {
    const folder = owner.folderPath
    let found: Mention[]
    if (outsidePrefix.test(text)) {
      const typed = text.slice(0, text.lastIndexOf('/') + 1)
      const entries = await outsideEntries(folder, typed, text.slice(typed.length).startsWith('.'))
      found = outsideMentions(entries, text, folder)
    } else {
      const directory = text.includes('/') ? text.slice(0, text.lastIndexOf('/')) : ''
      if (listed?.key !== directory) {
        // A failed list isn't kept: the next keystroke asks again.
        const paths = await projectMentionPaths(folder, directory).catch(() => null)
        listed = paths && { key: directory, paths }
      }
      found = projectMentions(listed?.paths ?? [], text)
    }
    // Everything offered exists, including ignored files and paths outside the project.
    for (const mention of found) trusted.add(mention.path)
    return found
  }
  /** Reads the text back into parts and redraws what depends on it. */
  function sync() {
    const known = projectPathsFor(owner.folderPath).known
    const used = new Set<ChatPart>()
    const offsets: Range[] = pasteRanges()
      .map((range) => ({
        from: model.getOffsetAt(range.getStartPosition()),
        to: model.getOffsetAt(range.getEndPosition())
      }))
      .sort((a, b) => a.from - b.from)
    const result = readText(model.getValue(), offsets, (ref) => {
      const held = attachments.get(ref)?.find((part) => !used.has(part))
      if (held) return (used.add(held), held)
      return known.has(ref) || trusted.has(ref) ? { type: 'file', path: ref } : null
    })
    refs = result.refs
    const span = (ref: Ref) => monaco.Range.fromPositions(model.getPositionAt(ref.from), model.getPositionAt(ref.to))
    marks.set([
      ...refs
        .filter((ref) => ref.part)
        .map((ref) => ({ range: span(ref), options: { inlineClassName: 'axome-prompt-ref' } })),
      ...pasteRanges().map((range) => {
        const column = model.getLineMaxColumn(range.startLineNumber)
        return {
          range: new monaco.Range(range.startLineNumber, column, range.startLineNumber, column),
          options: {
            showIfCollapsed: true,
            after: {
              content: `\u00a0\u00a0Pasted text · ${range.endLineNumber - range.startLineNumber + 1} lines`,
              inlineClassName: 'axome-prompt-paste-label',
              cursorStops: monaco.editor.InjectedTextCursorStops.None
            }
          }
        }
      })
    ])
    // A project-looking path that names nothing; outside paths and scheme URLs can't be checked from here.
    const missing = (ref: Ref) =>
      !ref.part && projectLoaded && !outsidePrefix.test(ref.ref) && !isSchemeUrl(ref.ref) && /[/.]/.test(ref.ref)
    monaco.editor.setModelMarkers(
      model,
      PROMPT_LANGUAGE,
      refs.filter(missing).map((ref) => {
        const range = span(ref)
        return {
          severity: monaco.MarkerSeverity.Warning,
          message: `No file at ${ref.ref}. It is sent as plain text.`,
          startLineNumber: range.startLineNumber,
          startColumn: range.startColumn,
          endLineNumber: range.endLineNumber,
          endColumn: range.endColumn
        }
      })
    )
    onParts(result.parts)
  }
  /** Replaces the whole text with `next`; pastes enter folded, except one the caret was sent into. */
  function fill(next: ChatPart[], caret: number, unfold: boolean) {
    // `setValue` clears Monaco's history, so no undo can bring back a token of an attachment dropped here.
    attachments.clear()
    hold(next)
    const { text, pastes } = writeText(next)
    filling = true
    model.setValue(text)
    filling = false
    regions.set(
      pastes.map((paste) => ({
        range: monaco.Range.fromPositions(model.getPositionAt(paste.from), model.getPositionAt(paste.to)),
        options: {
          stickiness: monaco.editor.TrackedRangeStickiness.NeverGrowsWhenTypingAtEdges,
          linesDecorationsClassName: 'axome-prompt-paste'
        }
      }))
    )
    const position = model.getPositionAt(Math.min(caret, text.length))
    editor!.setPosition(position)
    editor!.revealPositionInCenterIfOutsideViewport(position)
    sync()
    fold(
      folds()
        .map((region) => region.start)
        .filter((line) => !(unfold && line === position.lineNumber))
    )
  }

  export function load(next: ChatPart[]): void {
    if (editor) fill(next, Number.MAX_SAFE_INTEGER, false)
  }
  export function focus(insert?: string): void {
    editor?.focus()
    if (insert) editor?.trigger('keyboard', 'type', { text: insert })
  }
  /** The caret as a text offset; null until the editor exists. */
  export function caret(): number | null {
    const position = editor?.getPosition()
    return position ? model.getOffsetAt(position) : null
  }
  /** Types a part at the caret as its token; a chip keeps whitespace around it so the token reads back. */
  export function insertPart(part: ChatPart): void {
    if (!editor) return
    hold([part])
    const selection = editor.getSelection()!
    let text = partsText([part])
    if (part.type !== 'text' && part.type !== 'paste') {
      const { Range } = monaco
      const before = model.getValueInRange(
        new Range(
          selection.startLineNumber,
          Math.max(1, selection.startColumn - 1),
          selection.startLineNumber,
          selection.startColumn
        )
      )
      const after = model.getValueInRange(
        new Range(selection.endLineNumber, selection.endColumn, selection.endLineNumber, selection.endColumn + 1)
      )
      text = (before && !/\s/.test(before) ? ' ' : '') + text + (/\s/.test(after) ? '' : ' ')
    }
    const start = model.getOffsetAt(selection.getStartPosition())
    // No forceMoveMarkers: it would stretch a paste region ending at the caret over the token.
    editor.executeEdits('prompt', [{ range: selection, text }])
    editor.setPosition(model.getPositionAt(start + text.length))
    editor.focus()
  }

  // Monaco keeps its own keys: Enter, arrows, Tab and typing are never offered here, and the
  // `native` commands' strokes (move line up, page keys, Esc closing its popups) stop at this scope.
  const scope: KeyScope = {
    text: true,
    native: ['enterTranscript', 'pageUp', 'pageDown', 'stop', 'leave'],
    commands: {
      textView: () => onClose(),
      textSend: () => {
        if (disabled) return false
        onSend(false)
      },
      followUp: () => {
        if (disabled) return false
        onSend(true)
      }
    }
  }

  onMount(() => {
    let disposed = false
    let observer: ResizeObserver | undefined
    void (async () => {
      const { loadMonaco } = await import('$lib/features/editor/renderers/monaco/core/monacoEnv')
      const loaded = await loadMonaco()
      if (disposed) return
      monaco = loaded
      registerPromptLanguage(monaco)
      model = monaco.editor.createModel('', PROMPT_LANGUAGE)
      model.setEOL(monaco.editor.EndOfLineSequence.LF)
      promptModels.set(model, { folds, mentions })
      const created = monaco.editor.create(container, {
        model,
        theme: AXOME_THEME_NAME,
        readOnly: disabled,
        placeholder: `Write in markdown. @ for files. ${chatShortcut('textSend')} sends.`,
        // The textarea keeps ChatKeys reading this as a text field; EditContext input is not one.
        editContext: false,
        // Suggest and marker popups escape the composer's box.
        fixedOverflowWidgets: true,
        automaticLayout: false,
        wordWrap: 'on',
        minimap: { enabled: false },
        stickyScroll: { enabled: false },
        lineNumbers: 'on',
        lineNumbersMinChars: 3,
        glyphMargin: false,
        folding: true,
        showFoldingControls: 'always',
        renderLineHighlight: 'none',
        renderWhitespace: 'none',
        guides: { indentation: false },
        occurrencesHighlight: 'off',
        selectionHighlight: false,
        overviewRulerLanes: 0,
        hideCursorInOverviewRuler: true,
        overviewRulerBorder: false,
        scrollBeyondLastLine: false,
        scrollbar: { verticalScrollbarSize: 6, horizontalScrollbarSize: 6, useShadows: false },
        quickSuggestions: false,
        wordBasedSuggestions: 'off',
        suggestOnTriggerCharacters: true,
        codeLens: false,
        lightbulb: { enabled: monaco.editor.ShowLightbulbIconMode.Off },
        dropIntoEditor: { enabled: false },
        fontSize: getCodeFontSize(),
        lineHeight: CODE_LINE_HEIGHT,
        fontFamily: getCodeFontFamily(),
        fontLigatures: false,
        tabSize: 2,
        insertSpaces: true,
        padding: { top: 8, bottom: 8 }
      })
      regions = created.createDecorationsCollection()
      marks = created.createDecorationsCollection()
      created.onDidChangeModelContent(() => {
        if (!filling) sync()
      })
      // A large paste is a region, as it would be a chip in the chat input; inside a paste it is just more of it.
      created.onDidPaste(({ range }) => {
        if (!isLargePaste(model.getValueInRange(range)) || pasteRanges().some((held) => held.containsRange(range)))
          return
        regions.append([
          {
            range,
            options: {
              stickiness: monaco.editor.TrackedRangeStickiness.NeverGrowsWhenTypingAtEdges,
              linesDecorationsClassName: 'axome-prompt-paste'
            }
          }
        ])
        sync()
        if (range.endLineNumber - 1 > range.startLineNumber) fold([range.startLineNumber])
      })
      created.onMouseDown((event) => {
        const position = event.target.position
        if (!position || !(event.event.ctrlKey || event.event.metaKey)) return
        const offset = model.getOffsetAt(position)
        const hit = refs.find((ref) => ref.part && ref.from <= offset && offset < ref.to)
        if (!hit?.part) return
        event.event.preventDefault()
        openPart(hit.part, owner)
      })
      // Grows with the text up to two thirds of the conversation pane, then scrolls.
      const fit = () => {
        const pane = container.closest('[data-slot="conversation"]')
        const limit = Math.round(((pane?.clientHeight ?? window.innerHeight) * 2) / 3)
        const height = Math.max(56, Math.min(created.getContentHeight(), limit))
        container.style.height = `${height}px`
        created.layout({ width: container.clientWidth, height })
      }
      created.onDidContentSizeChange(fit)
      observer = new ResizeObserver(fit)
      observer.observe(container)
      editor = created
      fill(parts, open.caret, open.unfold)
      created.focus()
      void ensureProjectFiles(owner.folderPath).then(
        () => {
          if (disposed) return
          projectLoaded = true
          sync()
        },
        () => {}
      )
    })().catch((error) => {
      if (disposed) return
      notify.error('Text view unavailable', getErrorMessage(error))
      onClose()
    })
    return () => {
      disposed = true
      observer?.disconnect()
      if (model) {
        monaco.editor.setModelMarkers(model, PROMPT_LANGUAGE, [])
        promptModels.delete(model)
      }
      editor?.dispose()
      model?.dispose()
    }
  })

  $effect(() => {
    editor?.updateOptions({ readOnly: disabled, fontFamily: getCodeFontFamily(), fontSize: getCodeFontSize() })
  })
</script>

<div data-slot="prompt-text-view">
  <div class="flex items-center gap-3 border-b border-edge px-4 py-1.5 text-xs text-muted">
    <span class="font-medium text-fg">Text view</span>
    <span class="text-subtle">markdown</span>
    <span class="ms-auto flex items-center gap-1.5"><KeyHint keys={chatShortcut('textView')} /> chips</span>
    <span class="flex items-center gap-1.5"
      ><KeyHint keys={chatShortcut('textSend')} /> {running ? 'steer' : 'send'}</span
    >
    {#if running && canFollowUp}
      <span class="flex items-center gap-1.5"><KeyHint keys={chatShortcut('followUp')} /> follow-up</span>
    {/if}
  </div>
  <!-- Pasted files never reach Monaco: the composer turns them into attachments. -->
  <div
    bind:this={container}
    role="presentation"
    class="min-h-14 w-full"
    onpastecapture={(event) => {
      if (!event.clipboardData?.files.length) return
      event.preventDefault()
      event.stopPropagation()
      onFiles(event.clipboardData)
    }}
    {@attach keys.scope(() => scope)}
  ></div>
</div>
