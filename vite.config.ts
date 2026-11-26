import { statfsSync } from 'node:fs'
import { resolve, sep } from 'node:path'
import { sveltekit } from '@sveltejs/kit/vite'
import tailwindcss from '@tailwindcss/vite'
import { defineConfig } from 'vite'

// Tauri expects a fixed port during dev
const host = process.env.TAURI_DEV_HOST
const web = process.env.AXOME_TARGET === 'web'
const rootDir = process.cwd()
const uiWatchRoots = [resolve(rootDir, 'src')]

function isUiWatchPath(path: string): boolean {
  const absPath = resolve(path)
  if (absPath === rootDir) return true
  return uiWatchRoots.some((watchRoot) => {
    const isWatchRoot = absPath === watchRoot
    const isInsideWatchRoot = absPath.startsWith(`${watchRoot}${sep}`)
    const isAncestorOfWatchRoot = watchRoot.startsWith(`${absPath}${sep}`)
    return isWatchRoot || isInsideWatchRoot || isAncestorOfWatchRoot
  })
}
function isNetworkFs(dir = rootDir): boolean {
  try {
    const type = statfsSync(dir).type >>> 0
    // 0x6969: NFS, 0x517b: SMB, 0xfe534d42/0xff534d42: SMB2/CIFS, 0x65735546: FUSE, 0x01021997: 9P
    return [0x6969, 0x517b, 0xfe534d42 >>> 0, 0xff534d42 >>> 0, 0x65735546, 0x01021997].includes(type)
  } catch {
    return false
  }
}

const envPolling = process.env.CHOKIDAR_USEPOLLING ?? process.env.VITE_USE_POLLING
const usePolling = envPolling !== undefined ? envPolling === 'true' || envPolling === '1' : isNetworkFs()
const pollInterval = Number(process.env.CHOKIDAR_INTERVAL || 150)

export default defineConfig({
  plugins: [tailwindcss(), sveltekit()],

  // Prevent vite from obscuring Rust errors
  clearScreen: false,

  // Desktop and web dev servers run side by side; never share a dep cache.
  cacheDir: web ? 'node_modules/.vite-web' : undefined,

  // Pre-bundle Monaco worker entry points so dev doesn't re-scan them on each load.
  optimizeDeps: {
    // Discovery is off; every dependency that needs dev pre-bundling is listed explicitly.
    noDiscovery: true,
    include: [
      // Root index re-exports ~6k per-icon modules; pre-bundle it so dev loads one module, not thousands.
      '@hugeicons/core-free-icons',
      // Markdown parsing uses CommonJS dependencies; discovery is disabled above.
      'unified',
      'remark-parse',
      'monaco-editor/esm/vs/editor/editor.worker',
      'monaco-editor/esm/vs/language/typescript/ts.worker',
      'monaco-editor/esm/vs/language/json/json.worker',
      'monaco-editor/esm/vs/language/css/css.worker',
      'monaco-editor/esm/vs/language/html/html.worker'
    ]
  },

  server: {
    port: web ? 1430 : 1420,
    strictPort: true,
    host: host || '127.0.0.1',
    hmr: host
      ? {
          protocol: 'ws',
          host,
          port: 1421
        }
      : undefined,
    watch: {
      usePolling,
      interval: pollInterval,
      // Dev webview should reload only for files that can change UI content.
      ignored: (path) => !isUiWatchPath(path)
    }
  }
})
