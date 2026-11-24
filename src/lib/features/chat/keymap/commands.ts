/**
 * Where a chat binding applies. `surface` works anywhere in the chat view; `input` while typing in
 * a chat input; `nav` while moving through a list (the transcript, or the input's suggestions).
 * Input and nav share keys by design (Enter sends, or picks the highlighted item); surface keys
 * must stay unique (see `chatBindingsOverlap`).
 */
export type ChatContext = 'surface' | 'input' | 'nav'

interface ChatCommand {
  label: string
  context: ChatContext
  keys: readonly string[]
}

/** Every chat key, navigation included. Settings rebinds them by `chat:<name>`. */
export const CHAT_COMMANDS = {
  cycleThinking: { label: 'Cycle Thinking Effort', context: 'surface', keys: ['Alt+T'] },
  pickModel: { label: 'Pick Model', context: 'surface', keys: ['Alt+M'] },
  cycleDetails: { label: 'Cycle Work Details', context: 'surface', keys: ['Alt+O'] },
  jumpToQuestion: { label: 'Jump to Open Question', context: 'surface', keys: ['Alt+A'] },
  enterTranscript: { label: 'Move Into the Transcript', context: 'surface', keys: ['Alt+ArrowUp'] },
  pageUp: { label: 'Scroll Transcript Up a Page', context: 'surface', keys: ['PageUp'] },
  pageDown: { label: 'Scroll Transcript Down a Page', context: 'surface', keys: ['PageDown'] },

  send: { label: 'Send Message', context: 'input', keys: ['Enter'] },
  followUp: { label: 'Queue as Follow-Up', context: 'input', keys: ['Alt+Enter'] },
  newline: { label: 'Insert Newline', context: 'input', keys: ['Shift+Enter'] },
  complete: { label: 'Complete Suggestion', context: 'input', keys: ['Tab'] },
  stop: { label: 'Stop the Run', context: 'input', keys: ['Escape'] },

  previous: { label: 'Previous Item', context: 'nav', keys: ['ArrowUp'] },
  next: { label: 'Next Item', context: 'nav', keys: ['ArrowDown'] },
  first: { label: 'First Item', context: 'nav', keys: ['Home'] },
  last: { label: 'Last Item', context: 'nav', keys: ['End'] },
  activate: { label: 'Pick or Confirm', context: 'nav', keys: ['Enter'] },
  toggle: { label: 'Select Without Confirming', context: 'nav', keys: ['Space'] },
  decrease: { label: 'Step Value Down', context: 'nav', keys: ['ArrowLeft'] },
  increase: { label: 'Step Value Up', context: 'nav', keys: ['ArrowRight'] },
  leave: { label: 'Leave', context: 'nav', keys: ['Escape'] },
  sendAnswers: { label: 'Send Answers', context: 'nav', keys: ['Ctrl+Enter'] },
  dismiss: { label: 'Dismiss Question or Review', context: 'nav', keys: ['Alt+D'] }
} as const satisfies Record<string, ChatCommand>

export type ChatCommandId = keyof typeof CHAT_COMMANDS

export const chatCommandNames = Object.keys(CHAT_COMMANDS) as ChatCommandId[]

export const CHAT_GROUPS: Record<ChatContext, string> = {
  surface: 'Chat',
  input: 'Chat Input',
  nav: 'Chat Navigation'
}

/** Whether two chat contexts can hold focus together, so equal keys would collide. */
export function chatBindingsOverlap(a: ChatContext | undefined, b: ChatContext | undefined): boolean {
  return a === b || a === 'surface' || b === 'surface'
}
