// Monaco's one-time setup: workers, services, theme, keybinding overrides and the
// host's language support. Loaded only through `loadMonaco` (core/monaco.ts).

import EditorWorker from 'monaco-editor/esm/vs/editor/editor.worker?worker'
import CssWorker from 'monaco-editor/esm/vs/language/css/css.worker?worker'
import HtmlWorker from 'monaco-editor/esm/vs/language/html/html.worker?worker'
import JsonWorker from 'monaco-editor/esm/vs/language/json/json.worker?worker'
import TsWorker from 'monaco-editor/esm/vs/language/typescript/ts.worker?worker'
import { initializeMonacoEditorServices } from '$lib/features/editor/renderers/monaco/core/editorServices'
import { ensureFolderLanguageSupport } from '$lib/features/editor/renderers/monaco/core/languageSupport'
import { StandaloneServices } from 'monaco-editor/esm/vs/editor/standalone/browser/standaloneServices.js'
import { IKeybindingService } from 'monaco-editor/esm/vs/platform/keybinding/common/keybinding.js'
import {
  attachMonacoKeybindingOverrides,
  type KeybindingService
} from '$lib/features/editor/renderers/monaco/core/keybindings'
import { ensureMonacoLsp } from '$lib/features/editor/lsp/registry'
import { AXOME_THEME_NAME, registerAxomeTheme } from '$lib/features/editor/renderers/monaco/core/monacoTheme'
import { attachMonaco } from '$lib/features/editor/schemas/registry'
import type { Monaco } from './monaco'

self.MonacoEnvironment = {
  getWorker(_workerId: string, label: string): Worker {
    if (label === 'typescript' || label === 'javascript') return new TsWorker()
    if (label === 'json') return new JsonWorker()
    if (label === 'css' || label === 'less' || label === 'scss') return new CssWorker()
    if (label === 'html' || label === 'handlebars' || label === 'razor') return new HtmlWorker()
    return new EditorWorker()
  }
}

interface TsLanguageDefaults {
  modeConfiguration: {
    documentRangeFormattingEdits?: boolean
    onTypeFormattingEdits?: boolean
  }
  setDiagnosticsOptions(options: {
    noSemanticValidation: boolean
    noSyntaxValidation: boolean
    noSuggestionDiagnostics: boolean
  }): void
  setModeConfiguration(options: { documentRangeFormattingEdits?: boolean; onTypeFormattingEdits?: boolean }): void
}

interface CssLanguageDefaults {
  setDiagnosticsOptions(options: { validate?: boolean }): void
}

interface JsonLanguageDefaults {
  modeConfiguration: {
    documentFormattingEdits?: boolean
    documentRangeFormattingEdits?: boolean
  }
  setModeConfiguration(options: { documentFormattingEdits?: boolean; documentRangeFormattingEdits?: boolean }): void
}

const DISABLED_TS_DIAGNOSTICS = {
  noSemanticValidation: true,
  noSyntaxValidation: true,
  noSuggestionDiagnostics: true
} as const

function getTsLanguageService(monaco: Monaco): {
  javascriptDefaults: TsLanguageDefaults
  typescriptDefaults: TsLanguageDefaults
} {
  return monaco.languages.typescript as unknown as {
    javascriptDefaults: TsLanguageDefaults
    typescriptDefaults: TsLanguageDefaults
  }
}

function getCssLanguageServices(monaco: Monaco): CssLanguageDefaults[] {
  const cssLanguages = monaco.languages.css as unknown as {
    cssDefaults: CssLanguageDefaults
    scssDefaults: CssLanguageDefaults
    lessDefaults: CssLanguageDefaults
  }

  return [cssLanguages.cssDefaults, cssLanguages.scssDefaults, cssLanguages.lessDefaults]
}

function getJsonLanguageService(monaco: Monaco): JsonLanguageDefaults {
  return (monaco.languages.json as unknown as { jsonDefaults: JsonLanguageDefaults }).jsonDefaults
}

/** Loads Monaco and sets it up; `loadMonaco` runs it once. */
export async function setupMonaco(): Promise<Monaco> {
  const monaco = await import('monaco-editor')
  initializeMonacoEditorServices()
  registerAxomeTheme(monaco)
  monaco.editor.setTheme(AXOME_THEME_NAME)
  // Monaco caches glyph metrics per font. A face that finishes loading after
  // the first measurement (bundled Paper Mono, a newly chosen code font)
  // would otherwise keep the fallback's widths and misplace the cursor.
  document.fonts.addEventListener('loadingdone', () => monaco.editor.remeasureFonts())

  // Keep Monaco's built-in language features as fallback when an
  // external LSP is missing or disabled, but suppress TS diagnostics
  // because Monaco's standalone TS worker misreads Axome's project
  // alias/runtime environment and produces false positives.
  const tsLanguageService = getTsLanguageService(monaco)
  tsLanguageService.javascriptDefaults.setDiagnosticsOptions(DISABLED_TS_DIAGNOSTICS)
  tsLanguageService.typescriptDefaults.setDiagnosticsOptions(DISABLED_TS_DIAGNOSTICS)
  tsLanguageService.javascriptDefaults.setModeConfiguration({
    ...tsLanguageService.javascriptDefaults.modeConfiguration,
    documentRangeFormattingEdits: false,
    onTypeFormattingEdits: false
  })
  tsLanguageService.typescriptDefaults.setModeConfiguration({
    ...tsLanguageService.typescriptDefaults.modeConfiguration,
    documentRangeFormattingEdits: false,
    onTypeFormattingEdits: false
  })

  const jsonLanguageService = getJsonLanguageService(monaco)
  jsonLanguageService.setModeConfiguration({
    ...jsonLanguageService.modeConfiguration,
    documentFormattingEdits: false,
    documentRangeFormattingEdits: false
  })

  // CSS diagnostics stay off even without an external LSP so Tailwind
  // at-rules like @source and @apply do not regress to false errors.
  for (const cssLanguageService of getCssLanguageServices(monaco)) {
    cssLanguageService.setDiagnosticsOptions({ validate: false })
  }

  await ensureMonacoLsp(monaco)
  await ensureFolderLanguageSupport(monaco, null)

  const { KeyMod, KeyCode } = monaco
  monaco.editor.addKeybindingRules([
    { keybinding: KeyMod.CtrlCmd | KeyMod.Shift | KeyCode.KeyP, command: '-editor.action.quickCommand' },
    { keybinding: KeyCode.F1, command: '-editor.action.quickCommand' },
    { keybinding: KeyMod.CtrlCmd | KeyMod.Shift | KeyCode.KeyO, command: '-editor.action.quickOutline' }
  ])
  attachMonacoKeybindingOverrides(monaco, StandaloneServices.get(IKeybindingService) as KeybindingService)

  // Flush any JSON schemas that were registered before Monaco
  // finished loading (bootstrap runs before the first Monaco load).
  attachMonaco(monaco)
  return monaco
}
