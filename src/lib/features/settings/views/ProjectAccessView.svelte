<script lang="ts">
  import { Button } from '$lib/components/ui/button'
  import { Input } from '$lib/components/ui/input'
  import { Select } from '$lib/components/ui/select'
  import AxoToggleRow from './AxoToggleRow.svelte'
  import AxoChoiceRow from './AxoChoiceRow.svelte'
  import type { AxoProjectAccess } from '$lib/types/backend'
  import SettingRow from '../dialog/SettingRow.svelte'
  import SettingsSection from '../dialog/SettingsSection.svelte'
  import { createAxoFields } from './axoFields.svelte'

  const fields = createAxoFields()
  const access: AxoProjectAccess[] = ['ignore', 'read', 'trust']
  let settings = $derived(fields.settings)
  let glob = $state('')
  let globAccess = $state<AxoProjectAccess>('read')
  let pathsDraft = $state<Record<string, AxoProjectAccess | null> | null>(null)
  let paths = $derived(pathsDraft ?? settings?.project.paths ?? {})

  function setPath(name: string, value: AxoProjectAccess | null) {
    // Keep deletion markers in the coalesced map so a later edit cannot resurrect a removed glob.
    pathsDraft = { ...paths, [name]: value }
    fields.save(['project', 'paths'], { ...pathsDraft })
  }

  function addPath() {
    const name = glob.trim()
    if (!name || paths[name] != null) return
    setPath(name, globAccess)
    glob = ''
  }
</script>

<SettingsSection
  title="Project"
  description="Project access is stored here; opened projects currently run with trust access."
>
  <AxoToggleRow
    {fields}
    path={['project', 'files', 'skills']}
    label="Load project skills"
    checked={settings?.project.files.skills ?? false}
  />
  <AxoToggleRow
    {fields}
    path={['project', 'files', 'config']}
    label="Load project config"
    checked={settings?.project.files.config ?? false}
  />
  <AxoChoiceRow
    {fields}
    path={['project', 'access']}
    label="Default access"
    value={settings?.project.access ?? 'read'}
    choices={access}
  />
</SettingsSection>

<SettingsSection title="Path Access" description="Globs are evaluated in file order. New entries are appended.">
  <div class="divide-y divide-edge">
    {#each Object.entries(paths) as [name, mode] (name)}
      {#if mode !== null}
        <SettingRow label={name} as="div" class="py-3 first:pt-0 last:pb-0">
          <Select
            aria-label={`Access for ${name}`}
            value={mode}
            onValueChange={(next) => setPath(name, next)}
            items={access}
          />
          <Button size="sm" variant="ghost" onclick={() => setPath(name, null)}>Remove</Button>
        </SettingRow>
      {/if}
    {/each}
  </div>
  <SettingRow label="New path" stacked as="div">
    <div class="flex items-center gap-2">
      <Input aria-label="Project path glob" placeholder="/path/**" bind:value={glob} />
      <Select aria-label="New path access" bind:value={globAccess} items={access} />
      <Button size="sm" disabled={!settings || !glob.trim() || paths[glob.trim()] != null} onclick={addPath}
        >Add Path</Button
      >
    </div>
  </SettingRow>
</SettingsSection>
