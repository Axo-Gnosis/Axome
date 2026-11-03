import type { BundledLanguage, Highlighter, ThemedToken } from 'shiki'

export const SHIKI_THEME_NAME = 'axome'
let highlighterPromise: Promise<Highlighter> | null = null

export function getHighlighter(): Promise<Highlighter> {
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
