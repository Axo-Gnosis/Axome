import type { ShortcutCommandDefinition } from '$lib/features/command-palette/commands/registry.svelte'
import { getEffectiveBindings } from '$lib/features/command-palette/shortcuts/overrides.svelte'
import { formatShortcut, normalizeShortcut } from '$lib/features/command-palette/shortcuts/spec'
import { CHAT_COMMANDS, CHAT_GROUPS, chatCommandNames, type ChatCommandId } from './commands'

export function chatBindings(name: ChatCommandId): string[] {
  return getEffectiveBindings(`chat:${name}`, [...CHAT_COMMANDS[name].keys])
}

/** The commands bound to a normalized stroke, in table order. */
export function chatCommandsFor(stroke: string): ChatCommandId[] {
  return chatCommandNames.filter((name) => chatBindings(name).some((binding) => normalizeShortcut(binding) === stroke))
}

/** The first binding of a command, formatted for hints; empty when unbound. */
export function chatShortcut(name: ChatCommandId): string {
  return formatShortcut(chatBindings(name)[0])
}

export function getChatShortcutCommands(): ShortcutCommandDefinition[] {
  return chatCommandNames.map((name) => {
    const { label, context, keys } = CHAT_COMMANDS[name]
    return {
      id: `chat:${name}`,
      label,
      group: CHAT_GROUPS[context],
      keywords: ['chat', context, name],
      source: 'chat',
      chatContext: context,
      defaultKeybindings: [...keys]
    }
  })
}
