import { platform } from '$lib/platform'
import type { AssetHandle } from '$lib/platform'
import { splitRemotePath } from '$lib/utils/paths'
import { CONTENT_LIMIT, CONTENT_LIMIT_MESSAGE } from '$lib/features/chat/parts/parts'

/**
 * The file part for `path` on the session's own host: project-relative inside `folderPath`,
 * otherwise the absolute path as that host sees it. Folders end in `/`.
 */
export function fileReference(path: string, folderPath: string, folder = false): string {
  const root = folderPath.replace(/\/$/, '') + '/'
  const reference = path.startsWith(root) ? path.slice(root.length) : (splitRemotePath(path)?.path ?? path)
  return folder ? reference.replace(/\/?$/, '/') : reference
}

/** Whether `path` is on the same host as `folderPath`, so axo there can open it. */
export function onHost(path: string, folderPath: string): boolean {
  return (splitRemotePath(path)?.server ?? null) === (splitRemotePath(folderPath)?.server ?? null)
}

/** `relative` (which may climb with `..`) joined onto absolute `base`, keeping an `axome://server` prefix. */
export function joinHostPath(base: string, relative: string): string {
  const remote = splitRemotePath(base)
  const segments: string[] = []
  for (const segment of `${remote?.path ?? base}/${relative}`.split('/')) {
    if (segment === '..') segments.pop()
    else if (segment && segment !== '.') segments.push(segment)
  }
  return `${remote ? `axome://${remote.server}` : ''}/${segments.join('/')}`
}

/** `name`, or `stem-2.ext`, `stem-3.ext`, … when taken: the numbering axo uses when it saves content. */
export function uniqueName(name: string, taken: string[]): string {
  const dot = name.lastIndexOf('.')
  for (let copy = 1; ; copy++) {
    const candidate =
      copy === 1 ? name : dot > 0 ? `${name.slice(0, dot)}-${copy}${name.slice(dot)}` : `${name}-${copy}`
    if (!taken.includes(candidate)) return candidate
  }
}

export function blobBase64(blob: Blob): Promise<string> {
  const { promise, resolve, reject } = Promise.withResolvers<string>()
  const reader = new FileReader()
  reader.onload = () => resolve(String(reader.result).split(',')[1])
  reader.onerror = () => reject(reader.error)
  reader.readAsDataURL(blob)
  return promise
}

/** Uses the same native/web asset boundary as the media viewer. */
export async function readAttachment(folder: string, path: string, limit: number): Promise<string> {
  const assets = platform.assets
  const abort = new AbortController()
  let loaded: AssetHandle | undefined
  try {
    const url =
      'url' in assets ? assets.url(folder, path) : (loaded = await assets.load(folder, path, abort.signal)).url
    const response = await fetch(url, { signal: abort.signal })
    if (!response.ok || !response.body) throw new Error('Could not read dropped file.')
    const reader = response.body.getReader()
    const chunks: Uint8Array<ArrayBuffer>[] = []
    let size = 0
    for (;;) {
      const { done, value } = await reader.read()
      if (done) break
      size += value.byteLength
      if (size > Math.min(limit, CONTENT_LIMIT)) throw new Error(CONTENT_LIMIT_MESSAGE)
      chunks.push(value)
    }
    return await blobBase64(new Blob(chunks))
  } finally {
    abort.abort()
    loaded?.dispose()
  }
}
