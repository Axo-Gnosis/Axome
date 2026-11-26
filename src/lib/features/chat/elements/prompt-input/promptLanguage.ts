import type * as Monaco from 'monaco-editor'
import { backend } from '$lib/api/backend'
import { partText } from '$lib/features/chat/parts/parts'
import { mentionAt, type Mention } from './mentions'
import type { TypedRun } from './predict'

/** Markdown under its own id: providers are window-wide per language, and these belong to the composer only. */
export const PROMPT_LANGUAGE = 'axo-prompt'

/** What the language's providers ask the text view that owns a model. */
export interface PromptModel {
  /** Paste regions that can fold, as 1-based line ranges. */
  folds(): { start: number; end: number }[]
  mentions(text: string): Promise<Mention[]>
  /** Typed run around `offset`, or null inside a paste or resolved `@ref`. */
  predict(offset: number): TypedRun | null
}
export const promptModels = new WeakMap<Monaco.editor.ITextModel, PromptModel>()

let registered: Promise<void> | null = null

export function registerPromptLanguage(monaco: typeof Monaco): Promise<void> {
  registered ??= register(monaco).catch((error) => {
    registered = null
    throw error
  })
  return registered
}

async function register(monaco: typeof Monaco): Promise<void> {
  // Monaco's markdown grammar loads with Monaco, not with the composer.
  const { conf, language } = await import('monaco-editor/esm/vs/basic-languages/markdown/markdown.js')
  monaco.languages.register({ id: PROMPT_LANGUAGE })
  monaco.languages.setLanguageConfiguration(PROMPT_LANGUAGE, conf)
  monaco.languages.setMonarchTokensProvider(PROMPT_LANGUAGE, language)
  monaco.languages.registerFoldingRangeProvider(PROMPT_LANGUAGE, {
    provideFoldingRanges: (model) => promptModels.get(model)?.folds() ?? []
  })
  monaco.languages.registerCompletionItemProvider(PROMPT_LANGUAGE, {
    // Path characters that end Monaco's word re-open the list; the provider answers only inside an `@` mention.
    triggerCharacters: ['@', '/', '.', '-', '_'],
    async provideCompletionItems(model, position) {
      const owner = promptModels.get(model)
      const before = model.getLineContent(position.lineNumber).slice(0, position.column - 1)
      const typed = mentionAt.exec(before)?.[1]
      if (!owner || typed === undefined) return { suggestions: [] }
      const range = new monaco.Range(
        position.lineNumber,
        position.column - typed.length,
        position.lineNumber,
        position.column
      )
      const mentions = await owner.mentions(typed)
      return {
        // Ranking is ours (`rankPaths`): every row passes Monaco's filter and the list is asked again per key.
        incomplete: true,
        suggestions: mentions.map((mention, index) => ({
          label: { label: mention.label, description: mention.detail },
          kind: mention.folder ? monaco.languages.CompletionItemKind.Folder : monaco.languages.CompletionItemKind.File,
          // The `@` is typed already; a folder inserts the text that lists its contents.
          insertText: mention.folder
            ? (mention.complete ?? mention.path)
            : partText({ type: 'file', path: mention.path }).slice(1),
          filterText: typed,
          sortText: String(index).padStart(3, '0'),
          range,
          // A folder lists its contents next, as Tab does in the chat input's picker.
          command: mention.folder ? { id: 'editor.action.triggerSuggest', title: '' } : undefined
        }))
      }
    }
  })
  monaco.languages.registerInlineCompletionsProvider(PROMPT_LANGUAGE, {
    async provideInlineCompletions(model, position, _context, token) {
      const owner = promptModels.get(model)
      if (!owner || token.isCancellationRequested) return { items: [] }
      const run = owner.predict(model.getOffsetAt(position))
      if (!run || !run.before) return { items: [] }
      try {
        const suggestion = await backend.predict.complete(run.before, run.after)
        if (token.isCancellationRequested || !suggestion?.text) return { items: [] }
        return {
          items: [
            {
              insertText: suggestion.text,
              range: new monaco.Range(position.lineNumber, position.column, position.lineNumber, position.column)
            }
          ]
        }
      } catch {
        return { items: [] }
      }
    },
    disposeInlineCompletions() {}
  })
}
