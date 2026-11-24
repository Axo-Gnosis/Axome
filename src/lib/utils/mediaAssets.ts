import { platform } from '$lib/platform'
import { dirname, splitRemotePath } from '$lib/utils/paths'

export const URL_SCHEME_RE = /^[a-zA-Z][a-zA-Z0-9+.-]*:/

// Browser-relative URLs point at Vite/Tauri chrome; use the project loader
// except for explicitly absolute paths in local desktop chat.
export async function markdownImageSrc(
  href: string | null | undefined,
  folderPath?: string,
  markdownPath?: string | null,
  loadLocalImage?: (filePath: string) => Promise<string | null>
): Promise<string | null> {
  if (!href) return ''
  const trimmed = href.trim()
  if (!trimmed || trimmed.startsWith('//')) return trimmed
  if (URL_SCHEME_RE.test(trimmed) && !/^file:/i.test(trimmed)) return trimmed
  if (!folderPath) return null
  const assets = platform.assets
  const nativeAssets = 'url' in assets
  let homePath: string | undefined
  if (trimmed.startsWith('~/') && allowsHostImagePaths(folderPath, markdownPath, nativeAssets)) {
    try {
      // Keep the backend graph out of pure resolver tests.
      const { backend } = await import('$lib/api/backend')
      homePath = await backend.folders.home()
    } catch {
      return null
    }
  }

  const localPath = resolveMarkdownImagePath(trimmed, folderPath, markdownPath, nativeAssets, homePath)
  if (!localPath) return null
  if (nativeAssets) return assets.url(localPath.startsWith('/') ? '/' : folderPath, localPath)
  return loadLocalImage ? loadLocalImage(localPath) : null
}

function allowsHostImagePaths(
  folderPath: string,
  markdownPath: string | null | undefined,
  nativeAssets: boolean
): boolean {
  return nativeAssets && markdownPath == null && folderPath.startsWith('/') && !splitRemotePath(folderPath)
}

export function resolveMarkdownImagePath(
  href: string,
  folderPath: string | undefined,
  markdownPath: string | null | undefined,
  nativeAssets: boolean,
  homePath?: string
): string | null {
  if (!folderPath || !href) return null
  const hostPaths = allowsHostImagePaths(folderPath, markdownPath, nativeAssets)

  if (/^file:/i.test(href)) {
    return hostPaths ? (/^file:\/\/(?:localhost)?(\/[\s\S]*)$/i.exec(href)?.[1] ?? null) : null
  }
  if (href.startsWith('~/')) {
    return hostPaths && homePath?.startsWith('/') ? homePath.replace(/\/+$/, '') + href.slice(1) : null
  }
  if (hostPaths && href.startsWith('/')) return href
  return resolveMarkdownLocalPath(markdownPath ?? '', href)
}

export function resolveMarkdownLocalPath(markdownPath: string, href: string): string | null {
  const [pathPart] = href.split(/[?#]/, 1)
  if (!pathPart) return null

  const decoded = decodeHrefPath(pathPart).replaceAll('\\', '/')
  const baseParts = decoded.startsWith('/') ? [] : dirname(markdownPath).split('/').filter(Boolean)

  for (const part of decoded.split('/')) {
    if (!part || part === '.') continue
    if (part === '..') {
      if (baseParts.length === 0) return null
      baseParts.pop()
      continue
    }
    baseParts.push(part)
  }

  return baseParts.join('/')
}

function decodeHrefPath(value: string): string {
  try {
    return decodeURIComponent(value)
  } catch {
    return value
  }
}
