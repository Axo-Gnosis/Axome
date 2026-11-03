import { platform } from '$lib/platform'
import { dirname } from '$lib/utils/paths'

export const URL_SCHEME_RE = /^[a-zA-Z][a-zA-Z0-9+.-]*:/

// Markdown images are document-relative, or project-relative in chat (no document path).
// Browser-relative URLs point at Vite/Tauri chrome, not the repo.
// Stream-backed platforms resolve project images only through the view-owned loader, never an HTTP-relative fetch.
export async function markdownImageSrc(
  href: string | null | undefined,
  folderPath?: string,
  markdownPath?: string | null,
  loadLocalImage?: (filePath: string) => Promise<string | null>
): Promise<string | null> {
  if (!href) return ''
  const trimmed = href.trim()
  if (!trimmed || trimmed.startsWith('//')) return trimmed
  const assets = platform.assets
  if (URL_SCHEME_RE.test(trimmed)) {
    if (/^file:/i.test(trimmed) && !('url' in assets)) return null
    return trimmed
  }
  if (!folderPath) return null
  const localPath = resolveMarkdownLocalPath(markdownPath ?? '', trimmed)
  if (!localPath) return null
  if ('url' in assets) return assets.url(folderPath, localPath)
  return loadLocalImage ? loadLocalImage(localPath) : null
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
