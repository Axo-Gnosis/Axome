import { notify } from '$lib/features/notifications/state.svelte'
import { saveAxo } from '$lib/features/settings/state/axo.svelte'
import { getErrorMessage } from '$lib/utils/client-error'
import { createAutoSaver } from './autoSaver.svelte'
import { getSettings, getSettingsHost } from '../state/settings.svelte'

/** Keep field drafts, null-on-empty values, and debounce keys consistent across axo pages. */
export function createAxoFields() {
  const host = getSettingsHost()
  const draft = $state<Record<string, string>>({})
  const saver = createAutoSaver()
  function save(path: string[], value: unknown, text?: string) {
    const key = JSON.stringify(path)
    if (text !== undefined) draft[key] = text
    const patch = path.reduceRight<Record<string, unknown>>(
      (nested, part, index) => ({ [part]: index === path.length - 1 ? value : nested }),
      {}
    )
    saver.schedule(key, async () => {
      try {
        await saveAxo(patch, host)
        if (text !== undefined && draft[key] === text) delete draft[key]
      } catch (error) {
        notify.error('Save agent settings failed', getErrorMessage(error))
      }
    })
  }

  function number(path: string[], input: HTMLInputElement) {
    const text = input.value
    draft[JSON.stringify(path)] = text
    if (!input.validity.valid) return
    save(path, text === '' ? null : Number(text), text)
  }

  return {
    host,
    get settings() {
      return getSettings()?.axo
    },
    value(path: string[], saved: string | number | null): string | number {
      return draft[JSON.stringify(path)] ?? saved ?? ''
    },
    save,
    number
  }
}

export type AxoFields = ReturnType<typeof createAxoFields>
