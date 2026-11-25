export type BlockView = 'collapsed' | 'preview' | 'full'

/** Whether a fold is open: a manual choice in `overrides` wins over `fallback`. */
export function isOpen(overrides: Record<string, BlockView>, id: string, fallback: BlockView): boolean {
  return (overrides[id] ?? fallback) !== 'collapsed'
}
