// The diff gutter's git context: which modified models take stage/revert actions, and the context
// keys their toolbars show on. No Monaco import, so diff rows can use it before Monaco loads; the
// commands and menu items live in `gitGutterCommands`, loaded with Monaco.

import type { LineChange } from '$lib/features/git/lineChanges'
import type { DiffModelStore } from '$lib/features/editor/renderers/monaco/diff/diffModels.svelte'
import type { GitStatusKind } from '$lib/types/backend'

export const AXOME_DIFF_GIT_ACTIONS_CONTEXT = 'axomeDiffGitActions'
export const AXOME_DIFF_STAGED_CONTEXT = 'axomeDiffStaged'

export interface DiffGutterRegistration {
  folderPath: string
  filePath: string
  status: GitStatusKind
  staged: boolean
  store: DiffModelStore
  getLineChanges: () => readonly LineChange[]
}

const registrations = new Map<string, DiffGutterRegistration>()

export function diffGutterRegistration(modifiedUri: string): DiffGutterRegistration | null {
  return registrations.get(modifiedUri) ?? null
}

export function registerAxomeDiffGutterContext(
  modifiedUri: string,
  registration: DiffGutterRegistration
): { dispose(): void } {
  registrations.set(modifiedUri, registration)
  return {
    dispose: () => {
      if (registrations.get(modifiedUri) === registration) {
        registrations.delete(modifiedUri)
      }
    }
  }
}

if (import.meta.hot) import.meta.hot.dispose(() => registrations.clear())
