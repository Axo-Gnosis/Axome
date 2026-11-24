import { untrack } from 'svelte'
import {
  getAppCommandDefinitions,
  getAppShortcutCommands,
  getEditorShortcutCommands,
  type ShortcutCommandDefinition
} from '$lib/features/command-palette/commands/registry.svelte'
import { getEffectiveBindings } from '$lib/features/command-palette/shortcuts/overrides.svelte'
import { normalizeShortcut } from '$lib/features/command-palette/shortcuts/spec'
import { chatBindingsOverlap } from '$lib/features/chat/keymap/commands'
import { getChatShortcutCommands } from '$lib/features/chat/keymap/bindings.svelte'

export interface ShortcutCommandInfo extends ShortcutCommandDefinition {
  effectiveKeybindings: string[]
}

export interface ShortcutConflict {
  command: ShortcutCommandInfo
  key: string
}

export function getCommandShortcut(id: string): string | undefined {
  return getEffectiveBindings(
    id,
    untrack(() => getAppCommandDefinitions().find((command) => command.id === id)?.defaultKeybindings) ?? []
  )[0]
}

function getShortcutCommands(): ShortcutCommandDefinition[] {
  return [...getAppShortcutCommands(), ...getEditorShortcutCommands(), ...getChatShortcutCommands()]
}

/** App bindings fire everywhere; editor and chat bindings only inside their own surface. */
function bindingsOverlap(a: ShortcutCommandDefinition, b: ShortcutCommandDefinition): boolean {
  if (a.source === 'app' || b.source === 'app') return true
  if (a.source !== b.source) return false
  return a.source !== 'chat' || chatBindingsOverlap(a.chatContext, b.chatContext)
}

function getShortcutCommandInfo(command: ShortcutCommandDefinition): ShortcutCommandInfo {
  return {
    ...command,
    effectiveKeybindings: getEffectiveBindings(command.id, command.defaultKeybindings)
  }
}

export function getShortcutInfos(): ShortcutCommandInfo[] {
  return getShortcutCommands().map(getShortcutCommandInfo)
}

export function findShortcutConflict(
  commandId: string,
  key: string,
  infos = getShortcutInfos()
): ShortcutConflict | null {
  const normalized = normalizeShortcut(key)
  if (!normalized) return null
  const own = infos.find((command) => command.id === commandId)
  for (const command of infos) {
    if (command.id === commandId || (own && !bindingsOverlap(own, command))) continue
    if (command.effectiveKeybindings.some((binding) => normalizeShortcut(binding) === normalized)) {
      return {
        command,
        key: command.effectiveKeybindings.find((binding) => normalizeShortcut(binding) === normalized) ?? key
      }
    }
  }
  return null
}
