<script lang="ts">
  import { onDestroy, tick, untrack } from 'svelte'
  import * as Alert from '$lib/components/ui/alert'
  import { Badge } from '$lib/components/ui/badge'
  import { Button } from '$lib/components/ui/button'
  import { Spinner } from '$lib/components/ui/progress'
  import { StatusDot } from '$lib/components/ui/status-dot'
  import { Switch } from '$lib/components/ui/switch'
  import type { AxoExtension, AxoProjectAccess } from '$lib/types/backend'
  import { splitRemotePath } from '$lib/utils/paths'
  import {
    clearExtensionProjectScroll,
    closeExtensionDetail,
    getExtensionDetail,
    isExtensionProjectScroll,
    setExtensionDetail,
    setSettingsPage
  } from '../dialog/state.svelte'
  import SettingRow from '../dialog/SettingRow.svelte'
  import SettingsSection from '../dialog/SettingsSection.svelte'
  import {
    createExtensionsState,
    extensionRuntimeAction,
    extensionStateText,
    needsExtensionAttention
  } from '../state/extensions.svelte'
  import ExtensionDetail from './ExtensionDetail.svelte'
  import ExtensionFindings from './ExtensionFindings.svelte'
  import ExtensionInstallDialog from './ExtensionInstallDialog.svelte'
  import ExtensionRunnerMemory from './ExtensionRunnerMemory.svelte'
  import ProviderCredentials, {
    credentialReminder,
    providerStatus,
    sourceDetail,
    sourceLabel
  } from './ProviderCredentials.svelte'

  let {
    host,
    folderPath,
    hostLabel
  }: {
    host: string | null
    folderPath: string | null
    hostLabel: string
  } = $props()

  // SettingsDialog keys this view on both the owner and its project.
  const controls = untrack(() => createExtensionsState(host, folderPath))
  onDestroy(controls.release)
  const scopeFolder = $derived(folderPath ? (splitRemotePath(folderPath)?.path ?? folderPath) : null)
  const detailTarget = $derived(getExtensionDetail())
  const projectDetail = $derived(detailTarget?.project ?? false)
  const detail = $derived(
    detailTarget
      ? ((projectDetail ? controls.payload?.project?.items : controls.payload?.host)?.find(
          (item) => item.id === detailTarget.id
        ) ?? null)
      : null
  )
  const projectCatalog = $derived(controls.payload?.project)
  const attentionCount = $derived(controls.payload?.host.filter(needsExtensionAttention).length ?? 0)
  const bunUnavailable = $derived(controls.bunUnavailable)
  const disabled = $derived(controls.busy || controls.stale)
  let filter = $state<'all' | 'attention'>('all')
  let install = $state(false)
  let outputTarget = $state.raw<{ id: string; project: boolean } | null>(null)
  let projectHeading = $state<HTMLElement | null>(null)

  const scopedProviders = $derived.by(() => {
    if (projectCatalog?.access !== 'trust') return []
    const enabled = new Set(projectCatalog.items.filter((item) => item.enabled).map((item) => item.id))
    return (
      controls.providers?.providers.filter(
        (provider) => provider.originProject && provider.origin !== null && enabled.has(provider.origin)
      ) ?? []
    )
  })

  $effect(() => {
    if (attentionCount === 0) filter = 'all'
  })

  $effect(() => {
    if (!detailTarget) outputTarget = null
  })

  $effect(() => {
    if (detailTarget || !isExtensionProjectScroll() || !projectHeading) return
    const heading = projectHeading
    void tick().then(() => {
      heading.scrollIntoView({ block: 'start' })
      clearExtensionProjectScroll()
    })
  })

  function openDetail(item: AxoExtension, project: boolean, output = false) {
    outputTarget = output ? { id: item.id, project } : null
    setExtensionDetail({ id: item.id, project })
  }

  function back() {
    outputTarget = null
    closeExtensionDetail()
  }
</script>

