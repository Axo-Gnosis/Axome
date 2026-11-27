<script lang="ts">
  import { onMount } from 'svelte'
  import { backend } from '$lib/api/backend'
  import { Badge } from '$lib/components/ui/badge'
  import { Button } from '$lib/components/ui/button'
  import ApiKeyDialog from './ApiKeyDialog.svelte'
  import { Input, Textarea } from '$lib/components/ui/input'
  import { Select } from '$lib/components/ui/select'
  import { StatusDot } from '$lib/components/ui/status-dot'
  import { SelectorPicker } from '$lib/features/chat/elements/model-picker'
  import { formatCount } from '$lib/features/chat/format'
  import { notify } from '$lib/features/notifications/state.svelte'
  import { axoModels, settingsRoles } from '$lib/features/settings/state/axo.svelte'
  import type { AxoSearchProviderStatus, AxoWebToolsStatusPayload } from '$lib/types/backend'
  import { getErrorMessage } from '$lib/utils/client-error'
  import SettingRow from '../dialog/SettingRow.svelte'
  import SettingsSection from '../dialog/SettingsSection.svelte'
  import AxoNumberRow from './AxoNumberRow.svelte'
  import AxoTextRow from './AxoTextRow.svelte'
  import AxoToggleRow from './AxoToggleRow.svelte'
  import AxoChoiceRow from './AxoChoiceRow.svelte'
  import { createAxoFields } from './axoFields.svelte'

  const fields = createAxoFields()
  let settings = $derived(fields.settings)
  const available = axoModels(fields.host)
  let roles = $derived(settingsRoles(settings?.modelRoles))
  let agentName = $state('')
  let agentSelector = $state('')

  let webStatus = $state<AxoWebToolsStatusPayload | null>(null)
  let webBusy = $state(false)
  let keyTarget = $state<AxoSearchProviderStatus | null>(null)
  let keyValue = $state('')

  async function refreshWebStatus() {
    try {
      webStatus = await backend.axo.webToolsStatus(fields.host)
    } catch (error) {
      notify.error('Load web tools status failed', getErrorMessage(error))
    }
  }

  onMount(() => {
    void refreshWebStatus()
  })

  async function runWebAction(action: () => Promise<void>) {
    webBusy = true
    try {
      await action()
      await refreshWebStatus()
    } catch (error) {
      notify.error('Web tools action failed', getErrorMessage(error))
    } finally {
      webBusy = false
    }
  }

  function saveSearchKey() {
    if (!keyTarget || !keyValue.trim()) return
    const id = keyTarget.id
    const key = keyValue.trim()
    void runWebAction(async () => {
      await backend.axo.setSearchKey(id, key, fields.host)
      keyTarget = null
      keyValue = ''
    })
  }

  function removeSearchKey(id: string) {
    void runWebAction(() => backend.axo.removeSearchKey(id, fields.host))
  }

  function refreshDocs(source: string | null) {
    void runWebAction(() => backend.axo.refreshDocs(source, fields.host))
  }

  function removeDocs(source: string | null) {
    void runWebAction(() => backend.axo.removeDocs(source, fields.host))
  }

  function searchSourceLabel(provider: AxoSearchProviderStatus): string {
    switch (provider.source.kind) {
      case 'config':
        return 'Configured'
      case 'apiKey':
        return 'Stored key'
      case 'oauth':
        return 'Logged in'
      case 'env':
        return `Env: ${provider.source.name}`
      case 'none':
        return provider.requiresKey ? 'Not configured' : 'Keyless default'
    }
  }

  function formatBytes(bytes: number): string {
    if (bytes < 1024) return `${bytes} B`
    if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`
    return `${(bytes / (1024 * 1024)).toFixed(1)} MB`
  }

  function addModel() {
    const name = agentName.trim()
    const selector = agentSelector.trim()
    if (!name || !selector || Object.hasOwn(settings?.subagents.models ?? {}, name)) return
    fields.save(['subagents', 'models', name], selector)
    agentName = ''
    agentSelector = ''
  }
</script>

<!-- The paired controls edit the model, effort and tier in one selector. -->
{#snippet selectorPicker(path: string[], saved: string, label: string, none?: string)}
  <SelectorPicker
    class="flex-1"
    value={String(fields.value(path, saved))}
    models={available.models}
    {roles}
    {none}
    modelLabel={label}
    disabled={!settings}
    onSelect={(joined) => fields.save(path, joined || null, joined)}
  />
{/snippet}

<SettingsSection title="Read">
  <AxoNumberRow
    {fields}
    path={['read', 'outlineMinBytes']}
    label={'Outline code files from (bytes, 0 = off)'}
    value={settings?.read.outlineMinBytes ?? null}
  />
</SettingsSection>

<SettingsSection title="Bash">
  <AxoChoiceRow
    {fields}
    path={['bash', 'devShell']}
    label={'Development shell'}
    value={settings?.bash.devShell ?? 'off'}
    choices={['off', 'direnv', 'nix']}
  />
  <AxoTextRow
    {fields}
    path={['bash', 'devShellRef']}
    label="Shell reference"
    value={settings?.bash.devShellRef ?? null}
    placeholder=".#default"
  />
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
  <AxoToggleRow
    {fields}
    path={['bash', 'blockToolViews']}
    label={'Block tool-equivalent views'}
    description="Refuse a bash command that only views or searches files; the error names the read or grep call to use."
    checked={settings?.bash.blockToolViews ?? false}
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
  <SettingRow label="Subagent tier" description="Used when a subagent model selector has no tier suffix.">
    <Select
      aria-label="Subagent tier"
      value={settings?.subagents.serviceTier ?? 'standard'}
      disabled={!settings}
      items={[
        { value: 'standard', label: 'Standard' },
        { value: 'inherit', label: "Inherit parent's tier" }
      ]}
      onValueChange={(next) => fields.save(['subagents', 'serviceTier'], next)}
    />
  </SettingRow>
  <SettingRow
    label="Context files"
    description={settings && typeof settings.subagents.contextFiles === 'object'
      ? 'A file-specific map is configured. Choosing an option replaces it.'
      : undefined}
  >
    <Select
      aria-label="Subagent context files"
      value={settings?.subagents.contextFiles === false
        ? 'off'
        : (settings?.subagents.contextFiles ?? 'nested') === 'nested'
          ? 'nested'
          : 'on'}
      disabled={!settings}
      onValueChange={(next) => fields.save(['subagents', 'contextFiles'], next === 'nested' ? 'nested' : next === 'on')}
      items={[
        { value: 'off', label: 'Off' },
        { value: 'nested', label: 'Nested only' },
        { value: 'on', label: 'Same as parent' }
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
    <div class="flex flex-wrap items-center gap-2">
      <Input class="min-w-0 flex-1" aria-label="Agent name" placeholder="Agent name" bind:value={agentName} />
      <SelectorPicker
        class="flex-1"
        value={agentSelector}
        models={available.models}
        {roles}
        modelLabel="Agent model"
        onSelect={(joined) => (agentSelector = joined)}
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

<SettingsSection title="Web Search">
  <AxoToggleRow
    {fields}
    path={['webSearch', 'enabled']}
    label="Enable web search and fetch"
    checked={settings?.webSearch.enabled ?? true}
  />
  <SettingRow
    label="Provider chain"
    description="Comma-separated order (searxng, tavily, exa, brave, perplexity, parallel, openai-codex, anthropic, openai, google). Leave blank for default."
    stacked
  >
    <Input
      value={String(fields.value(['webSearch', 'providers'], settings?.webSearch.providers.join(', ') ?? ''))}
      placeholder="Default (keyed engines → parallel → grounded LLM)"
      disabled={!settings}
      oninput={(event) =>
        fields.save(
          ['webSearch', 'providers'],
          event.currentTarget.value
            .split(',')
            .map((part) => part.trim())
            .filter(Boolean),
          event.currentTarget.value
        )}
    />
  </SettingRow>
  <AxoTextRow
    {fields}
    path={['webSearch', 'searxngUrl']}
    label="SearXNG URL"
    value={settings?.webSearch.searxngUrl ?? null}
    placeholder="https://search.example.com"
  />
  <SettingRow label="Grounded fallback model" stacked as="div">
    {@render selectorPicker(
      ['webSearch', 'groundedModel'],
      settings?.webSearch.groundedModel ?? '',
      'Grounded search model',
      'Auto (session provider first)'
    )}
  </SettingRow>
  <div class="divide-y divide-edge">
    {#each webStatus?.searchProviders ?? [] as provider (provider.id)}
      <SettingRow label={provider.label} class="py-3 first:pt-0 last:pb-0">
        {#snippet description()}
          <span class="inline-flex items-center gap-1.5">
            <StatusDot tone={provider.configured ? 'success' : 'idle'} />
            <span>{searchSourceLabel(provider)}</span>
          </span>
        {/snippet}
        <div class="flex items-center gap-2">
          {#if provider.id !== 'searxng'}
            {#if provider.source.kind === 'apiKey'}
              <Button size="sm" variant="ghost" disabled={webBusy} onclick={() => removeSearchKey(provider.id)}
                >Remove</Button
              >
            {/if}
            <Button
              size="sm"
              variant="outline"
              disabled={webBusy}
              onclick={() => {
                keyTarget = provider
                keyValue = ''
              }}>{provider.source.kind === 'apiKey' ? 'Update key' : 'Set key'}</Button
            >
          {:else}
            <Badge variant={provider.configured ? 'success' : 'default'}
              >{provider.configured ? 'Active' : 'Unset'}</Badge
            >
          {/if}
        </div>
      </SettingRow>
    {/each}
  </div>
</SettingsSection>

<SettingsSection title="Docs Cache">
  <AxoToggleRow
    {fields}
    path={['docs', 'enabled']}
    label="Enable search_docs"
    checked={settings?.docs.enabled ?? true}
  />
  <AxoNumberRow
    {fields}
    path={['docs', 'ttlHours']}
    label="URL refresh TTL (hours)"
    value={settings?.docs.ttlHours ?? null}
  />
  <AxoNumberRow
    {fields}
    path={['docs', 'retentionDays']}
    label="Retention (days)"
    value={settings?.docs.retentionDays ?? null}
  />
  <SettingRow label="Pre-configured sources (one URL, llms.txt, or path per line)" stacked>
    <Textarea
      value={String(fields.value(['docs', 'sources'], settings?.docs.sources.join('\n') ?? ''))}
      rows={3}
      disabled={!settings}
      oninput={(event) =>
        fields.save(
          ['docs', 'sources'],
          event.currentTarget.value
            .split('\n')
            .map((line) => line.trim())
            .filter(Boolean),
          event.currentTarget.value
        )}
    />
  </SettingRow>
  <SettingRow
    label="Cached index"
    description={webStatus
      ? `${formatCount(webStatus.docsSources.length, 'source')} · ${formatBytes(webStatus.cacheBytes)}`
      : 'Loading…'}
  >
    <div class="flex items-center gap-2">
      <Button
        size="sm"
        variant="outline"
        disabled={webBusy || !webStatus?.docsSources.length}
        onclick={() => refreshDocs(null)}>Refresh all</Button
      >
      <Button
        size="sm"
        variant="ghost"
        disabled={webBusy || !webStatus?.docsSources.length}
        onclick={() => removeDocs(null)}>Clear cache</Button
      >
    </div>
  </SettingRow>
  {#if webStatus && webStatus.docsSources.length > 0}
    <div class="divide-y divide-edge">
      {#each webStatus.docsSources as src (src.key)}
        <SettingRow label={src.key} class="py-2.5 first:pt-0 last:pb-0">
          {#snippet description()}
            <span>{formatCount(src.sections, 'section')} · {src.kind}{src.stale ? ' · stale' : ''}</span>
          {/snippet}
          <div class="flex items-center gap-2">
            <Button size="sm" variant="ghost" disabled={webBusy} onclick={() => refreshDocs(src.key)}>Refresh</Button>
            <Button size="sm" variant="ghost" disabled={webBusy} onclick={() => removeDocs(src.key)}>Remove</Button>
          </div>
        </SettingRow>
      {/each}
    </div>
  {/if}
</SettingsSection>

<SettingsSection
  title="Browser"
  description="Playwright browser automation for Axo. Auto-detects your installed Chromium-based browser (Helium, Chrome, Chromium, Brave, etc.)."
>
  <AxoToggleRow
    {fields}
    path={['browser', 'enabled']}
    label="Enable browser tool"
    description="Register the deferred browser tool in Axo sessions."
    checked={settings?.browser.enabled ?? true}
  />
  <AxoToggleRow
    {fields}
    path={['browser', 'headless']}
    label="Headless by default"
    description="Run without a visible browser window unless a tool call passes headed: true."
    checked={settings?.browser.headless ?? true}
  />
  <AxoTextRow
    {fields}
    path={['browser', 'executablePath']}
    label="Browser executable"
    description="Optional command name or path (e.g. helium, chromium, /usr/bin/brave). Leave blank to auto-detect."
    placeholder="Auto-detect (helium, chrome, chromium, brave…)"
    value={settings?.browser.executablePath ?? null}
  />
  <AxoTextRow
    {fields}
    path={['browser', 'cdpUrl']}
    label="CDP endpoint URL"
    description="Optional default remote debugging URL (e.g. http://127.0.0.1:9222) to attach to instead of launching a browser."
    placeholder="http://127.0.0.1:9222"
    value={settings?.browser.cdpUrl ?? null}
  />
  <AxoNumberRow
    {fields}
    path={['browser', 'idleCloseSecs']}
    label="Idle tab timeout (seconds)"
    value={settings?.browser.idleCloseSecs ?? null}
  />
</SettingsSection>

<ApiKeyDialog
  open={keyTarget !== null}
  title={`Set ${keyTarget?.label ?? ''} API key`}
  description={`Saved to auth.json alongside model credentials. Environment fallback: ${keyTarget?.env.join(', ') || 'none'}.`}
  bind:key={keyValue}
  busy={webBusy}
  onSave={saveSearchKey}
  onClose={() => {
    keyTarget = null
    keyValue = ''
  }}
/>
