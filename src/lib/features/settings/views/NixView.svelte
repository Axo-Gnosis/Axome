<!--
  @component
  NixView — Nix runtime and editor settings.

  Eval timeout governs how long Axome waits on a flake evaluation
  or build before giving up. Cold stores and first-run flakes can
  take minutes; under-tuned values produce false "timeout" errors.

-->

<script lang="ts">
  import { Input } from '$lib/components/ui/input'
  import LanguageSettingsView from './LanguageSettingsView.svelte'
  import type { SettingsPage } from '$lib/types/backend'
  import { notify } from '$lib/features/notifications/state.svelte'
  import { getSettings, getSettingsHost, saveNixSettings } from '$lib/features/settings/state/settings.svelte'
  import { getErrorMessage } from '$lib/utils/client-error'
  import { createAutoSaver } from './autoSaver.svelte'
  import SettingsSection from '../dialog/SettingsSection.svelte'
  import SettingRow from '../dialog/SettingRow.svelte'

  let { definition }: { definition: SettingsPage } = $props()

  let settings = $derived(getSettings())
  const host = getSettingsHost()
  let timeoutDraft = $state<number | null>(null)
  let timeout = $derived(timeoutDraft ?? settings?.nix.evalTimeoutSecs ?? 600)

  const saver = createAutoSaver()

  async function flush(next: { evalTimeoutSecs: number }) {
    try {
      await saveNixSettings(next, host)
    } catch (error) {
      notify.error('Save nix settings failed', getErrorMessage(error))
    }
  }

  function schedule() {
    const next = { evalTimeoutSecs: Number(timeout) }
    saver.schedule('nix-eval-timeout', () => flush(next))
  }
</script>

<SettingsSection title="Runtime">
  <SettingRow label="Eval timeout" description="Cold stores need 300s+; first flake build may take minutes.">
    <Input
      class="w-24"
      type="number"
      min="30"
      max="3600"
      step="30"
      value={timeout}
      oninput={(event) => {
        timeoutDraft = Number(event.currentTarget.value)
        schedule()
      }}
      disabled={!settings}
    />
    <span class="text-sm text-subtle">seconds</span>
  </SettingRow>
</SettingsSection>

<LanguageSettingsView {definition} />
