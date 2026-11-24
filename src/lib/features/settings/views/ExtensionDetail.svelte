<script lang="ts">
  import { onDestroy, untrack } from 'svelte'
  import * as Alert from '$lib/components/ui/alert'
  import { Button } from '$lib/components/ui/button'
  import { Input } from '$lib/components/ui/input'
  import { Select } from '$lib/components/ui/select'
  import { Switch } from '$lib/components/ui/switch'
  import type { AxoExtension } from '$lib/types/backend'
  import { copyToClipboard } from '$lib/utils/clipboard'
  import { setSettingsPage } from '../dialog/state.svelte'
  import SettingsSection from '../dialog/SettingsSection.svelte'
  import SettingRow from '../dialog/SettingRow.svelte'
  import {
    extensionMissing,
    extensionRuntimeAction,
    extensionStateText,
    shortRevision,
    type ExtensionsState
  } from '../state/extensions.svelte'
  import ExtensionFindings from './ExtensionFindings.svelte'
  import ExtensionInstallDialog from './ExtensionInstallDialog.svelte'
  import ExtensionUninstallDialog from './ExtensionUninstallDialog.svelte'

  let {
    state: controls,
    item,
    project,
    host,
    hostLabel,
    initialOutput = false,
    onBack
  }: {
    state: ExtensionsState
    item: AxoExtension
    project: boolean
    host: string | null
    hostLabel: string
    initialOutput?: boolean
    onBack: () => void
  } = $props()

  const catalog = $derived(controls.payload)
  const owner = $derived(project ? catalog?.project : catalog)
  const access = $derived(project ? (catalog?.project?.access ?? null) : null)
  const list = $derived((project ? catalog?.project?.items : catalog?.host) ?? [])
  const enabled = $derived(list.filter((entry) => entry.enabled))
  const otherMembers = $derived(
    item.group === null ? [] : enabled.filter((entry) => entry.group === item.group && entry.id !== item.id)
  )
  const groups = $derived.by(() => {
    const counts = new Map<string, number>()
    for (const { group } of enabled) if (group !== null) counts.set(group, (counts.get(group) ?? 0) + 1)
    return [...counts]
      .sort(([a], [b]) => a.localeCompare(b))
      .map(([value, count]) => ({ value, label: `Group ${value} (${count})` }))
  })
  const runtime = $derived(extensionRuntimeAction(item, access))
  const authored = $derived(item.origin === 'userFolder' || item.origin === 'projectFolder')
  const installed = $derived(item.origin === 'installed' && !extensionMissing(item))
  const disabled = $derived(controls.busy || controls.stale)
  const bunUnavailable = $derived(controls.bunUnavailable)
  const transitional = $derived(item.phase === 'installing' || item.phase === 'starting' || item.phase === 'restarting')
  const codeDisabled = $derived(disabled || bunUnavailable || !item.enabled || (project && access !== 'trust'))
  const adds = $derived(item.adds)
  const failure = $derived(
    item.failure ??
      (item.problem?.kind === 'dependencyInstall' ? { title: 'Dependency installation failed', output: null } : null)
  )
  let showOutput = $state(untrack(() => initialOutput))
  let panel = $state<HTMLElement | null>(null)
  let newGroup = $state(false)
  let groupName = $state('')
  let groupError = $state<string | null>(null)
  let groupEpoch = $state(0)
  let update = $state.raw<AxoExtension | null>(null)
  let confirmUninstall = $state(false)
  let uninstallError = $state<string | null>(null)
  let active = true
  onDestroy(() => {
    active = false
  })

  $effect(() => {
    if (showOutput && panel) panel.scrollIntoView({ block: 'nearest' })
  })

  function installDependencies() {
    if (codeDisabled || transitional || !authored) return
    void controls.installDependencies(item.id, project)
  }

  function chooseGroup(value: string) {
    if (value === '+new') {
      newGroup = true
      groupName = ''
      groupError = null
    } else void controls.setGroup(item.id, project, value === '+own' ? null : value).then(() => groupEpoch++)
  }

  async function saveGroup() {
    const name = groupName.trim()
    if (!/^[a-z0-9][a-z0-9._-]*$/.test(name)) {
      groupError = 'Use lowercase letters, digits, dots, underscores or hyphens; start with a letter or digit.'
      return
    }
    groupError = null
    if (name === item.group || (await controls.setGroup(item.id, project, name))) newGroup = false
  }

  async function remove(deleteData: boolean) {
    if (!confirmUninstall || disabled) return
    const id = item.id
    uninstallError = null
    if (await controls.uninstall(id, deleteData)) {
      if (!active) return
      confirmUninstall = false
      onBack()
    } else uninstallError = controls.error
  }

  const relative = new Intl.RelativeTimeFormat('en', { numeric: 'auto' })
  function age(time: number | null) {
    if (time === null || !Number.isFinite(new Date(time).getTime())) return null
    const minutes = Math.trunc((time - Date.now()) / 60000)
    if (!minutes) return 'just now'
    if (Math.abs(minutes) >= 1440) return relative.format(Math.trunc(minutes / 1440), 'day')
    if (Math.abs(minutes) >= 60) return relative.format(Math.trunc(minutes / 60), 'hour')
    return relative.format(minutes, 'minute')
  }
