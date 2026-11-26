import type { ChatPart } from '$lib/types/backend'
import { openFileLocation, openLink } from '$lib/features/workbench/links/openLink'
import { isSchemeUrl } from '$lib/features/workbench/links/parseLink'

/** Opens what a file or saved part points at; other parts have nothing to open. */
export function openPart(part: ChatPart, owner: { folderPath: string; sessionId: string | null }): void {
  if (part.type === 'file')
    void (isSchemeUrl(part.path) ? openLink(part.path, owner) : openFileLocation(part.path, null, owner.folderPath))
  else if (part.type === 'saved') void openLink(part.url, owner)
}
