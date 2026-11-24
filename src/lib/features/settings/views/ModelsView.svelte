<script lang="ts">
  import { Button } from '$lib/components/ui/button'
  import { Input } from '$lib/components/ui/input'
  import { Chip } from '$lib/components/ui/chip'
  import { Select } from '$lib/components/ui/select'
  import { EffortPicker, SelectorPicker, levelsForSelector, ROLE_NAME } from '$lib/features/chat/elements/model-picker'
  import { BUILTIN_ROLES, axoModels, settingsRoles } from '$lib/features/settings/state/axo.svelte'
  import SettingRow from '../dialog/SettingRow.svelte'
  import SettingsSection from '../dialog/SettingsSection.svelte'
  import { createAxoFields } from './axoFields.svelte'

  const fields = createAxoFields()
  const fixed: readonly string[] = BUILTIN_ROLES
  // What each built-in role is used for (design.md D11).
  const roleDescriptions: Record<string, string> = {
    default: 'Primary model; fallback for every role',
    reason: 'Planning, research and reasoning',
    work: 'Delegated task subagents',
    dash: 'Fast exploration',
    conduct: 'Orchestrating subagents',
    memory: 'Background memory capture. Runs after sessions; a local model keeps it free.'
  }
  let settings = $derived(fields.settings)
  const defaultModel = $derived(String(fields.value(['defaultModel'], settings?.defaultModel ?? '')))
  // Settings are host-scoped and must load models even with no folder open.
  const available = axoModels(fields.host)
  let models = $derived(available.models)
  let custom = $state<string[]>([])
  let newName = $state('')
  let roles = $derived(settingsRoles(settings?.modelRoles, custom))
  let names = $derived(roles.map((role) => role.name))
  let validName = $derived(ROLE_NAME.test(newName) && !names.includes(newName))

  function selection(name: string) {
    return String(fields.value(['modelRoles', name], settings?.modelRoles[name] ?? ''))
  }

  function update(name: string, selector: string) {
    fields.save(['modelRoles', name], selector || null, selector)
  }

  function add() {
    if (!validName) return
    custom = [...custom, newName]
    newName = ''
  }

  function remove(name: string) {
    custom = custom.filter((entry) => entry !== name)
    update(name, '')
  }
</script>

<SettingsSection title="Defaults">
  <SettingRow
    label="Default model"
    description="Model, effort and tier for new chats. Unset uses @default."
    stacked
    as="div"
  >
    <SelectorPicker
      modelLabel="Default model"
      value={defaultModel}
      {models}
      {roles}
      none="Not set (uses @default)"
      disabled={!settings}
      onSelect={(next) => fields.save(['defaultModel'], next || null, next)}
    />
  </SettingRow>
  <SettingRow label="Default effort" description="Used when the default model selector has no effort suffix." as="div">
    <EffortPicker
      label="Default effort"
      value={settings?.defaultThinkingLevel ?? 'medium'}
      levels={levelsForSelector(defaultModel, models, roles)}
      disabled={!settings}
      onSelect={(level) => fields.save(['defaultThinkingLevel'], level)}
    />
  </SettingRow>
  <SettingRow
    label="Tier after switching model"
    description="Whether Flex, Fast or Ultrafast survives a model change in a chat."
  >
    <Select
      aria-label="Tier after switching model"
      value={settings?.serviceTier.onModelChange ?? 'reset'}
      disabled={!settings}
      items={[
        { value: 'reset', label: 'Reset to Standard' },
        { value: 'keep', label: 'Keep selected tier' }
      ]}
      onValueChange={(next) => fields.save(['serviceTier', 'onModelChange'], next)}
    />
  </SettingRow>
</SettingsSection>

<SettingsSection
  title="Roles"
  description="Unset roles inherit @default; an unset @default uses the last model picked in a chat."
>
  {#if available.loading}<p class="text-sm text-muted">Loading models…</p>{/if}
  {#if available.error}<p class="text-sm text-danger" role="alert">{available.error}</p>{/if}
  <div class="flex flex-col gap-2">
    {#each names as name (name)}
      {@const selected = selection(name)}
      {@const inherits = name === 'default' ? 'Last model picked' : 'Inherits @default'}
      <div
        class="grid grid-cols-[minmax(9rem,1fr)_minmax(0,2.4fr)_4.5rem] items-center gap-3 rounded-lg border bg-surface px-3 py-2.5
               {selected ? 'border-accent/40' : 'border-edge'}"
      >
        <div class="flex min-w-0 flex-col gap-1">
          <Chip tone={selected ? 'accent' : 'default'} class="self-start font-mono">@{name}</Chip>
          <span class="text-sm text-muted">{roleDescriptions[name] ?? 'Custom role'}</span>
        </div>

        <SelectorPicker
          value={selected}
          {models}
          none={inherits}
          modelLabel={`Model for @${name}`}
          disabled={!settings || available.loading}
          onSelect={(joined) => update(name, joined)}
        />
        <div class="flex justify-end">
          {#if !fixed.includes(name)}
            <Button size="sm" variant="ghost" disabled={!settings} onclick={() => remove(name)}>Remove</Button>
          {:else if selected}
            <Button size="sm" variant="ghost" disabled={!settings} onclick={() => update(name, '')}>Clear</Button>
          {:else}
            <span class="px-2.5 text-subtle" aria-label="Nothing to reset">—</span>
          {/if}
        </div>
      </div>
    {/each}
  </div>
  <SettingRow
    label="Custom role"
    description={newName && !validName
      ? 'Use a unique name starting with a lowercase letter, followed by lowercase letters, digits or hyphens.'
      : undefined}
    stacked
    as="div"
  >
    <div class="flex items-center gap-2">
      <Input aria-label="Custom role name" placeholder="custom-role" bind:value={newName} />
      <Button size="sm" disabled={!settings || !validName} onclick={add}>Add Role</Button>
    </div>
  </SettingRow>
</SettingsSection>