{#snippet rows(items: AxoExtension[], project: boolean, access: AxoProjectAccess | null)}
  <div class="divide-y divide-edge">
    {#each items as item (item.id)}
      {@const action = extensionRuntimeAction(item, access)}
      <article class="flex items-center justify-between gap-4 py-2">
        <div class="flex min-w-0 items-start gap-2">
          <StatusDot
            class="mt-2"
            tone={needsExtensionAttention(item)
              ? 'danger'
              : item.phase === 'ready'
                ? 'success'
                : item.phase === 'installing' || item.phase === 'starting' || item.phase === 'restarting'
                  ? 'working'
                  : 'idle'}
          />
          <div class="flex min-w-0 flex-col items-start">
            <Button
              class="h-auto min-w-0 justify-start px-0 py-0 text-left font-mono break-all whitespace-normal text-fg"
              variant="text"
              onclick={() => openDetail(item, project)}>{item.id}</Button
            >
            <p class="text-sm break-words whitespace-pre-wrap text-muted">{extensionStateText(item, access)}</p>
            {#if item.failure}<p class="text-xs text-danger-bright">{item.failure.title}</p>{/if}
          </div>
        </div>
        <div class="flex shrink-0 items-center gap-2">
          {#if action === 'off'}
            <Button size="xs" {disabled} onclick={() => controls.setEnabled(item.id, project, false)}>Turn Off</Button>
          {:else if action === 'retry'}
            <Button size="xs" disabled={disabled || bunUnavailable} onclick={() => controls.retry(item.id, project)}
              >Retry</Button
            >
          {:else if action === 'dependencies'}
            <Button
              size="xs"
              disabled={disabled || bunUnavailable}
              onclick={() => controls.installDependencies(item.id, project)}>Install Dependencies</Button
            >
          {/if}
          {#if item.failure || action === 'output'}
            <Button size="xs" onclick={() => openDetail(item, project, true)}>View Output</Button>
          {/if}
          <Switch
            aria-label={`Turn on ${item.id}`}
            {disabled}
            bind:checked={
              () => item.enabled,
              (value) => {
                void controls.setEnabled(item.id, project, value)
              }
            }
          />
        </div>
      </article>
    {/each}
  </div>
{/snippet}

{#if controls.error}
  <Alert.Root variant="danger" class="mb-5">
    <div class="flex items-center justify-between gap-2">
      <p class="break-words whitespace-pre-wrap">{controls.error}</p>
      <Button size="xs" disabled={controls.busy} onclick={controls.refresh}>Try Again</Button>
    </div>
  </Alert.Root>
{/if}

{#if controls.payload}
  {@const payload = controls.payload}
  {#if bunUnavailable}
    <Alert.Root variant="warning" class="mb-5">
      <div class="flex items-center justify-between gap-3">
        <div class="min-w-0">
          <p class="font-semibold">Bun is missing on {hostLabel}</p>
          <p class="break-words">
            No extension can start until it's back. Axome looked for {payload.bun.searched.join(', ') ||
              'the bundled Bun binary and PATH'}.
          </p>
        </div>
        <Button size="xs" {disabled} onclick={() => controls.checkBun()}>Check Again</Button>
      </div>
    </Alert.Root>
  {/if}

  {#if detailTarget}
    {#if detail}
      {#key JSON.stringify([projectDetail, detail.id])}
        <ExtensionDetail
          state={controls}
          item={detail}
          project={projectDetail}
          {host}
          {hostLabel}
          initialOutput={outputTarget?.id === detail.id && outputTarget?.project === projectDetail}
          onBack={back}
        />
      {/key}
    {:else}
      <p class="mb-3 text-sm text-muted">Extension is no longer available.</p>
      <Button class="self-start" size="sm" onclick={back}>Back to Extensions</Button>
    {/if}
  {:else}
    {@const filtered =
      filter === 'attention' && attentionCount > 0 ? payload.host.filter(needsExtensionAttention) : payload.host}
    {@const off = filtered.filter((item) => !item.enabled)}
    <SettingsSection
      title="Extensions"
      description="When two extensions add the same provider, the one lower in the list wins."
    >
      {#snippet actions()}
        {#if controls.loading}<Spinner label="Refreshing extensions" />{/if}
        <Button size="sm" {disabled} onclick={() => (install = true)}>Install…</Button>
      {/snippet}
      {#if attentionCount > 0}
        <div class="flex gap-2">
          <Button
            size="xs"
            aria-pressed={filter === 'all'}
            variant={filter === 'all' ? 'outline' : 'text'}
            onclick={() => (filter = 'all')}>All {payload.host.length}</Button
          >
          <Button
            size="xs"
            aria-pressed={filter === 'attention'}
            variant={filter === 'attention' ? 'outline' : 'text'}
            onclick={() => (filter = 'attention')}>Needs attention {attentionCount}</Button
          >
        </div>
      {/if}
      {#if filtered.length}
        {@render rows(
          filtered.filter((item) => item.enabled),
          false,
          null
        )}
        {#if off.length}
          <div>
            <h3 class="mb-1 text-xs tracking-wide text-muted uppercase">Off</h3>
            {@render rows(off, false, null)}
          </div>
        {/if}
      {:else}<p class="text-sm text-muted">No extensions found.</p>{/if}
      <ExtensionFindings collisions={payload.collisions} diagnostics={payload.diagnostics} />
    </SettingsSection>

    {#if folderPath}
      <SettingsSection title="This Project">
        {#if projectCatalog}
          {@const project = projectCatalog}
          <div class="scroll-mt-8" bind:this={projectHeading}>
            <p class="font-mono break-all text-fg">{project.folder}</p>
            <p class="text-sm text-muted">
              {project.items.length} project extensions. They run only in this folder, once you trust it and turn them on.
            </p>
          </div>
          <SettingRow
            as="div"
            label="Workspace trust"
            description={project.access === 'trust' ? 'Trusted' : "Restricted. Project extensions can't run."}
          >
            {#if project.access !== 'trust'}<Button size="xs" onclick={() => setSettingsPage('project-access')}
                >Manage Trust</Button
              >{/if}
          </SettingRow>
          {#if !project.rootExists}<p class="text-sm text-muted">No .axome/extensions folder in this project.</p>
          {:else if !project.items.length}<p class="text-sm text-muted">
              No extensions found in .axome/extensions.
            </p>{/if}
          {#if project.items.length}{@render rows(project.items, true, project.access)}{/if}
          <ExtensionFindings collisions={project.collisions} diagnostics={project.diagnostics} />

          {#if project.access === 'trust' && (controls.providerError || scopedProviders.length)}
            <div class="border-t border-edge pt-4">
              <h3 class="mb-3 text-lg font-semibold">Project Providers</h3>
              {#if controls.providerError}
                <Alert.Root variant="danger" class="mb-2">
                  <div class="flex items-center justify-between gap-2">
                    <p>{controls.providerError}</p>
                    <Button size="xs" disabled={controls.loading} onclick={controls.refresh}>Try Again</Button>
                  </div>
                </Alert.Root>
              {/if}
              <ProviderCredentials
                {host}
                {scopeFolder}
                disabled={disabled || controls.providerError !== null || controls.loading}
                onChanged={() => void controls.refresh()}
              >
                {#snippet children(actions)}
                  <div class="space-y-3">
                    {#each scopedProviders as provider (provider.id)}
                      {@const status = providerStatus(provider)}
                      {@const label = sourceLabel(provider)}
                      {@const detail = sourceDetail(provider)}
                      {#snippet providerDescription()}
                        <span
                          >{provider.models}
                          {provider.models === 1 ? 'model' : 'models'}{#if label}
                            · <Badge>{label}</Badge>{/if}</span
                        >
                        {#if provider.status.kind === 'error'}<span class="block text-danger-bright"
                            >{provider.status.message}</span
                          >{/if}
                        {#if detail}<span class="block font-mono">{detail}</span>{/if}
                      {/snippet}
                      <SettingRow as="div" label={provider.id} description={providerDescription}>
                        <StatusDot tone={status.tone} label={status.label} />
                        {@render actions(provider)}
                      </SettingRow>
                    {/each}
                  </div>
                {/snippet}
              </ProviderCredentials>
              {#if scopedProviders.length}<p class="mt-3 text-xs text-muted">{credentialReminder}</p>{/if}
            </div>
          {/if}
        {:else}<p class="text-sm text-muted">Project extension state is unavailable.</p>{/if}
      </SettingsSection>
    {/if}
  {/if}
{:else if controls.loading}
  <p class="flex items-center gap-2 text-sm text-muted"><Spinner /> Loading extensions…</p>
{/if}

<ExtensionRunnerMemory {host} {scopeFolder} catalog={controls.payload} active={!detailTarget} />

{#if install}
  <ExtensionInstallDialog {host} {hostLabel} state={controls} update={null} onClose={() => (install = false)} />
{/if}
