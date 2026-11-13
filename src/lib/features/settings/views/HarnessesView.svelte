<!--
  @component
  HarnessesView — coding-agent harness roster.

  Flat, IDE-style row list. Each row: switch + name/status/version
  + resolved path + chevron toggle. Expanding reveals inline rows
  for binary-path override and extra-args.
  Writes debounce through a shared autoSaver.

-->

<script lang="ts">
  import { Input } from '$lib/components/ui/input'
  import BinaryRow from './BinaryRow.svelte'
  import { notify } from '$lib/features/notifications/state.svelte'
  import { getSettings, getSettingsHost, saveHarnessConfig } from '$lib/features/settings/state/settings.svelte'
  import { getErrorMessage } from '$lib/utils/client-error'
  import type { HarnessConfig, SettingsPayload } from '$lib/types/backend'
  import { createAutoSaver } from './autoSaver.svelte'
  import SettingsSection from '../dialog/SettingsSection.svelte'
  import SettingRow from '../dialog/SettingRow.svelte'

  let settings = $derived(getSettings())
  const host = getSettingsHost()

  type Draft = { enabled: boolean; binaryPath: string; extraArgs: string }
  let drafts = $state<Record<string, Draft>>({})

  const valueOf = (entry: SettingsPayload['harnesses'][number]): Draft =>
    drafts[entry.harness.id] ?? {
      enabled: entry.config.enabled,
      binaryPath: entry.config.binaryPathOverride ?? '',
      extraArgs: entry.config.extraArgs.join(' ')
    }

  const saver = createAutoSaver()

  async function flush(next: HarnessConfig) {
    try {
      await saveHarnessConfig(next, host)
    } catch (error) {
      notify.error('Save harness failed', getErrorMessage(error))
    }
  }

  function update(id: string, key: keyof Draft, value: boolean | string) {
    const entry = settings?.harnesses.find((entry) => entry.harness.id === id)
    if (!entry) return
    const draft = { ...valueOf(entry), [key]: value }
    drafts = { ...drafts, [id]: draft }
    const next: HarnessConfig = {
      harnessId: id,
      enabled: draft.enabled,
      binaryPathOverride: draft.binaryPath.trim() || null,
      extraArgs: draft.extraArgs.split(/\s+/).filter(Boolean)
    }
    saver.schedule(`harness:${id}`, () => flush(next))
  }
</script>

<SettingsSection title="Harnesses">
  {#snippet description()}
    Enabled harnesses appear in New Session. Add presets under
    <code class="font-mono">harnesses.&lt;id&gt;.presets</code> in settings.jsonc.
  {/snippet}

  {#if settings}
    <div class="flex flex-col divide-y divide-edge">
      {#each settings.harnesses as entry (entry.harness.id)}
        {@const id = entry.harness.id}
        {@const d = valueOf(entry)}

        <BinaryRow
          label={entry.harness.label}
          status={entry.harness.status}
          path={entry.harness.resolved_path ?? entry.harness.install_hint}
          message={entry.harness.message}
          enabled={d.enabled}
          onEnabledChange={(value) => update(id, 'enabled', value)}
        >
          {#snippet meta()}{#if entry.harness.version}<span class="font-mono text-xs text-subtle"
                >{entry.harness.version}</span
              >{/if}{/snippet}
          <SettingRow label="Binary path" stacked>
            <Input
              value={d.binaryPath}
              oninput={(e: Event) => update(id, 'binaryPath', (e.currentTarget as HTMLInputElement).value)}
              placeholder="Use detected CLI"
            />
          </SettingRow>

          <SettingRow label="Extra args" stacked>
            <Input
              value={d.extraArgs}
              oninput={(e: Event) => update(id, 'extraArgs', (e.currentTarget as HTMLInputElement).value)}
              placeholder="--flag value"
            />
          </SettingRow>
        </BinaryRow>
      {/each}
    </div>
  {:else}
    <div class="py-8 text-sm text-muted">Loading harnesses&hellip;</div>
  {/if}
</SettingsSection>
