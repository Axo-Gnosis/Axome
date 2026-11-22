<script lang="ts">
  import { onMount } from 'svelte'
  import { backend } from '$lib/api/backend'
  import { Badge } from '$lib/components/ui/badge'
  import { Button } from '$lib/components/ui/button'
  import { Input } from '$lib/components/ui/input'
  import { Switch } from '$lib/components/ui/switch'
  import { notify } from '$lib/features/notifications/state.svelte'
  import { getSettings, getSettingsHost, loadSettings, saveMemorySettings } from '../state/settings.svelte'
  import { setSettingsPage } from '../dialog/state.svelte'
  import { openFolder } from '$lib/features/workbench/state.svelte'
  import type { MemorySettings, MemoryStatus } from '$lib/types/backend'
  import { getErrorMessage } from '$lib/utils/client-error'
  import { timeAgo } from '$lib/utils/date'
  import { splitRemotePath } from '$lib/utils/paths'
  import SettingRow from '../dialog/SettingRow.svelte'
  import SettingsSection from '../dialog/SettingsSection.svelte'

  const BANK_NAME = '[a-z][a-z0-9-]*'
  const host = getSettingsHost()
  let settings = $derived(getSettings()?.memory)
  let status = $state<MemoryStatus | null>(null)
  let busy = $state(false)
  let name = $state('')
  let path = $state('')
  let disabled = $derived(!settings?.enabled || busy)
  let validName = $derived(new RegExp(`^${BANK_NAME}$`).test(name) && !status?.banks.some((bank) => bank.name === name))
  let embedLabel = $derived.by(() => {
    const embed = status?.embed
    switch (embed?.state) {
      case 'downloading':
        return `Downloading ${(embed.received / 1_000_000).toFixed(1)}/${(embed.total / 1_000_000).toFixed(1)} MB`
      case 'ready':
        return 'Ready'
      case 'failed':
        return 'Download failed'
      default:
        return 'Not downloaded'
    }
  })

  async function refreshStatus() {
    try {
      status = await backend.memory.status(host)
    } catch (error) {
      notify.error('Load memory status failed', getErrorMessage(error))
    }
  }

  onMount(() => {
    void loadSettings(host).catch((error) => notify.error('Load memory settings failed', getErrorMessage(error)))
    void refreshStatus()
  })

  $effect(() => {
    if (status?.embed.state !== 'downloading') return
    // ponytail: poll only this open page during downloads; use a host event if another surface needs it.
    const timer = setTimeout(() => void refreshStatus(), 2000)
    return () => clearTimeout(timer)
  })

  async function runAction(action: () => Promise<void>) {
    if (busy) return
    busy = true
    try {
      await action()
      await refreshStatus()
    } catch (error) {
      notify.error('Memory action failed', getErrorMessage(error))
    } finally {
      busy = false
    }
  }

  async function save(next: MemorySettings) {
    await saveMemorySettings(next, host)
  }

  function update(patch: Partial<MemorySettings>) {
    if (!settings) return
    const next = { ...settings, ...patch }
    void runAction(() => save(next))
  }

  function addBank() {
    if (!settings || !validName) return
    const next = { ...settings, banks: { ...settings.banks, [name]: { path: path.trim() || null } } }
    void runAction(async () => {
      await save(next)
      name = ''
      path = ''
    })
  }

  function removeBank(bank: string) {
    if (!settings) return
    const banks = { ...settings.banks }
    delete banks[bank]
    update({ banks, bank: settings.bank === bank ? 'main' : settings.bank })
  }

  function openBank(bank: string) {
    void runAction(async () => {
      const directory = await backend.memory.ensureBank(bank, host)
      const remote = host ? splitRemotePath(host) : null
      await openFolder(remote ? `axome://${remote.server}${directory}` : directory)
    })
  }
</script>

