import { untrack } from 'svelte'
import type { Attachment } from 'svelte/attachments'
import { getActiveFolderPath } from '$lib/features/workbench/state.svelte'
import { invalidateSettingsLoad } from '../state/settings.svelte'
import { hostRoot } from '$lib/utils/paths'

let settingsOpen = $state(false)
let settingsPage = $state('appearance')
let openingFolder = $state<string | null>(null)

export type ExtensionSettingsTarget = {
  hostFolder: string | null
  projectFolder: string | null
  id: string | null
}

export type ExtensionDetailTarget = {
  id: string
  project: boolean
}

let extensionContext = $state.raw<{ host: string | null; projectFolder: string | null } | null>(null)
let extensionDetail = $state.raw<ExtensionDetailTarget | null>(null)
let scrollToProject = $state(false)

export function getExtensionDetail(): ExtensionDetailTarget | null {
  return extensionDetail
}

export function setExtensionDetail(detail: ExtensionDetailTarget | null): void {
  extensionDetail = detail
  scrollToProject = false
}

export function closeExtensionDetail(): void {
  const wasProject = extensionDetail?.project ?? false
  extensionDetail = null
  scrollToProject = wasProject
}

export function isExtensionProjectScroll(): boolean {
  return scrollToProject
}

export function clearExtensionProjectScroll(): void {
  scrollToProject = false
}

export function getExtensionContextFolder(): string | null {
  return extensionContext ? extensionContext.projectFolder : openingFolder
}

export function getSettingsLoadFolder(): string | null {
  return hostRoot(extensionContext ? extensionContext.host : settingsOpen ? openingFolder : getActiveFolderPath())
}

export function openExtensionSettings(target: ExtensionSettingsTarget): void {
  setSettingsOpen(true)
  const host = hostRoot(target.hostFolder ?? target.projectFolder)
  const currentFolder = getExtensionContextFolder()
  const projectFolder =
    target.projectFolder ?? (currentFolder && hostRoot(currentFolder) === host ? currentFolder : null)
  extensionContext = { host, projectFolder }
  extensionDetail = target.id !== null ? { id: target.id, project: target.projectFolder !== null } : null
  scrollToProject = target.id === null && target.projectFolder !== null
  settingsPage = 'extensions'
}

export function getSettingsPage(): string {
  return settingsPage
}

export function setSettingsPage(page: string): void {
  if (page !== 'extensions' && (extensionContext || extensionDetail)) {
    extensionContext = null
    extensionDetail = null
    scrollToProject = false
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
    extensionContext = null
    extensionDetail = null
    scrollToProject = false
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
