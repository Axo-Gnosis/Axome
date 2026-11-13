<script lang="ts">
  import { Badge } from '$lib/components/ui/badge'
  import { Input } from '$lib/components/ui/input'
  import { notify } from '$lib/features/notifications/state.svelte'
  import { getSettings, getSettingsHost, saveShellConfig } from '$lib/features/settings/state/settings.svelte'
  import type { ShellConfig } from '$lib/types/backend'
  import { getErrorMessage } from '$lib/utils/client-error'
  import { createAutoSaver } from './autoSaver.svelte'
  import SettingsSection from '../dialog/SettingsSection.svelte'
  import SettingRow from '../dialog/SettingRow.svelte'

  let settings = $derived(getSettings())
  const host = getSettingsHost()
  type Draft = { binaryPath: string; extraArgs: string }
  // Keep edits local so a backend refresh cannot replace pending input.
  let draft = $state<Draft | null>(null)
  let values = $derived(
    draft ?? {
      binaryPath: settings?.shell.binaryPathOverride ?? '',
      extraArgs: settings?.shell.extraArgs.join(' ') ?? ''
    }
  )

  const saver = createAutoSaver()

  function update(key: keyof Draft, value: string) {
    draft = { ...values, [key]: value }
    const next: ShellConfig = {
      binaryPathOverride: draft.binaryPath.trim() || null,
      extraArgs: draft.extraArgs.split(/\s+/).filter(Boolean)
    }
    saver.schedule('shell', async () => {
      try {
        await saveShellConfig(next, host)
      } catch (error) {
        notify.error('Save shell failed', getErrorMessage(error))
      }
    })
  }
</script>

<SettingsSection title="Shell">
  <SettingRow label="Shell path" stacked>
    <Input
      value={values.binaryPath}
      oninput={(e: Event) => update('binaryPath', (e.currentTarget as HTMLInputElement).value)}
      placeholder="Detected login shell"
      disabled={!settings}
    />
  </SettingRow>
  <SettingRow label="Shell args" stacked>
    <Input
      value={values.extraArgs}
      oninput={(e: Event) => update('extraArgs', (e.currentTarget as HTMLInputElement).value)}
      placeholder="--flag value"
      disabled={!settings}
    />
  </SettingRow>
</SettingsSection>

<SettingsSection title="Presets">
  <div class="divide-y divide-edge">
    {#each Object.entries(settings?.shell.presets ?? {}) as [name, preset] (name)}
      <div class="flex flex-col gap-1 py-4 first:pt-0 last:pb-0">
        <div class="flex items-center gap-2">
          <span class="font-mono text-sm text-fg">{name}</span>
          {#if name === settings?.shell.defaultPreset}
            <Badge>Default</Badge>
          {/if}
        </div>
        <div class="font-mono text-xs break-all text-muted">
          {preset.command ?? 'default shell'}{preset.args.length ? ` ${preset.args.join(' ')}` : ''}
        </div>
        {#if preset.startup}
          <div class="font-mono text-xs break-all whitespace-pre-wrap text-muted">{preset.startup}</div>
        {/if}
      </div>
    {/each}
  </div>
  <p class="text-sm text-muted">
    Add presets under <code class="font-mono">shell.presets</code> and pick one with
    <code class="font-mono">shell.defaultPreset</code> in settings.jsonc.
  </p>
</SettingsSection>
