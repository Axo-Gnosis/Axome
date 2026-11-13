<script lang="ts">
  import { Button } from '$lib/components/ui/button'
  import { Input, Textarea } from '$lib/components/ui/input'
  import { Select } from '$lib/components/ui/select'
  import {
    ModelPicker,
    EffortPicker,
    splitSelector,
    joinSelector,
    levelsForSelector
  } from '$lib/features/chat/elements/model-picker'
  import { axoModels, settingsRoles } from '$lib/features/settings/state/axo.svelte'
  import SettingRow from '../dialog/SettingRow.svelte'
  import SettingsSection from '../dialog/SettingsSection.svelte'
  import AxoNumberRow from './AxoNumberRow.svelte'
  import AxoToggleRow from './AxoToggleRow.svelte'
  import AxoChoiceRow from './AxoChoiceRow.svelte'
  import { createAxoFields } from './axoFields.svelte'

  const fields = createAxoFields()
  let settings = $derived(fields.settings)
  const available = axoModels(fields.host)
  let roles = $derived(settingsRoles(settings?.modelRoles))
  let agentName = $state('')
  let agentSelector = $state('')

  function addModel() {
    const name = agentName.trim()
    const selector = agentSelector.trim()
    if (!name || !selector || Object.hasOwn(settings?.subagents.models ?? {}, name)) return
    fields.save(['subagents', 'models', name], selector)
    agentName = ''
    agentSelector = ''
  }
</script>

