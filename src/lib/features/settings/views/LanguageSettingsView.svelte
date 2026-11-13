<script lang="ts">
  import { Badge } from '$lib/components/ui/badge'
  import { Button } from '$lib/components/ui/button'
  import { Input } from '$lib/components/ui/input'
  import BinaryRow from './BinaryRow.svelte'
  import * as Tabs from '$lib/components/ui/tabs'
  import { AlertCircleIcon, RefreshCwIcon } from '@hugeicons/core-free-icons'
  import Icon from '$lib/icons/Icon.svelte'
  import { getLspServers, getLspServersLoading, loadLspServers } from '$lib/features/settings/state/lspSettings.svelte'
  import { getFormatterStatuses, loadFormatterStatuses } from '$lib/features/settings/state/formatterSettings.svelte'
  import { getFormatterPolicy } from '$lib/features/extensions/catalog.svelte'
  import { notify } from '$lib/features/notifications/state.svelte'
  import {
    getSettings,
    getSettingsHost,
    saveFormattingSettings,
    saveLspServerConfig
  } from '$lib/features/settings/state/settings.svelte'
  import { getActiveFolderPath } from '$lib/features/workbench/state.svelte'
  import type {
    SettingsPage,
    LanguageFormatterPolicy,
    LspServerConfig,
    LspServerSettingsEntry
  } from '$lib/types/backend'
  import { getErrorMessage } from '$lib/utils/client-error'
  import { createTrackedAsyncLoad } from '$lib/utils/trackedAsyncLoad.svelte'
  import { createAutoSaver, trackSave } from './autoSaver.svelte'
  import SettingsSection from '../dialog/SettingsSection.svelte'
  import SettingRow from '../dialog/SettingRow.svelte'

  let { definition }: { definition: SettingsPage } = $props()

  let settings = $derived(getSettings())
  const host = getSettingsHost()
  let activeFolderPath = $derived(getActiveFolderPath() ?? undefined)
  let lspLoading = $derived(getLspServersLoading())
  let allServers = $derived(getLspServers())
  let servers = $derived(
    allServers.filter((entry) => definition.server_definition_ids.includes(entry.server.server_definition_id))
  )
  let policies = $derived(definition.language_ids.flatMap((id) => getFormatterPolicy(host, id) ?? []))
  let formatterStatuses = $derived(getFormatterStatuses())

  type Draft = {
    enabled: boolean
    binaryPath: string
    extraArgs: string
    trace: 'off' | 'messages' | 'verbose'
    settings: unknown | null
  }

  let drafts = $state<Record<string, Draft>>({})

  function seed(id: string): Draft {
    const config = settings?.lsp.servers[id]
    if (!config) throw new Error(`Missing global settings for language server: ${id}`)
    return {
      enabled: config.enabled,
      binaryPath: config.binaryPathOverride ?? '',
      extraArgs: config.extraArgs.join(' '),
      trace: config.trace,
      settings: config.settings ?? null
    }
  }

  const saver = createAutoSaver()

  // Folder switches refresh runtime status; host-keyed remounts own drafts.
  const lspLoader = createTrackedAsyncLoad<string | undefined>()
  $effect(() => {
    const folderPath = activeFolderPath
    lspLoader.run(folderPath, () => loadLspServers(folderPath))
  })

  const formatterLoader = createTrackedAsyncLoad<string | undefined>()
  $effect(() => {
    const folderPath = activeFolderPath
    formatterLoader.run(folderPath, () => loadFormatterStatuses(folderPath))
  })

  function selectedFormatter(policy: LanguageFormatterPolicy): string {
    const stored = settings?.formatting[policy.language_id]?.formatter
    return stored && policy.options.some((option) => option.id === stored) ? stored : policy.default
  }

  async function saveFormatter(policy: LanguageFormatterPolicy, formatter: string) {
    if (!settings || selectedFormatter(policy) === formatter) return
    try {
      await trackSave(
        saveFormattingSettings(
          {
            ...settings.formatting,
            [policy.language_id]: { formatter }
          },
          host
        )
      )
    } catch (error) {
      notify.error('Save formatting settings failed', getErrorMessage(error))
    }
  }

  async function saveServer(snapshot: LspServerConfig, host: string | null) {
    try {
      await saveLspServerConfig(snapshot, host)
    } catch (error) {
      notify.error('Save language server failed', getErrorMessage(error))
    }
  }

  function updateServer(id: string, key: keyof Draft, value: boolean | string) {
    const draft = drafts[id] ?? seed(id)
    const next = { ...draft, [key]: value }
    drafts = { ...drafts, [id]: next }
    const snapshot: LspServerConfig = {
      serverDefinitionId: id,
      enabled: next.enabled,
      binaryPathOverride: next.binaryPath.trim() || null,
      extraArgs: splitArgs(next.extraArgs),
      trace: next.trace,
      settings: next.settings
    }
    saver.schedule(id, () => saveServer(snapshot, host))
  }

  async function refresh() {
    try {
      await Promise.all([loadLspServers(activeFolderPath), loadFormatterStatuses(activeFolderPath)])
    } catch (error) {
      notify.error('Refresh language servers failed', getErrorMessage(error))
    }
  }

  function splitArgs(value: string): string[] {
    return value
      .split(/\s+/)
      .map((segment) => segment.trim())
      .filter(Boolean)
  }

  function relatedLanguages(entry: LspServerSettingsEntry): string[] {
    const pageLanguages = new Set(definition.language_ids)
    const languages = new Set<string>()
    for (const selector of entry.server.document_selectors) {
      if (selector.language && !pageLanguages.has(selector.language)) {
        languages.add(selector.language)
      }
    }
    return [...languages].sort()
  }
