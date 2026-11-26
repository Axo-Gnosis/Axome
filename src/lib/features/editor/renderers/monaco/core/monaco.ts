export type Monaco = typeof import('monaco-editor')

let loading: Promise<Monaco> | null = null

/**
 * The only way to obtain Monaco. Monaco and its setup (`monacoEnv`) load on first call, in their own
 * chunk; nothing here may import them statically, so any module can import this one. Setup applies
 * service overrides, which only take effect before the first Monaco API call: a path that touched
 * Monaco first would silently get the defaults. A failed load is asked again on the next call.
 */
export function loadMonaco(): Promise<Monaco> {
  loading ??= import('./monacoEnv')
    .then((env) => env.setupMonaco())
    .catch((error) => {
      loading = null
      throw error
    })
  return loading
}
