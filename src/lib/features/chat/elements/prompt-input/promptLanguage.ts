import type * as Monaco from 'monaco-editor'
import { conf, language } from 'monaco-editor/esm/vs/basic-languages/markdown/markdown.js'
import type { Mention } from './mentions'

/** Markdown under its own id: providers are window-wide per language, and these belong to the composer only. */
export const PROMPT_LANGUAGE = 'axo-prompt'

/** What the language's providers ask the text view that owns a model. */
export interface PromptModel {
  /** Paste regions that can fold, as 1-based line ranges. */
  folds(): { start: number; end: number }[]
  mentions(text: string): Promise<Mention[]>
}
export const promptModels = new WeakMap<Monaco.editor.ITextModel, PromptModel>()

let registered = false

export function registerPromptLanguage(monaco: typeof Monaco): void {
  if (registered) return
  registered = true
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
      const typed = /(?:^|\s)@(\S*)$/.exec(before)?.[1]
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
          insertText: mention.folder
            ? (mention.complete ?? mention.path)
            : /\s/.test(mention.path)
              ? `"${mention.path}"`
              : mention.path,
          filterText: typed,
          sortText: String(index).padStart(3, '0'),
          range,
          // A folder lists its contents next, as Tab does in the chat input's picker.
          command: mention.folder ? { id: 'editor.action.triggerSuggest', title: '' } : undefined
        }))
      }
    }
  })
}
