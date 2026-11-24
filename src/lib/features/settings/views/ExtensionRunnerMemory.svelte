<script lang="ts">
  import { onMount } from 'svelte'
  import { backend } from '$lib/api/backend'
  import { Button } from '$lib/components/ui/button'
  import { Spinner } from '$lib/components/ui/progress'
  import type { AxoExtension, AxoExtensionPhase, AxoExtensions, AxoRunnerMemory } from '$lib/types/backend'
  import { getErrorMessage } from '$lib/utils/client-error'
  import { extensionStateText } from '../state/extensions.svelte'
  import SettingsSection from '../dialog/SettingsSection.svelte'

  let {
    host,
    scopeFolder,
    catalog,
    active
  }: {
    host: string | null
    scopeFolder: string | null
    catalog: AxoExtensions | null
    active: boolean
  } = $props()

  let expanded = $state(false)
  let visible = $state(false)
  let sample = $state.raw<AxoRunnerMemory[] | null>(null)
  let error = $state<string | null>(null)
  let loading = $state(false)
  let opening = true
  let inFlight: Promise<AxoRunnerMemory[]> | null = null
  const precedence: AxoExtensionPhase[] = [
    'unavailable',
    'disabled',
    'restarting',
    'installing',
    'starting',
    'ready',
    'cold'
  ]

  onMount(() => {
    const visibilityChanged = () => {
      visible = document.visibilityState === 'visible'
    }
    visibilityChanged()
    document.addEventListener('visibilitychange', visibilityChanged)
    return () => document.removeEventListener('visibilitychange', visibilityChanged)
  })

  $effect(() => {
    const live = expanded
    const owner = host
    const scope = scopeFolder
    if (!active || !visible || (!live && !opening)) return
    let valid = true
    let timer: ReturnType<typeof setTimeout> | undefined

    async function refresh() {
      loading = true
      // A cancelled loop may still be waiting on the host. Never overlap its sample.
      if (inFlight) await inFlight.catch(() => {})
      if (!valid) return
      const request = backend.axo.extensionsMemory(scope, owner)
      inFlight = request
      try {
        const next = await request
        if (valid) {
          sample = next
          error = null
        }
      } catch (cause) {
        if (valid) error = getErrorMessage(cause)
      } finally {
        if (inFlight === request) inFlight = null
        if (valid) {
          opening = false
          loading = false
          if (live) timer = setTimeout(refresh, 2000)
        }
      }
    }

    void refresh()
    return () => {
      valid = false
      loading = false
      clearTimeout(timer)
    }
  })

  const mib = (bytes: number) => `${(bytes / 1048576).toFixed(1)} MiB`
  const restricted = $derived.by(() => {
    const groups = new Map<string, { label: string; members: string[]; own: boolean }>()
    const project = catalog?.project
    if (!project || project.access === 'trust') return []
    for (const item of project.items) {
      if (!item.enabled) continue
      const key = item.group === null ? `id:${item.id}` : `group:${item.group}`
      const row = groups.get(key)
      if (row) row.members.push(item.id)
      else
        groups.set(key, {
          label: item.group === null ? item.id : `Group ${item.group}`,
          members: [item.id],
          own: item.group === null
        })
    }
    return [...groups.values()]
  })

  function phaseText(row: AxoRunnerMemory) {
    const list = row.project ? catalog?.project?.items : catalog?.host
    const matches = row.members
      .map((id) => list?.find((item) => item.id === id))
      .filter((item): item is AxoExtension => !!item?.phase)
    const phase = precedence.find((phase) => matches.some((item) => item.phase === phase))
    const item = matches.find((item) => item.phase === phase)
    if (!item) return 'State unavailable'
    if (phase === 'installing' && !item.problem) return 'Installing dependencies'
    return extensionStateText(item, row.project ? (catalog?.project?.access ?? null) : null)
  }
</script>

{#if active}
  <SettingsSection
    title="Runner Memory"
    description={expanded
      ? 'Live while this list is open, refreshed every 2 seconds. Grouped extensions share one runner, so they share one number.'
      : 'Sampled once when this page opens. Grouped extensions share one runner and one number.'}
  >
    {#snippet actions()}
      <Button size="sm" aria-expanded={expanded} onclick={() => (expanded = !expanded)}
        >{expanded ? 'Hide Runners' : 'Show Runners'}</Button
      >
    {/snippet}
    <div class="flex items-center gap-2 text-sm text-muted">
      {#if loading || (sample === null && !error)}<Spinner label="Sampling runner memory" />{/if}
      {#if sample}
        {#if error}Last sample ·
        {/if}{sample.length}
        {sample.length === 1 ? 'runner' : 'runners'} · {mib(sample.reduce((sum, row) => sum + row.pssBytes, 0))}
      {:else}—{/if}
    </div>
    {#if error}<p role="alert" class="text-sm text-danger-bright">{error}</p>{/if}
    {#if expanded}
      {#if sample?.length || restricted.length}
        <ul class="divide-y divide-edge">
          {#each sample ?? [] as row}
            <li class="flex items-center justify-between gap-4 py-2 text-sm">
              <div class="min-w-0">
                <p class="font-mono break-words">{row.group === null ? row.members[0] : `Group ${row.group}`}</p>
                <p class="text-muted">
                  {row.group === null ? 'Own runner' : row.members.join(', ')}{row.project ? ' · this project' : ''}
                </p>
              </div>
              <p class="shrink-0 text-right text-muted">
                {phaseText(row)} · <span class="font-mono text-fg">{mib(row.pssBytes)}</span>
              </p>
            </li>
          {/each}
          {#each restricted as row}
            <li class="flex items-center justify-between gap-4 py-2 text-sm">
              <div class="min-w-0">
                <p class="font-mono break-words">{row.label}</p>
                <p class="text-muted">{row.own ? 'Own runner' : row.members.join(', ')} · this project</p>
              </div>
              <p class="shrink-0 text-muted">Waiting for trust · —</p>
            </li>
          {/each}
        </ul>
      {:else if sample}<p class="text-sm text-muted">No runners are active.</p>{/if}
    {/if}
    <p class="text-xs text-muted">
      Off extensions are not listed. Extensions that start when first used have no runner until then. Memory is PSS:
      what the process really uses, with shared libraries split between the processes that load them.
    </p>
  </SettingsSection>
{/if}