<SettingsSection title="Memory">
  <SettingRow
    label="Enable Memory"
    description="Adds memory tools to chats and captures notes after sessions. Notes are Markdown files on this host."
    as="div"
  >
    <Switch
      aria-label="Enable Memory"
      checked={settings?.enabled ?? false}
      disabled={!settings || busy}
      onCheckedChange={(enabled) => update({ enabled })}
    />
  </SettingRow>
  <SettingRow
    label="Capture model"
    description="Uses the @memory role. Unset, it follows @default and spends that model's tokens."
    as="div"
  >
    <Button variant="text" size="sm" onclick={() => setSettingsPage('models')}>Models</Button>
  </SettingRow>
</SettingsSection>

<div class:opacity-50={!settings?.enabled}>
  <SettingsSection title="Search">
    <SettingRow
      label="Semantic Search"
      description="Downloads a 133 MB embedding model once and runs it on this host."
      as="div"
    >
      <Switch
        aria-label="Semantic Search"
        checked={settings?.semantic ?? true}
        {disabled}
        onCheckedChange={(semantic) => update({ semantic })}
      />
    </SettingRow>
    <SettingRow label="Embedding Model" description={embedLabel} as="div">
      {#if status?.embed.state === 'failed'}
        <Button variant="text" size="sm" {disabled} onclick={() => runAction(() => backend.memory.retryDownload(host))}
          >Retry</Button
        >
      {/if}
    </SettingRow>
    {#if status?.embed.state === 'failed' && status.embed.error}
      <p class="text-sm text-danger" role="alert">{status.embed.error}</p>
    {/if}
  </SettingsSection>

  <SettingsSection title="Capture">
    <SettingRow label="After Compaction" as="div">
      <Switch
        aria-label="After Compaction"
        checked={settings?.capture.compaction ?? true}
        {disabled}
        onCheckedChange={(compaction) => settings && update({ capture: { ...settings.capture, compaction } })}
      />
    </SettingRow>
    <SettingRow label="After Closing a Chat" as="div">
      <Switch
        aria-label="After Closing a Chat"
        checked={settings?.capture.close ?? true}
        {disabled}
        onCheckedChange={(close) => settings && update({ capture: { ...settings.capture, close } })}
      />
    </SettingRow>
    {#if status?.lastCapture}
      {#if status.lastCapture.error}
        <p class="text-sm text-danger" role="alert">{status.lastCapture.error}</p>
      {:else}
        <p class="text-sm text-muted">
          Last capture {timeAgo(status.lastCapture.at)}: {status.lastCapture.added} added, {status.lastCapture.replaced} replaced
        </p>
      {/if}
    {/if}
  </SettingsSection>

  <SettingsSection title="Banks">
    <div class="divide-y divide-edge">
      {#each status?.banks ?? [] as bank (bank.name)}
        <SettingRow label={bank.name} as="div" stacked class="py-3 first:pt-0 last:pb-0">
          {#snippet description()}
            <span class="font-mono break-all">{bank.path}</span> · {bank.notes} notes
          {/snippet}
          <div class="flex flex-wrap items-center gap-2">
            {#if bank.default}<Badge>Default</Badge>{/if}
            <Button variant="text" size="sm" {disabled} onclick={() => openBank(bank.name)}>Open</Button>
            {#if !bank.default}
              <Button variant="text" size="sm" {disabled} onclick={() => update({ bank: bank.name })}
                >Make Default</Button
              >
            {/if}
            {#if bank.name !== 'main'}
              <Button variant="text" size="sm" {disabled} onclick={() => removeBank(bank.name)}>Remove</Button>
            {/if}
          </div>
        </SettingRow>
      {/each}
    </div>
    <SettingRow label="Add Bank" stacked as="div">
      <Input aria-label="Bank name" placeholder="Name" pattern={BANK_NAME} bind:value={name} {disabled} />
      <Input aria-label="Bank path" placeholder="Default location" bind:value={path} {disabled} />
      <div><Button size="sm" disabled={disabled || !validName} onclick={addBank}>Add</Button></div>
    </SettingRow>
  </SettingsSection>
</div>
