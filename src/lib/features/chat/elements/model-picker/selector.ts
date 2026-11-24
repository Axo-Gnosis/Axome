import type { ChatModel, ChatModelRole, ChatServiceTier, ChatThinking } from '$lib/types/backend'

export const THINKING_LEVELS: readonly ChatThinking[] = ['off', 'minimal', 'low', 'medium', 'high', 'xhigh', 'max']

export const EFFORT_DESCRIPTIONS: Readonly<Record<ChatThinking, string>> = {
  off: 'No reasoning',
  minimal: 'Fast reasoning',
  low: 'Light reasoning',
  medium: 'Balanced reasoning',
  high: 'Deeper reasoning',
  xhigh: 'Extensive reasoning',
  max: 'Maximum reasoning'
}

export const SERVICE_TIERS: readonly ChatServiceTier[] = ['standard', 'flex', 'fast', 'ultrafast']

export const TIER_LABELS: Readonly<Record<ChatServiceTier, string>> = {
  standard: 'Standard',
  flex: 'Flex',
  fast: 'Fast',
  ultrafast: 'Ultrafast'
}

export interface SelectorParts {
  model: string
  effort: ChatThinking | null
  tier: ChatServiceTier | null
}

export function splitSelector(selector: string, models?: readonly ChatModel[]): SelectorParts | null {
  const parts: SelectorParts = { model: selector, effort: null, tier: null }
  const role = selector.startsWith('@')
  const concrete = !role && !selector.startsWith(':')
  for (;;) {
    // Like axo, take the first registry match even if it has more colons.
    if (concrete && models?.some((model) => model.selector === parts.model)) return parts
    const colon = parts.model.lastIndexOf(':')
    if (colon === -1) return !role || ROLE_NAME.test(parts.model.slice(1)) ? parts : null
    const suffix = parts.model.slice(colon + 1)
    const effort = THINKING_LEVELS.find((level) => level === suffix)
    const tier = SERVICE_TIERS.find((tier) => tier === suffix)
    if (effort && parts.effort === null) parts.effort = effort
    else if (tier && parts.tier === null) parts.tier = tier
    else return null

    parts.model = parts.model.slice(0, colon)
  }
}

export function joinSelector(model: string, effort: ChatThinking | null, tier: ChatServiceTier | null): string {
  return model + (effort ? `:${effort}` : '') + (tier ? `:${tier}` : '')
}

export interface ResolvedSelector {
  parts: SelectorParts
  model: ChatModel | undefined
  role: ChatModelRole | undefined
}

export function resolveSelector(
  selector: string,
  models: readonly ChatModel[],
  roles: readonly ChatModelRole[] = []
): ResolvedSelector | null {
  const parts = splitSelector(selector, models)
  if (!parts) return null
  const role = parts.model.startsWith('@') ? roles.find((candidate) => `@${candidate.name}` === parts.model) : undefined
  let model: ChatModel | undefined
  if (role) {
    const target = role.model ? splitSelector(role.model, models)?.model : undefined
    model = target && !target.startsWith('@') ? models.find((candidate) => candidate.selector === target) : undefined
  } else if (parts.model && !parts.model.startsWith('@')) {
    model = models.find((candidate) => candidate.selector === parts.model)
  }
  return { parts, model, role }
}

function matchesSelector(selector: string, wanted: SelectorParts, models: readonly ChatModel[]): boolean {
  const actual = splitSelector(selector, models)
  return actual?.model === wanted.model && actual.effort === wanted.effort && actual.tier === wanted.tier
}

export function canonicalSelector(
  parts: SelectorParts,
  models: readonly ChatModel[],
  original?: string
): string | null {
  const canonical = joinSelector(parts.model, parts.effort, parts.tier)
  if (matchesSelector(canonical, parts, models)) return canonical
  if (original && matchesSelector(original, parts, models)) return original
  // Some model IDs are spelled like selectors. Never save options for a different model.
  if (parts.tier && parts.effort) {
    const reversed = `${parts.model}:${parts.tier}:${parts.effort}`
    if (matchesSelector(reversed, parts, models)) return reversed
  }
  return null
}

export function modelForSelector(
  selector: string,
  models: readonly ChatModel[],
  roles: readonly ChatModelRole[] = []
): ChatModel | undefined {
  return resolveSelector(selector, models, roles)?.model
}

/** Falls back to all levels when the model isn't known. */
export function levelsForSelector(
  selector: string,
  models: readonly ChatModel[],
  roles?: readonly ChatModelRole[]
): readonly ChatThinking[] {
  return modelForSelector(selector, models, roles)?.thinking ?? THINKING_LEVELS
}

export const ROLE_NAME = /^[a-z][a-z0-9-]*$/
