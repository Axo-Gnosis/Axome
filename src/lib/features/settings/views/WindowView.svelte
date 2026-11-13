<!--
  @component
  WindowView — startup, window chrome and external-open routing preferences.
-->

<script lang="ts" module>
  import type { WindowSettings } from '$lib/types/backend'
  let saveChain = Promise.resolve()
</script>

<script lang="ts">
  import { backend } from '$lib/api/backend'
  import { Button } from '$lib/components/ui/button'
  import { selectFolder } from '$lib/features/browser/state.svelte'
  import { Select } from '$lib/components/ui/select'
  import { Switch } from '$lib/components/ui/switch'
  import { getWindowControls, setWindowControls } from '$lib/features/app-shell/window-controls/state.svelte'
  import { notify } from '$lib/features/notifications/state.svelte'
  import { getErrorMessage } from '$lib/utils/client-error'
  import {
    getDesktopSettings,
    saveWindowSettings as saveWindowSettingsApi
  } from '$lib/features/settings/state/settings.svelte'
  import SettingRow from '../dialog/SettingRow.svelte'
  import SettingsSection from '../dialog/SettingsSection.svelte'

  let wc = $derived(getWindowControls())
  let desktop = $derived(getDesktopSettings())

  function toggleSystemDecorations(useSystemDecorations: boolean) {
    setWindowControls({ useSystemDecorations })
  }

  function toggleButton(key: 'showMinimize' | 'showMaximize' | 'showClose', value: boolean) {
    setWindowControls({ [key]: value })
  }

  function saveWindowSettings(patch: Partial<WindowSettings>): void {
    saveChain = saveChain
      .then(async () => {
        const current = getDesktopSettings()?.window
        if (!current) throw new Error('Window settings have not loaded')
        await saveWindowSettingsApi({ ...current, ...patch })
      })
      .catch((e) => {
        notify.error('Failed to save window settings', getErrorMessage(e))
      })
  }
</script>

<SettingsSection title="Startup">
  <SettingRow label="Home Folder" as="div">
    {#snippet description()}
      <span class="block">New windows open a New Tab on this folder, and the folder browser starts here.</span>
      <span class="block truncate font-mono">{desktop?.window.homeFolder ?? 'Not set'}</span>
    {/snippet}
    <Button
      size="sm"
      disabled={!desktop}
      onclick={async () => {
        const path = await selectFolder({ path: desktop?.window.homeFolder ?? (await backend.folders.home()) })
        if (path) saveWindowSettings({ homeFolder: path })
      }}>Choose…</Button
    >
    {#if desktop?.window.homeFolder}
      <Button size="sm" onclick={() => saveWindowSettings({ homeFolder: null })}>Clear</Button>
    {/if}
  </SettingRow>
</SettingsSection>

<SettingsSection title="External Open">
  <SettingRow
    label="Open Folders From External Applications"
    description="Choose which window handles folders opened from the OS or command line."
  >
    <Select
      class="w-44 shrink-0"
      value={desktop?.window.externalFolderOpenMode ?? 'newWindow'}
      disabled={!desktop}
      onValueChange={(next) =>
        void saveWindowSettings({
          externalFolderOpenMode: next
        })}
      items={[
        { value: 'newWindow', label: 'New window' },
        { value: 'focusedWindow', label: 'Focused window' }
      ]}
    />
  </SettingRow>

  <SettingRow
    label="Open Files From External Applications"
    description="Prefer a window already showing the file's folder, or choose another destination."
  >
    <Select
      class="w-56 shrink-0"
      value={desktop?.window.externalFileOpenMode ?? 'preferFolder'}
      disabled={!desktop}
      onValueChange={(next) =>
        void saveWindowSettings({
          externalFileOpenMode: next
        })}
      items={[
        { value: 'preferFolder', label: 'Prefer existing folder window' },
        { value: 'focusedWindow', label: 'Focused window' },
        { value: 'newWindow', label: 'New window' }
      ]}
    />
  </SettingRow>
</SettingsSection>

<SettingsSection title="Title Bar">
  <SettingRow label="Use system window decorations" description="Revert to the OS-provided title bar and controls.">
    <Switch checked={wc.useSystemDecorations} disabled={!desktop} onCheckedChange={toggleSystemDecorations} />
  </SettingRow>

  <SettingRow
    label="Active tab bar position"
    description="Position the animated indicator bar above or under the active tab."
  >
    <Select
      class="w-48 shrink-0"
      value={desktop?.window.tabBeamPosition ?? 'top'}
      disabled={!desktop}
      onValueChange={(next) =>
        void saveWindowSettings({
          tabBeamPosition: next
        })}
      items={[
        { value: 'top', label: 'Top (above tab)' },
        { value: 'bottom', label: 'Bottom (under tab)' }
      ]}
    />
  </SettingRow>
</SettingsSection>

{#if !wc.useSystemDecorations}
  <SettingsSection title="Buttons" description="Hide individual custom-chrome buttons.">
    <SettingRow label="Minimize">
      <Switch checked={wc.showMinimize} disabled={!desktop} onCheckedChange={(v) => toggleButton('showMinimize', v)} />
    </SettingRow>
    <SettingRow label="Maximize">
      <Switch checked={wc.showMaximize} disabled={!desktop} onCheckedChange={(v) => toggleButton('showMaximize', v)} />
    </SettingRow>
    <SettingRow label="Close">
      <Switch checked={wc.showClose} disabled={!desktop} onCheckedChange={(v) => toggleButton('showClose', v)} />
    </SettingRow>
  </SettingsSection>
{/if}
