import { backend } from '$lib/api/backend'
import type { ToolStep } from '$lib/features/chat/turns'
import { notify } from '$lib/features/notifications/state.svelte'
import { openLink, resolveFileLocationPath } from '$lib/features/workbench/links/openLink'
import { isSchemeUrl, parseFilePathAndLocation } from '$lib/features/workbench/links/parseLink'
import { getErrorMessage } from '$lib/utils/client-error'
import { basename } from '$lib/utils/paths'
import { record } from './payload'

/** Preserve scheme URLs for the owning session; filesystem paths keep the existing reveal callback. */
export async function openToolLocation(
  path: string,
  line: number | undefined,
  owner: { folderPath: string; sessionId: string | null },
  onOpenLocation: (path: string, line?: number) => void
): Promise<void> {
  if (isSchemeUrl(path)) {
    const hasLocation = parseFilePathAndLocation(path).reveal !== null
    await openLink(line === undefined || hasLocation ? path : `${path}:${line}`, owner)
  } else {
    onOpenLocation(path, line)
  }
}

/** Search output paths are relative to the tool root, except single-file grep's basename. */
export async function openReferenceResult(
  call: ToolStep,
  owner: { folderPath: string; sessionId: string | null },
  resultPath: string,
  line: number | undefined,
  onOpenLocation: (path: string, line?: number) => void
): Promise<void> {
  try {
    if (isSchemeUrl(resultPath)) {
      await openToolLocation(resultPath, line, owner, onOpenLocation)
      return
    }
    const args = record(call.arguments)
    const rootPath = typeof args.path === 'string' ? args.path : '.'
    if (isSchemeUrl(rootPath)) {
      if (!owner.sessionId) throw new Error('Search reference has no owning session')
      const target = await backend.chat.resolveLink(owner.folderPath, owner.sessionId, rootPath)
      if (target.kind !== 'file') throw new Error('Search root is not a file or directory')
      const url = target.isDir ? `${rootPath.endsWith('/') ? rootPath : `${rootPath}/`}${resultPath}` : rootPath
      await openToolLocation(url, line, owner, onOpenLocation)
      return
    }
    const root = await resolveFileLocationPath(rootPath, owner.folderPath)
    // Single-file grep reports the file's own basename; any other root is a directory holding the result.
    const fileRoot =
      call.name === 'grep' &&
      !resultPath.includes('/') &&
      basename(root) === resultPath &&
      !(await backend.folders.resolve(root).catch(() => null))
    onOpenLocation(fileRoot ? root : `${root.replace(/\/+$/, '')}/${resultPath}`, line)
  } catch (error) {
    notify.error('Cannot open reference', getErrorMessage(error))
  }
}
