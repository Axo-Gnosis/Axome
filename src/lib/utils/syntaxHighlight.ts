import type { BundledLanguage, Highlighter, ThemedToken } from 'shiki'

const SHIKI_THEME_NAME = 'axome'
let highlighterPromise: Promise<Highlighter> | null = null

function getHighlighter(): Promise<Highlighter> {
  return (highlighterPromise ??= import('shiki').then(
    ({ createHighlighter, createCssVariablesTheme, createJavaScriptRegexEngine }) =>
      createHighlighter({
        themes: [createCssVariablesTheme({ name: SHIKI_THEME_NAME, variablePrefix: '--shiki-' })],
        langs: [],
        // Match Monaco's engine; avoid the desktop runtime's crashing Oniguruma wasm path.
        engine: createJavaScriptRegexEngine({ forgiving: true })
      })
  ))
}

// Bounded LRU of in-flight/completed tokenizations, shared across tool body remounts.
const TOKEN_CACHE_CHARACTERS = 256_000
const TOKEN_CACHE_ENTRIES = 64
const tokenCache = new Map<string, { size: number; value: Promise<ThemedToken[][] | null> }>()
let cachedCharacters = 0

/** Tokenize with a lazily loaded bundled grammar; null when Shiki has none for `language`. */
export function codeTokens(code: string, language: string): Promise<ThemedToken[][] | null> {
  if (code.length + language.length + 1 > TOKEN_CACHE_CHARACTERS) return tokenizeCode(code, language)
  const key = `${language}\0${code}`
  const cached = tokenCache.get(key)
  if (cached) {
    tokenCache.delete(key)
    tokenCache.set(key, cached)
    return cached.value
  }
  const value = tokenizeCode(code, language)
  while (
    tokenCache.size &&
    (cachedCharacters + key.length > TOKEN_CACHE_CHARACTERS || tokenCache.size >= TOKEN_CACHE_ENTRIES)
  ) {
    const oldest = tokenCache.keys().next().value!
    cachedCharacters -= tokenCache.get(oldest)!.size
    tokenCache.delete(oldest)
  }
  const entry = { size: key.length, value }
  tokenCache.set(key, entry)
  cachedCharacters += entry.size
  void value.catch(() => {
    if (tokenCache.get(key) !== entry) return
    tokenCache.delete(key)
    cachedCharacters -= entry.size
  })
  return value
}

export type HighlightSpan = { text: string; color?: string }

/** Tokenizes `code` into contiguous spans preserving exact source whitespace and line endings. */
export async function highlightSpans(code: string, language: string): Promise<HighlightSpan[] | null> {
  const lines = await codeTokens(code, language)
  if (!lines) return null
  const spans: HighlightSpan[] = []
  let offset = 0
  // Slice the original source, including gaps between tokens, so line endings
  // and whitespace never depend on Shiki's line-oriented representation.
  for (const line of lines) {
    for (const token of line) {
      if (token.offset > offset) spans.push({ text: code.slice(offset, token.offset) })
      const end = token.offset + token.content.length
      spans.push({ text: code.slice(token.offset, end), color: token.color })
      offset = end
    }
  }
  if (offset < code.length) spans.push({ text: code.slice(offset) })
  return spans
}

async function tokenizeCode(code: string, language: string): Promise<ThemedToken[][] | null> {
  const highlighter = await getHighlighter()
  // This lazy boundary keeps Shiki's grammar registry out of startup; only known languages load it.
  const { bundledLanguages, bundledLanguagesAlias } = await import('shiki')
  const grammar =
    bundledLanguages[language as keyof typeof bundledLanguages] ??
    bundledLanguagesAlias[language as keyof typeof bundledLanguagesAlias]
  if (!grammar) return null
  await highlighter.loadLanguage(grammar)
  return highlighter.codeToTokens(code, { lang: language as BundledLanguage, theme: SHIKI_THEME_NAME }).tokens
}