</script>

<SettingsSection title="Formatting">
  {#if policies.length > 0}
    {#if settings}
      {#each policies as policy (policy.language_id)}
        {@const selected = selectedFormatter(policy)}
        {@const status = formatterStatuses.find((entry) => entry.formatter_id === selected)}
        <SettingRow label={policy.language_label} stacked as="div">
          <Tabs.Root value={selected} onValueChange={(value) => saveFormatter(policy, value)}>
            <Tabs.List>
              {#each policy.options as option (option.id)}
                <Tabs.Trigger value={option.id}>{option.label}</Tabs.Trigger>
              {/each}
            </Tabs.List>
          </Tabs.Root>
          {#if status}
            <div class="flex min-w-0 flex-col gap-0.5">
              <div class="flex items-center gap-2">
                <Badge variant={status.status === 'available' ? 'success' : 'warning'}>{status.status}</Badge>
              </div>
              <span class="truncate font-mono text-xs text-muted">
                {status.resolved_path ?? status.install_hint}
              </span>
            </div>
            {#if status.status === 'missing' && status.message}
              <div class="flex items-center gap-2 bg-danger-bg/40 py-2 text-sm text-danger-bright">
                <Icon icon={AlertCircleIcon} size={14} />
                <span>{status.message}</span>
              </div>
            {/if}
          {/if}
        </SettingRow>
      {/each}
    {:else}
      <p class="text-sm text-muted">Loading formatting settings&hellip;</p>
    {/if}
  {:else}
    <p class="text-sm text-muted">Formatting currently follows the language server path.</p>
    <div>
      <Badge>LSP</Badge>
    </div>
  {/if}
</SettingsSection>

<SettingsSection title="Language Servers">
  {#snippet actions()}
    <Button variant="outline" size="xs" onclick={refresh}>
      <Icon icon={RefreshCwIcon} size={12} />
      Refresh
    </Button>
  {/snippet}

  {#if lspLoading && servers.length === 0}
    <div class="py-8 text-sm text-muted">Loading language servers&hellip;</div>
  {:else if servers.length === 0}
    <div class="py-8 text-sm text-muted">No language servers are attached to this language yet.</div>
  {:else if settings}
    <div class="flex flex-col divide-y divide-edge">
      {#each servers as entry (entry.server.server_definition_id)}
        {@const id = entry.server.server_definition_id}
        {@const draft = drafts[id] ?? seed(id)}
        {@const extraLanguages = relatedLanguages(entry)}

        <BinaryRow
          label={entry.server.label}
          status={entry.server.status}
          path={entry.server.resolved_path ?? entry.server.install_hint}
          message={entry.server.message}
          enabled={draft.enabled}
          onEnabledChange={(value) => updateServer(id, 'enabled', value)}
        >
          {#snippet meta()}{#if extraLanguages.length > 0}<span class="text-xs text-subtle">Shared config</span
              >{/if}{/snippet}
          <SettingRow label="Binary path" stacked>
            <Input
              value={draft.binaryPath}
              oninput={(event: Event) =>
                updateServer(id, 'binaryPath', (event.currentTarget as HTMLInputElement).value)}
              placeholder="Use detected binary"
            />
          </SettingRow>

          <SettingRow label="Extra args" stacked>
            <Input
              value={draft.extraArgs}
              oninput={(event: Event) => updateServer(id, 'extraArgs', (event.currentTarget as HTMLInputElement).value)}
              placeholder="--stdio already comes from the manifest"
            />
          </SettingRow>

          <SettingRow label="Trace" as="div">
            <div class="flex flex-wrap gap-2">
              {#each ['off', 'messages', 'verbose'] as trace}
                <Button
                  variant={draft.trace === trace ? 'outline' : 'ghost'}
                  size="xs"
                  onclick={() => updateServer(id, 'trace', trace)}
                >
                  {trace}
                </Button>
              {/each}
            </div>
          </SettingRow>

          <SettingRow label="Preferences" stacked as="div">
            <p class="text-sm text-muted">
              Edit nested LSP preferences in <span class="font-mono">settings.jsonc</span> under
              <span class="font-mono">lsp.servers.{id}.settings</span>.
            </p>
          </SettingRow>
        </BinaryRow>
      {/each}
    </div>
  {/if}
</SettingsSection>
