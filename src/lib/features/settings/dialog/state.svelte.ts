import { untrack } from 'svelte'
import type { Attachment } from 'svelte/attachments'

let settingsOpen = $state(false)
let settingsPage = $state('appearance')

export function getSettingsPage(): string {
  return settingsPage
}

export function setSettingsPage(page: string): void {
  settingsPage = page
}

export function isSettingsOpen(): boolean {
  return settingsOpen
}

export function setSettingsOpen(open: boolean) {
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