{#snippet text(path: string[], label: string, value: string | null, placeholder: string)}
  <SettingRow {label} stacked>
    <Input
      value={fields.value(path, value)}
      {placeholder}
      disabled={!settings}
      oninput={(event) => fields.save(path, event.currentTarget.value.trim() || null, event.currentTarget.value)}
    />
  </SettingRow>
{/snippet}

<!-- Selectors here may carry an effort suffix (`provider/model:high`); the pair edits both parts. -->
{#snippet selectorPicker(path: string[], saved: string, label: string, none?: string)}
  {@const current = String(fields.value(path, saved))}
  {@const parts = splitSelector(current)}
  <div class="flex min-w-0 flex-1 items-center gap-2">
    <ModelPicker
      {label}
      class="min-w-0 flex-1"
      value={parts.model}
      models={available.models}
      {roles}
      {none}
      custom
      disabled={!settings}
      onSelect={(next) => {
        const picked = splitSelector(next)
        const joined = joinSelector(picked.model, picked.effort || parts.effort)
        fields.save(path, joined || null, joined)
      }}
    />
    <EffortPicker
      label={`Effort for ${label}`}
      value={parts.effort}
      levels={levelsForSelector(parts.model, available.models, roles)}
      defaultLabel="Default"
      disabled={!settings || !parts.model || parts.model.startsWith('@')}
      onSelect={(level) => {
        const joined = joinSelector(parts.model, level)
        fields.save(path, joined || null, joined)
      }}
    />
  </div>
{/snippet}

<SettingsSection title="Bash">
  <AxoChoiceRow
    {fields}
    path={['bash', 'devShell']}
    label={'Development shell'}
    value={settings?.bash.devShell ?? 'off'}
    choices={['off', 'direnv', 'nix']}
  />
  {@render text(['bash', 'devShellRef'], 'Shell reference', settings?.bash.devShellRef ?? null, '.#default')}
  <SettingRow label="Watch files (one per line)" stacked>
    <Textarea
      value={String(fields.value(['bash', 'devShellWatch'], settings?.bash.devShellWatch.join('\n') ?? ''))}
      rows={3}
      disabled={!settings}
      oninput={(event) =>
        fields.save(
          ['bash', 'devShellWatch'],
          event.currentTarget.value
            .split('\n')
            .map((line) => line.trim())
            .filter(Boolean),
          event.currentTarget.value
        )}
    />
  </SettingRow>
  <AxoNumberRow
    {fields}
    path={['bash', 'devShellTimeoutMs']}
    label={'Shell timeout (ms)'}
    value={settings?.bash.devShellTimeoutMs ?? null}
  />
  <AxoToggleRow {fields} path={['bash', 'ci']} label={'CI environment'} checked={settings?.bash.ci ?? false} />
  <AxoNumberRow
    {fields}
    path={['bash', 'promoteAfterMs']}
    label={'Background after (ms)'}
    value={settings?.bash.promoteAfterMs ?? null}
  />
</SettingsSection>

<SettingsSection title="Subagents">
  <AxoNumberRow
    {fields}
    path={['subagents', 'maxDepth']}
    label={'Maximum depth'}
    value={settings?.subagents.maxDepth ?? null}
  />
  <AxoNumberRow
    {fields}
    path={['subagents', 'maxConcurrent']}
    label={'Maximum concurrent'}
    value={settings?.subagents.maxConcurrent ?? null}
  />
  <AxoNumberRow
    {fields}
    path={['subagents', 'maxTurns']}
    label={'Maximum turns'}
    value={settings?.subagents.maxTurns ?? null}
  />
  <SettingRow label="Default model" stacked as="div">
    {@render selectorPicker(
      ['subagents', 'defaultModel'],
      settings?.subagents.defaultModel ?? '',
      'Subagent default model',
      'Not set (uses @default)'
    )}
  </SettingRow>
  <SettingRow
    label="Context files"
    description={settings && typeof settings.subagents.contextFiles !== 'boolean'
      ? 'A file-specific map is configured. Choosing Off or On replaces it.'
      : undefined}
  >
    <Select
      value={settings?.subagents.contextFiles === false ? 'off' : 'on'}
      disabled={!settings}
      onValueChange={(next) => fields.save(['subagents', 'contextFiles'], next === 'on')}
      items={[
        { value: 'off', label: 'Off' },
        { value: 'on', label: 'On' }
      ]}
    />
  </SettingRow>
</SettingsSection>

<SettingsSection title="Agent Models">
  <div class="divide-y divide-edge">
    {#each Object.entries(settings?.subagents.models ?? {}) as [name, selector] (name)}
      <SettingRow label={name} stacked as="div" class="py-4 first:pt-0 last:pb-0">
        <div class="flex items-center gap-2">
          {@render selectorPicker(['subagents', 'models', name], selector, `Model for ${name}`)}
          <Button size="sm" variant="ghost" onclick={() => fields.save(['subagents', 'models', name], null, '')}
            >Remove</Button
          >
        </div>
      </SettingRow>
    {/each}
  </div>
  <SettingRow label="New agent model" stacked as="div">
    {@const draftParts = splitSelector(agentSelector)}
    <div class="flex flex-wrap items-center gap-2">
      <Input class="min-w-0 flex-1" aria-label="Agent name" placeholder="Agent name" bind:value={agentName} />
      <ModelPicker
        label="Agent model"
        class="min-w-0 flex-1"
        value={draftParts.model}
        models={available.models}
        {roles}
        custom
        onSelect={(next) => {
          const picked = splitSelector(next)
          agentSelector = joinSelector(picked.model, picked.effort || draftParts.effort)
        }}
      />
      <EffortPicker
        label="Agent effort"
        value={draftParts.effort}
        levels={levelsForSelector(draftParts.model, available.models, roles)}
        defaultLabel="Default"
        disabled={!draftParts.model || draftParts.model.startsWith('@')}
        onSelect={(level) => (agentSelector = joinSelector(draftParts.model, level))}
      />
      <Button
        size="sm"
        disabled={!settings ||
          !agentName.trim() ||
          !agentSelector.trim() ||
          Object.hasOwn(settings.subagents.models, agentName.trim())}
        onclick={addModel}>Add Model</Button
      >
    </div>
  </SettingRow>
</SettingsSection>
