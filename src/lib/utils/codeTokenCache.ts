/** Bounded LRU of in-flight/completed tokenizations, shared across tool body remounts. */
export function cachedCodeTokens<T>(
  tokenize: (code: string, language: string) => Promise<T>,
  maxCharacters = 256_000,
  maxEntries = 64
): (code: string, language: string) => Promise<T> {
  const cache = new Map<string, { size: number; value: Promise<T> }>()
  let characters = 0
  return (code, language) => {
    const key = `${language}\0${code}`
    const cached = cache.get(key)
    if (cached) {
      cache.delete(key)
      cache.set(key, cached)
      return cached.value
    }
    const value = tokenize(code, language)
    if (key.length > maxCharacters || maxEntries < 1) return value
    while (cache.size && (characters + key.length > maxCharacters || cache.size >= maxEntries)) {
      const oldest = cache.keys().next().value!
      characters -= cache.get(oldest)!.size
      cache.delete(oldest)
    }
    const entry = { size: key.length, value }
    cache.set(key, entry)
    characters += entry.size
    void value.catch(() => {
      if (cache.get(key) !== entry) return
      cache.delete(key)
      characters -= entry.size
    })
    return value
  }
}