</script>

<header class="flex flex-col gap-1 pb-6">
  <h2 class="font-mono text-2xl font-semibold text-bright">{item.id}</h2>
  <p class="text-sm whitespace-pre-wrap text-muted">
    {extensionStateText(item, access)}{item.enabled && item.position ? ` · ${item.position} of ${enabled.length}` : ''}
  </p>
  {#if otherMembers.length}<p class="text-sm text-muted">
      shares a runner with {otherMembers.map(({ id }) => id).join(', ')}
    </p>{/if}
  {#if failure && !showOutput}
    <div class="flex items-center gap-2 text-sm text-danger-bright">
      {failure.title}<Button variant="text" size="xs" onclick={() => (showOutput = true)}>View Output</Button>
    </div>
  {/if}
</header>

{#if failure && showOutput}
  <div class="pb-6" bind:this={panel}>
    <Alert.Root variant="danger">
      <h3 class="text-md font-semibold">{failure.title}</h3>
      {#if !item.failure && item.problem}<p class="mt-1 whitespace-pre-wrap">{item.problem.message}</p>{/if}
      {#if failure.output}<pre class="my-2 max-h-48 overflow-auto font-mono whitespace-pre-wrap">{failure.output}</pre>
      {:else}<p class="my-2">No output was captured.</p>{/if}
      <div class="flex gap-2">
        <Button size="xs" disabled={!failure.output} onclick={() => copyToClipboard(failure?.output ?? '')}
          >Copy Output</Button
        >
        {#if installed}<Button size="xs" {disabled} onclick={() => (update = item)}>Try Again</Button>
        {:else if authored && item.enabled}<Button
            size="xs"
            disabled={codeDisabled || transitional}
            onclick={installDependencies}>Try Again</Button
          >{/if}
      </div>
    </Alert.Root>
  </div>
{/if}

<SettingsSection title={project ? 'In This Project' : 'On This Computer'}>
  <SettingRow
    as="div"
    label="Turned on"
    description={installed ? 'Off keeps it installed but out of every chat.' : 'Off keeps it out of chats.'}
  >
    {#if runtime === 'off'}<Button size="xs" {disabled} onclick={() => controls.setEnabled(item.id, project, false)}
        >Turn Off</Button
      >{/if}
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
  </SettingRow>
  <SettingRow
    as="div"
    label="Priority"
    description={`${item.position ? `${item.position} of ${enabled.length}. ` : ''}Later wins when two add the same provider.`}
  >
    <Button
      size="xs"
      disabled={disabled || !item.enabled || (item.position ?? 0) <= 1}
      onclick={() => controls.move(item.id, project, -1)}>Earlier</Button
    >
    <Button
      size="xs"
      disabled={disabled || !item.enabled || (item.position ?? enabled.length) >= enabled.length}
      onclick={() => controls.move(item.id, project, 1)}>Later</Button
    >
  </SettingRow>
  <SettingRow
    as="div"
    label="Runner"
    description="Grouped extensions share one process: less memory, but a crash restarts the group."
  >
    {#key groupEpoch}
      <Select
        aria-label="Runner"
        value={newGroup ? '+new' : (item.group ?? '+own')}
        items={[{ value: '+own', label: 'Own runner' }, ...groups, { value: '+new', label: 'New Group…' }]}
        disabled={disabled || !item.enabled}
        onValueChange={chooseGroup}
      />
    {/key}
  </SettingRow>
  {#if newGroup && item.enabled}
    <SettingRow as="div" label="New Group" stacked>
      <div class="flex items-end gap-2">
        <label class="flex flex-1 flex-col gap-1 text-xs text-muted"
          >Group name
          <Input
            bind:value={groupName}
            {disabled}
            aria-invalid={groupError !== null}
            onkeydown={(event) => {
              if (!event.isComposing && event.key === 'Enter') void saveGroup()
            }}
          />
        </label>
        <Button size="sm" {disabled} onclick={saveGroup}>Save Group</Button>
        <Button
          size="sm"
          variant="text"
          onclick={() => {
            newGroup = false
            groupEpoch++
          }}>Cancel</Button
        >
      </div>
      {#if groupError}<p role="alert" class="text-sm text-danger-bright">{groupError}</p>{/if}
    </SettingRow>
  {/if}
  {#if !extensionMissing(item)}
    <SettingRow
      as="div"
      label="Code changes"
      description="Picks up edits without restarting chats. Calls in progress are cut off."
    >
      <Button size="xs" disabled={codeDisabled} onclick={() => controls.reload(item.id, project)}>Reload Code</Button>
      {#if runtime === 'retry'}
        <Button size="xs" disabled={codeDisabled} onclick={() => controls.retry(item.id, project)}>Retry</Button>
      {/if}
    </SettingRow>
  {/if}
  {#if authored && !extensionMissing(item)}
    <SettingRow as="div" label="Dependencies" description="Runs dependency scripts as your account.">
      <Button size="xs" disabled={codeDisabled || transitional} onclick={installDependencies}
        >Install Dependencies</Button
      >
    </SettingRow>
  {/if}
</SettingsSection>

<SettingsSection
  title="What It Adds"
  description="From its manifest. It can still do more than this, since it runs as your account."
>
  {#each adds.providers as provider (provider.id)}
    {@const loaded = owner?.collisions.some(
      (entry) => entry.winner === item.id && entry.provider === provider.id && entry.hidden.type === 'loaded'
    )}
    <SettingRow
      as="div"
      label="Provider"
      description={`${provider.id}, ${provider.models} ${provider.models === 1 ? 'model' : 'models'}`}
    >
      {#if loaded}<Button size="xs" onclick={() => setSettingsPage('providers')}>Open Providers</Button>{/if}
    </SettingRow>
    {#if loaded}<p class="text-sm text-warning-bright">Hides the existing {provider.id} provider.</p>{/if}
  {/each}
  {#if adds.tools.length}<SettingRow as="div" label="Tools" description={adds.tools.join(', ')} />{/if}
  {#if adds.commands.length}<SettingRow
      as="div"
      label="Commands"
      description={adds.commands.map((name) => `/${name}`).join(', ')}
    />{/if}
  {#if adds.toolPolicy.length}<SettingRow
      as="div"
      label="Tool policy"
      description={adds.toolPolicy.map((when) => `Checks tools ${when} they run`).join(', ')}
    />{/if}
  {#if adds.spawnHook}<SettingRow
      as="div"
      label="Subagent hook"
      description="Checks subagents before they start"
    />{/if}
  {#if !adds.providers.length && !adds.tools.length && !adds.commands.length && !adds.toolPolicy.length && !adds.spawnHook}
    <p class="text-sm text-muted">No providers, tools, commands or hooks declared.</p>
  {/if}
  {#if owner}<ExtensionFindings collisions={owner.collisions} diagnostics={owner.diagnostics} id={item.id} />{/if}
</SettingsSection>

{#if installed && item.install}
  {@const copy = item.install}
  {@const installedAge = age(copy.installedAt)}
  {#snippet version()}
    {#if copy.revision}<span class="font-mono" title={copy.revision}>{shortRevision(copy.revision)}</span>
    {:else}Folder copy, no commit{/if}
    {#if copy.gitRef}
      on <span class="font-mono">{copy.gitRef}</span>{/if}
    {#if installedAge}
      · installed {installedAge}{/if}
  {/snippet}
  <SettingsSection title="Source">
    {#snippet actions()}<Button size="sm" {disabled} onclick={() => (update = item)}>Update…</Button>{/snippet}
    <SettingRow as="div" label="Installed from" description={copy.origin} />
    {#if copy.url}<SettingRow as="div" label="Repository" description={copy.url} />{/if}
    <SettingRow as="div" label="Version" description={version} />
    {#if copy.previous}
      {@const previous = copy.previous}
      {@const previousAge = age(previous.installedAt)}
      {#snippet previousLabel()}
        {#if previous.revision}<span class="font-mono" title={previous.revision}
            >{shortRevision(previous.revision)}</span
          >
        {:else}Folder copy{/if} · kept for rollback{#if previousAge}
          · installed {previousAge}{/if}
      {/snippet}
      <SettingRow as="div" label="Previous copy" description={previousLabel}>
        <Button size="xs" {disabled} onclick={() => controls.rollback(item.id)}>Roll Back</Button>
      </SettingRow>
    {/if}
  </SettingsSection>
  <SettingsSection title="Remove">
    <SettingRow
      as="div"
      label="Uninstall"
      description={`Turns it off, then removes both copies from ${hostLabel}. Its data stays unless you choose to delete it.`}
    >
      <Button
        size="sm"
        variant="destructive"
        {disabled}
        onclick={() => {
          uninstallError = null
          confirmUninstall = true
        }}>Uninstall…</Button
      >
    </SettingRow>
  </SettingsSection>
{/if}

{#if update}
  <ExtensionInstallDialog {host} {hostLabel} state={controls} {update} onClose={() => (update = null)} />
{/if}
{#if confirmUninstall}
  <ExtensionUninstallDialog
    {item}
    {hostLabel}
    busy={controls.busy}
    disabled={controls.stale}
    error={uninstallError}
    onUninstall={remove}
    onClose={() => (confirmUninstall = false)}
  />
{/if}
