<script lang="ts">
  import { Button } from '$lib/components/ui/button'
  import { Input } from '$lib/components/ui/input'
  import { Switch } from '$lib/components/ui/switch'
  import { backend } from '$lib/api/backend'
  import type { AxoKeywordInfo } from '$lib/types/backend'
  import { getErrorMessage } from '$lib/utils/client-error'
  import { onMount } from 'svelte'
  import SettingRow from '../dialog/SettingRow.svelte'
  import SettingsSection from '../dialog/SettingsSection.svelte'
  import AxoNumberRow from './AxoNumberRow.svelte'
  import AxoToggleRow from './AxoToggleRow.svelte'
  import AxoChoiceRow from './AxoChoiceRow.svelte'
  import { createAxoFields } from './axoFields.svelte'

  const fields = createAxoFields()
  let settings = $derived(fields.settings)
  let keywords = $state<AxoKeywordInfo[]>([])
  let keywordError = $state('')
  let disabledDraft = $state<string[] | null>(null)
  let disabledKeywords = $derived(disabledDraft ?? settings?.keywords.disabled ?? [])
  let fileName = $state('')
  onMount(() => {
    let active = true
    void backend.axo
      .keywords(fields.host)
      .then((result) => {
        if (active) keywords = result
      })
      .catch((error) => {
        if (active) keywordError = getErrorMessage(error)
      })
    return () => {
      active = false
    }
  })

  function keyword(name: string, enabled: boolean) {
    disabledDraft = enabled
      ? disabledKeywords.filter((item) => item !== name)
      : [...new Set([...disabledKeywords, name])]
    fields.save(['keywords', 'disabled'], [...disabledDraft])
  }

  function addFile() {
    const name = fileName.trim()
    if (!name || Object.hasOwn(settings?.contextFiles ?? {}, name)) return
    fields.save(['contextFiles', name], true)
    fileName = ''
  }
</script>

<SettingsSection title="Queue">
  <AxoChoiceRow
    {fields}
    path={['steeringMode']}
    label={'Steering mode'}
    value={settings?.steeringMode ?? 'one-at-a-time'}
    choices={['all', 'one-at-a-time']}
  />
  <AxoChoiceRow
    {fields}
    path={['followUpMode']}
    label={'Follow-up mode'}
    value={settings?.followUpMode ?? 'one-at-a-time'}
    choices={['all', 'one-at-a-time']}
  />
  <AxoNumberRow
    {fields}
    path={['ask', 'timeoutMs']}
    label={'Question timeout (ms)'}
    value={settings?.ask.timeoutMs ?? null}
  />
</SettingsSection>

<SettingsSection title="Context">
  <SettingRow label="Context files" stacked as="div">
    <div class="divide-y divide-edge">
      {#each Object.entries(settings?.contextFiles ?? {}) as [name, enabled] (name)}
        <div class="flex items-center gap-3 py-3 first:pt-0">
          <span class="min-w-0 flex-1 font-mono text-sm break-all text-fg">{name}</span>
          <Switch
            aria-label={`Load ${name}`}
            checked={enabled}
            onCheckedChange={(value) => fields.save(['contextFiles', name], value)}
          />
          <Button size="sm" variant="ghost" onclick={() => fields.save(['contextFiles', name], null)}>Remove</Button>
        </div>
      {/each}
    </div>
    <div class="flex items-center gap-2">
      <Input aria-label="Context file name" placeholder="AGENTS.md" bind:value={fileName} />
      <Button
        size="sm"
        disabled={!settings || !fileName.trim() || Object.hasOwn(settings.contextFiles, fileName.trim())}
        onclick={addFile}>Add File</Button
      >
    </div>
  </SettingRow>
  <AxoToggleRow
    {fields}
    path={['nestedContextFiles']}
    label={'Nested context files'}
    checked={settings?.nestedContextFiles ?? false}
  />
  <AxoToggleRow
    {fields}
    path={['homeAgentsDir']}
    label={'Home agents directory'}
    checked={settings?.homeAgentsDir ?? false}
  />
  <AxoToggleRow
    {fields}
    path={['extendedContext']}
    label={'Extended context'}
    checked={settings?.extendedContext ?? false}
  />
</SettingsSection>

<SettingsSection title="Keywords">
  <AxoToggleRow
    {fields}
    path={['keywords', 'enabled']}
    label={'Enable keywords'}
    checked={settings?.keywords.enabled ?? false}
  />
  {#if keywordError}<p class="text-sm text-danger" role="alert">{keywordError}</p>{/if}
  <div class="divide-y divide-edge">
    {#each keywords as entry (entry.name)}
      <SettingRow label={entry.name} description={entry.description} as="div" class="py-3 first:pt-0 last:pb-0">
        <Switch
          aria-label={`Enable keyword ${entry.name}`}
          checked={!disabledKeywords.includes(entry.name)}
          disabled={!settings}
          onCheckedChange={(enabled) => keyword(entry.name, enabled)}
        />
      </SettingRow>
    {/each}
  </div>
</SettingsSection>

<SettingsSection title="Compaction">
  <AxoToggleRow
    {fields}
    path={['compaction', 'enabled']}
    label={'Enable compaction'}
    checked={settings?.compaction.enabled ?? false}
  />
  <AxoNumberRow
    {fields}
    path={['compaction', 'maxContext']}
    label={'Maximum context'}
    value={settings?.compaction.maxContext ?? null}
  />
  <AxoNumberRow
    {fields}
    path={['compaction', 'reserveTokens']}
    label={'Reserve tokens'}
    value={settings?.compaction.reserveTokens ?? null}
  />
  <AxoNumberRow
    {fields}
    path={['compaction', 'keepRecentTokens']}
    label={'Keep recent tokens'}
    value={settings?.compaction.keepRecentTokens ?? null}
  />
  <AxoChoiceRow
    {fields}
    path={['compaction', 'summaryRequest']}
    label={'Summary request'}
    value={settings?.compaction.summaryRequest ?? 'live'}
    choices={['live', 'separate']}
  />
</SettingsSection>

<SettingsSection title="Retry">
  <AxoNumberRow
    {fields}
    path={['retry', 'maxRetries']}
    label={'Maximum retries'}
    value={settings?.retry.maxRetries ?? null}
  />
  <AxoNumberRow
    {fields}
    path={['retry', 'baseDelayMs']}
    label={'Base delay (ms)'}
    value={settings?.retry.baseDelayMs ?? null}
  />
  <AxoNumberRow
    {fields}
    path={['retry', 'maxDelayMs']}
    label={'Maximum delay (ms)'}
    value={settings?.retry.maxDelayMs ?? null}
  />
</SettingsSection>
