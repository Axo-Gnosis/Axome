<!--
  @component
  PredictView — local ghost-text predictive text settings for the prompt composer.
  Always applies to this desktop: typed text and learned state never leave the machine.
-->

<script lang="ts">
  import { onMount } from 'svelte'
  import { backend } from '$lib/api/backend'
  import { Badge } from '$lib/components/ui/badge'
  import { Button } from '$lib/components/ui/button'
  import { Select } from '$lib/components/ui/select'
  import { Switch } from '$lib/components/ui/switch'
  import { notify } from '$lib/features/notifications/state.svelte'
  import { getDesktopSettings, loadSettings, savePredictSettings } from '../state/settings.svelte'
  import type { PredictSettings, PredictStatus } from '$lib/types/backend'
  import { getErrorMessage } from '$lib/utils/client-error'
  import SettingRow from '../dialog/SettingRow.svelte'
  import SettingsSection from '../dialog/SettingsSection.svelte'

  let settings = $derived(getDesktopSettings()?.predict)
  let status = $state<PredictStatus | null>(null)
  let busy = $state(false)
  type NextWordsOption = '0' | '1' | '2'
  let nextWordsValue = $derived<NextWordsOption>(
    settings?.nextWords === 0 ? '0' : settings?.nextWords === 2 ? '2' : '1'
  )

  type ThresholdPreset = '0.15' | '0.25' | '0.40' | '0.55'
  const THRESHOLD_PRESETS: { value: ThresholdPreset; label: string; threshold: number }[] = [
    { value: '0.15', label: 'Aggressive (0.15)', threshold: 0.15 },
    { value: '0.25', label: 'Balanced (0.25)', threshold: 0.25 },
    { value: '0.40', label: 'Selective (0.40)', threshold: 0.4 },
    { value: '0.55', label: 'Conservative (0.55)', threshold: 0.55 }
  ]
  let thresholdValue = $derived.by<ThresholdPreset>(() => {
    const value = settings?.nextWordThreshold ?? 0.25
    let closest = THRESHOLD_PRESETS[1]
    for (const preset of THRESHOLD_PRESETS) {
      if (Math.abs(preset.threshold - value) < Math.abs(closest.threshold - value)) closest = preset
    }
    return closest.value
  })

  let modelBadge = $derived.by(() => {
    const model = status?.model
    switch (model?.state) {
      case 'off':
        return { label: 'Off', tone: 'neutral' as const }
      case 'downloading': {
        const received = (model.received / 1_000_000).toFixed(1)
        const total = (model.total / 1_000_000).toFixed(1)
        const label =
          model.received >= model.total && model.total > 0 ? 'Loading model…' : `Downloading ${received}/${total} MB`
        return { label, tone: 'neutral' as const }
      }
      case 'ready':
        return { label: 'Ready · SmolLM2-135M', tone: 'good' as const }
      case 'failed':
        return { label: 'Failed', tone: 'bad' as const }
      default:
        return { label: 'Not downloaded', tone: 'neutral' as const }
    }
  })

  async function refreshStatus() {
    try {
      status = await backend.predict.status()
    } catch (error) {
      notify.error('Load predictive text status failed', getErrorMessage(error))
    }
  }

  onMount(() => {
    void loadSettings(null).catch((error) => notify.error('Load settings failed', getErrorMessage(error)))
    void refreshStatus()
  })

  $effect(() => {
    if (status?.model.state !== 'downloading') return
    // ponytail: poll only this open page while downloading/loading; add a host event if another surface needs it.
    const timer = setTimeout(() => void refreshStatus(), 1500)
    return () => clearTimeout(timer)
  })

  async function runAction(action: () => Promise<void>) {
    if (busy) return
    busy = true
    try {
      await action()
      await refreshStatus()
    } catch (error) {
      notify.error('Predictive text update failed', getErrorMessage(error))
    } finally {
      busy = false
    }
  }

  function update(patch: Partial<PredictSettings>) {
    if (!settings) return
    const next: PredictSettings = { ...settings, ...patch }
    void runAction(async () => {
      await savePredictSettings(next)
    })
  }

  function resetLearned() {
    void runAction(async () => {
      await backend.predict.reset()
      notify.info('Predictive text reset', 'Cleared learned vocabulary and prompt memory.')
    })
  }
</script>

<SettingsSection title="Predictive Text">
  <SettingRow
    label="Enable Predictive Text"
    description="Shows inline ghost-text word completions while typing prompts. Learns from prompts you submit; everything stays on this machine."
    as="div"
  >
    <Switch
      aria-label="Enable Predictive Text"
      checked={settings?.enabled ?? true}
      disabled={!settings || busy}
      onCheckedChange={(enabled) => update({ enabled })}
    />
  </SettingRow>
</SettingsSection>

<div class:opacity-50={!settings?.enabled}>
  <SettingsSection title="Language Model">
    <SettingRow label="Use Local Language Model" as="div">
      {#snippet description()}
        <span class="block">
          Adds SmolLM2-135M (145 MB download, runs on CPU) for stronger word completions and next-word suggestions.
        </span>
        <span class="mt-1 flex items-center gap-2">
          <Badge variant={modelBadge.tone === 'bad' ? 'danger' : modelBadge.tone === 'good' ? 'success' : 'default'}>
            {modelBadge.label}
          </Badge>
          {#if status?.model.state === 'failed'}
            <span class="truncate text-xs text-danger">{status.model.error}</span>
          {/if}
        </span>
      {/snippet}
      <div class="flex items-center gap-2">
        {#if status?.model.state === 'failed'}
          <Button size="sm" variant="outline" disabled={busy} onclick={resetLearned}>Retry</Button>
        {/if}
        <Switch
          aria-label="Use Local Language Model"
          checked={settings?.model ?? false}
          disabled={!settings?.enabled || busy}
          onCheckedChange={(model) => update({ model })}
        />
      </div>
    </SettingRow>

    <SettingRow
      label="Next-Word Suggestions"
      description="How many words to suggest beyond the current word. Requires the language model."
    >
      <Select
        class="w-44 shrink-0"
        value={nextWordsValue}
        disabled={!settings?.enabled || !settings?.model || busy}
        onValueChange={(value) => update({ nextWords: Number(value) })}
        items={[
          { value: '0', label: 'Current word only' },
          { value: '1', label: '+1 word' },
          { value: '2', label: '+2 words' }
        ]}
      />
    </SettingRow>

    <SettingRow
      label="Next-Word Sensitivity"
      description="Minimum confidence required before showing a next-word ghost. Lower values suggest more often."
    >
      <Select
        class="w-48 shrink-0"
        value={thresholdValue}
        disabled={!settings?.enabled || !settings?.model || (settings?.nextWords ?? 1) === 0 || busy}
        onValueChange={(value) => update({ nextWordThreshold: Number(value) })}
        items={THRESHOLD_PRESETS}
      />
    </SettingRow>
  </SettingsSection>

  <SettingsSection title="Learned Data">
    <SettingRow
      label="Reset Learned Data"
      description="Clears your personal word counts and recent-prompt memory stored under this desktop's data directory."
      as="div"
    >
      <Button size="sm" variant="outline" disabled={busy} onclick={resetLearned}>Reset</Button>
    </SettingRow>
  </SettingsSection>
</div>
