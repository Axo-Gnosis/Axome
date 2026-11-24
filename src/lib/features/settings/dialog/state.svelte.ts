import { untrack } from 'svelte'
import type { Attachment } from 'svelte/attachments'
import { getActiveFolderPath } from '$lib/features/workbench/state.svelte'
import { invalidateSettingsLoad } from '../state/settings.svelte'
import { splitRemotePath } from '$lib/utils/paths'

let settingsOpen = $state(false)
let settingsPage = $state('appearance')
let openingFolder = $state<string | null>(null)
let extensionTarget = $state.raw<ExtensionSettingsTarget | null>(null)
let extensionContextFolder = $state<string | null>(null)

export type ExtensionSettingsTarget = {
  hostFolder: string | null
  projectFolder: string | null
  id: string | null
}

export function getExtensionSettingsTarget(): ExtensionSettingsTarget | null {
  return extensionTarget
}

export function getExtensionContextFolder(): string | null {
  return extensionTarget ? extensionContextFolder : openingFolder
}

export function getSettingsLoadFolder(): string | null {
  return extensionTarget ? extensionTarget.hostFolder : settingsOpen ? openingFolder : getActiveFolderPath()
}

export function openExtensionSettings(target: ExtensionSettingsTarget): void {
  setSettingsOpen(true)
  const host = target.hostFolder ? splitRemotePath(target.hostFolder)?.server : undefined
  const currentFolder = getExtensionContextFolder()
  extensionContextFolder =
    target.projectFolder ?? (currentFolder && splitRemotePath(currentFolder)?.server === host ? currentFolder : null)
  const previous = getSettingsLoadFolder()
  if (previous !== target.hostFolder && (host || (previous && splitRemotePath(previous)))) invalidateSettingsLoad()
  extensionTarget = target
  settingsPage = 'extensions'
}

export function getSettingsPage(): string {
  return settingsPage
}

export function setSettingsPage(page: string): void {
  if (page !== 'extensions' && extensionTarget) {
    if (
      extensionTarget.hostFolder !== openingFolder &&
      ((extensionTarget.hostFolder && splitRemotePath(extensionTarget.hostFolder)) ||
        (openingFolder && splitRemotePath(openingFolder)))
    )
      invalidateSettingsLoad()
    extensionTarget = null
    extensionContextFolder = null
  }
  settingsPage = page
}

export function isSettingsOpen(): boolean {
  return settingsOpen
}

export function setSettingsOpen(open: boolean) {
  if (settingsOpen === open) return
  if (open) openingFolder = getActiveFolderPath()
  else {
    invalidateSettingsLoad()
    extensionTarget = null
    extensionContextFolder = null
    openingFolder = null
  }
  settingsOpen = open
}

/** A section of the open settings page, listed under its sidebar entry. */
export interface PageSection {
  id: string
  title: string
  element: HTMLElement
}

let sections = $state.raw<PageSection[]>([])

/** Sections of the open page in document order. */
export function getPageSections(): PageSection[] {
  return sections
}

/** Lists a section in the sidebar while its element is in the DOM. */
// Attachments run inside an effect; reading `sections` there would make the
// attachment re-run on every write it performs, so both updates are untracked.
export function trackSection(id: string, title: string): Attachment<HTMLElement> {
  return (element) => {
    const entry = { id, title, element }
    untrack(() => {
      sections = [...sections, entry].sort((a, b) =>
        a.element.compareDocumentPosition(b.element) & Node.DOCUMENT_POSITION_FOLLOWING ? -1 : 1
      )
    })
    return () => {
      untrack(() => {
        sections = sections.filter((section) => section !== entry)
      })
    }
  }
}
