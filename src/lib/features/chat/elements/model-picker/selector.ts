import type { ChatModel, ChatModelRole, ChatThinking } from '$lib/types/backend'

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

/** Splits an axo selector `provider/model:effort`; a suffix that isn't an effort level stays in the model. */
export function splitSelector(selector: string): { model: string; effort: string } {
  const colon = selector.lastIndexOf(':')
  const effort = selector.slice(colon + 1)
  return colon > 0 && (THINKING_LEVELS as readonly string[]).includes(effort)
    ? { model: selector.slice(0, colon), effort }
    : { model: selector, effort: '' }
}

/** Joins a model and effort into one selector; roles and empty models take no suffix. */
export function joinSelector(model: string, effort: string): string {
  return model && effort && !model.startsWith('@') ? `${model}:${effort}` : model
}

/** Supported effort levels for a model or `@role` selector; falls back to all levels when unset or unknown. */
export function levelsForSelector(
  selector: string,
  models: readonly ChatModel[],
  roles: readonly ChatModelRole[] = []
): readonly ChatThinking[] {
  const { model: base } = splitSelector(selector)
  if (!base) return THINKING_LEVELS
  if (base.startsWith('@')) {
    const role = roles.find((item) => `@${item.name}` === base)
    if (!role?.model) return THINKING_LEVELS
    const target = models.find((item) => item.selector === splitSelector(role.model!).model)
    return target?.thinking ?? THINKING_LEVELS
  }
  const target = models.find((item) => item.selector === base)
  return target?.thinking ?? THINKING_LEVELS
}

export const ROLE_NAME = /^[a-z][a-z0-9-]*$/
