/** `memory://main/Axome/Pause menu.md` → { bank: 'main', note: 'Axome/Pause menu' }; null for anything else. */
export function memoryNote(url: string): { bank: string; note: string } | null {
  const match = /^memory:\/\/([^/]+)\/(.+?)(?:\.md)?$/.exec(url)
  return match ? { bank: match[1], note: match[2] } : null
}
