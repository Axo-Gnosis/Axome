<script lang="ts">
  import { untrack } from 'svelte'
  import { backend } from '$lib/api/backend'
  import * as Alert from '$lib/components/ui/alert'
  import { Button } from '$lib/components/ui/button'
  import * as Dialog from '$lib/components/ui/dialog'
  import { Input } from '$lib/components/ui/input'
  import { Spinner } from '$lib/components/ui/progress'
  import * as Tabs from '$lib/components/ui/tabs'
  import { selectFolder } from '$lib/features/browser/state.svelte'
  import { notify } from '$lib/features/notifications/state.svelte'
  import { platform } from '$lib/platform'
  import type {
    AxoExtension,
    AxoExtensionFailure,
    AxoExtensionSource,
    AxoExtensionStage,
    AxoInstallOutcome
  } from '$lib/types/backend'
  import { getErrorMessage } from '$lib/utils/client-error'
  import { copyToClipboard } from '$lib/utils/clipboard'
  import { splitRemotePath } from '$lib/utils/paths'
  import { extensionAddsText, shortRevision } from '../state/extensions.svelte'

  let {
    open,
    host,
    hostLabel,
    update,
    bunUnavailable,
    busy,
    actionError,
    onInstalled,
    onTurnOn,
    onClose,
    run
  }: {
    open: boolean
    host: string | null
    hostLabel: string
    update: AxoExtension | null
    bunUnavailable: boolean
    busy: boolean
    actionError: string | null
    onInstalled: (extension: AxoExtension) => Promise<void>
    onTurnOn: (id: string) => Promise<boolean>
    onClose: () => void
    run: (action: () => Promise<void>) => Promise<boolean>
  } = $props()

  let tab = $state<'folder' | 'git'>('folder')
  let folder = $state('')
  let url = $state('')
  let gitRef = $state('')
  let error = $state<string | null>(null)
  let inspecting = $state(false)
  let browsing = $state(false)
  let committing = $state(false)
  let stage = $state.raw<{ value: AxoExtensionStage; host: string | null } | null>(null)
  let replacement = $state.raw<{ id: string; installed: string; offered: string } | null>(null)
  let failure = $state.raw<AxoExtensionFailure | null>(null)
  let installed = $state<string | null>(null)
  let inspectedSource: AxoExtensionSource | null = null
  let generation = 0
  const localBrowser = $derived(host === null ? platform.native : null)

  function discard(stageId: string, owner: string | null) {
    void backend.axo
      .discardExtensionStage(stageId, owner)
      .catch((cause) => notify.error('Could not discard extension inspection', getErrorMessage(cause)))
  }

  function invalidateInspection() {
    generation++
    inspecting = false
    if (stage) discard(stage.value.stageId, stage.host)
    stage = null
    replacement = null
    failure = null
    error = null
  }

  function close() {
    if (committing) return
    invalidateInspection()
    onClose()
  }

  async function inspect(source?: AxoExtensionSource) {
    if (committing) return
    invalidateInspection()
    const path = splitRemotePath(folder.trim())?.path ?? folder.trim()
    if (!source && tab === 'folder' && !path.startsWith('/')) {
      error = `Enter an absolute folder path on ${hostLabel}.`
      return
    }
    if (!source && tab === 'git' && !url.trim()) {
      error = 'Enter the repository URL.'
      return
    }
    const owner = host
    const expected = update?.id
    const current = generation
    inspectedSource =
      source ??
      (tab === 'folder' ? { type: 'folder', path } : { type: 'git', url: url.trim(), gitRef: gitRef.trim() || null })
    inspecting = true
    try {
      const value = await backend.axo.inspectExtension(inspectedSource, owner)
      if (current !== generation) return discard(value.stageId, owner)
      if (expected && value.id !== expected) {
        discard(value.stageId, owner)
        error = `Expected ${expected}, but this package is ${value.id}.`
        return
      }
      stage = { value, host: owner }
    } catch (cause) {
      if (current === generation) error = getErrorMessage(cause)
    } finally {
      if (current === generation) inspecting = false
    }
  }

  $effect(() => {
    if (!open) return
    const copy = update?.install
    void host
    untrack(() => {
      invalidateInspection()
      installed = null
      tab = copy?.url ? 'git' : 'folder'
      folder = copy?.origin.startsWith('folder:') ? copy.origin.slice('folder:'.length) : ''
      url = copy?.url ?? ''
      gitRef = copy?.gitRef ?? ''
      if (copy?.url) void inspect({ type: 'git', url: copy.url, gitRef: copy.gitRef })
    })
    return () => untrack(invalidateInspection)
  })

  async function browse() {
    const current = generation
    browsing = true
    try {
      const server = host ? splitRemotePath(host)?.server : undefined
      const path = localBrowser
        ? await localBrowser.dialogs.selectDirectory()
        : await backend.folders
            .home(server)
            .then((home) => (current === generation ? selectFolder({ path: home }) : null))
      if (path && current === generation) {
        invalidateInspection()
        folder = splitRemotePath(path)?.path ?? path
      }
    } catch (cause) {
      if (current === generation) error = getErrorMessage(cause)
    } finally {
      browsing = false
    }
  }

  async function commit(replaceOrigin: boolean) {
    if (!stage?.value.compatible || bunUnavailable || busy || committing) return
    const { value, host: owner } = stage
    const current = generation
    const updating = update !== null
    stage = null // The in-flight commit owns cleanup, including if the page goes away.
    committing = true
    error = null
    const result: { outcome?: AxoInstallOutcome; error?: string } = {}
    try {
      const ok = await run(async () => {
        try {
          result.outcome = await backend.axo.installExtension(value.stageId, replaceOrigin, owner)
        } catch (cause) {
          result.error = getErrorMessage(cause)
          throw cause
        }
      })
      if (!ok || !result.outcome) {
        discard(value.stageId, owner)
        if (current === generation) {
          replacement = null
          error = result.error ?? 'Installation could not be confirmed. Inspect the source again.'
        }
        return
      }
      const outcome = result.outcome
      if (current !== generation) {
        if (outcome.type === 'originChanged') discard(value.stageId, owner)
        return
      }
      if (outcome.type === 'originChanged') {
        stage = { value, host: owner }
        replacement = { id: value.id, installed: outcome.installed, offered: outcome.offered }
        return
      }
      replacement = null
      if (outcome.type === 'failed') {
        failure = { title: outcome.title, output: outcome.output }
        return
      }
      await onInstalled(outcome.extension)
      if (current === generation) {
        if (updating) onClose()
        else installed = outcome.extension.id
      }
    } finally {
      committing = false
    }
  }

  async function turnOn(id: string) {
    if (busy || committing) return
    committing = true
    try {
      if (await onTurnOn(id)) onClose()
    } finally {
      committing = false
    }
  }
</script>

<Dialog.Root
  {open}
  onOpenChange={(value) => {
    if (!value) close()
  }}
>
  <Dialog.Content
    class="max-h-[85dvh] overflow-y-auto"
    onModalClose={browsing ? undefined : close}
    aria-busy={committing}
    escapeKeydownBehavior={committing ? 'ignore' : 'close'}
    interactOutsideBehavior={committing || browsing ? 'ignore' : 'close'}
  >
    <Dialog.Header>
      <Dialog.Title>
        {#if installed}{installed} is installed
        {:else if replacement}Update from a different source?
        {:else if update}Update {update.id}
        {:else}Install Extension{/if}
      </Dialog.Title>
      <Dialog.Description>
        {#if installed}It's off for now. Turning it on adds it at the bottom of the list; you can move it up after.
        {:else if replacement}This copy of {replacement.id} came from somewhere else. Only continue if you expect the move.
        {:else if update}Replaces the installed copy on {hostLabel}. The current copy is kept for rollback.
        {:else}Copies a package onto {hostLabel}. It stays off until you turn it on.{/if}
      </Dialog.Description>
    </Dialog.Header>

    {#if installed}
      {#if actionError}<p role="alert" class="text-sm text-danger-bright">{actionError}</p>{/if}
      <Dialog.Footer>
        <Button size="sm" disabled={committing} onclick={close}>Not Now</Button>
        <Button size="sm" variant="accent" disabled={busy || committing} onclick={() => installed && turnOn(installed)}
          >Turn On</Button
        >
      </Dialog.Footer>
    {:else if replacement}
      <div class="flex flex-col gap-2 text-sm text-muted">
        <p>Installed <span class="font-mono break-all text-fg">{replacement.installed}</span></p>
        <p>New <span class="font-mono break-all text-fg">{replacement.offered}</span></p>
        <p>Installing runs the package's dependency scripts now, as your account.</p>
      </div>
      <Dialog.Footer>
        <Button size="sm" disabled={committing} onclick={close}>Cancel</Button>
        <Button size="sm" variant="accent" disabled={committing || busy || bunUnavailable} onclick={() => commit(true)}>
          Update from New Source
        </Button>
      </Dialog.Footer>
    {:else if failure}
      <Alert.Root variant="danger">
        <p class="font-semibold">{failure.title}</p>
        {#if failure.output}<pre
            class="mt-2 max-h-48 overflow-auto font-mono whitespace-pre-wrap">{failure.output}</pre>
        {:else}<p>No output was captured.</p>{/if}
      </Alert.Root>
      <Dialog.Footer>
        <Button size="sm" disabled={!failure.output} onclick={() => copyToClipboard(failure?.output ?? '')}
          >Copy Output</Button
        >
        <Button size="sm" onclick={close}>Cancel</Button>
        <Button size="sm" variant="accent" onclick={() => inspect(inspectedSource ?? undefined)}>Try Again</Button>
      </Dialog.Footer>
    {:else}
      <Tabs.Root
        value={tab}
        onValueChange={(value) => {
          if (committing || browsing || (value !== 'folder' && value !== 'git')) return
          tab = value
          invalidateInspection()
        }}
      >
        <div inert={committing || browsing}>
          <Tabs.List>
            <Tabs.Trigger value="folder">Folder on {hostLabel}</Tabs.Trigger>
            <Tabs.Trigger value="git">Git repository</Tabs.Trigger>
          </Tabs.List>
        </div>
        {#if tab === 'folder'}
          <div class="mt-3 space-y-2">
            <div class="flex items-end gap-2">
              <label class="flex min-w-0 flex-1 flex-col gap-1 text-sm"
                >Folder
                <Input
                  value={folder}
                  disabled={committing || browsing}
                  oninput={(event) => {
                    invalidateInspection()
                    folder = event.currentTarget.value
                  }}
                />
              </label>
              <Button size="sm" disabled={committing || browsing} onclick={browse}>Browse…</Button>
            </div>
            {#if !localBrowser}<p class="text-xs text-muted">
                Choose a folder on {hostLabel}, not this browser's computer.
              </p>{/if}
            <p class="text-xs text-muted">
              Editing an extension yourself? Keep it in <span class="font-mono">~/.config/axome/extensions</span> instead.
              Axome runs it from there without copying.
            </p>
          </div>
        {:else}
          <div class="mt-3 space-y-2">
            <label class="flex flex-col gap-1 text-sm"
              >Repository
              <Input
                value={url}
                disabled={committing}
                oninput={(event) => {
                  invalidateInspection()
                  url = event.currentTarget.value
                }}
              />
            </label>
            <label class="flex flex-col gap-1 text-sm"
              >Branch or commit
              <Input
                value={gitRef}
                placeholder="Default branch"
                disabled={committing}
                oninput={(event) => {
                  invalidateInspection()
                  gitRef = event.currentTarget.value
                }}
              />
            </label>
          </div>
        {/if}
      </Tabs.Root>
      {#if update && !update.install?.url && !update.install?.origin.startsWith('folder:')}
        <p class="mt-2 text-sm text-muted">Enter the repository URL or folder to update from.</p>
      {/if}
      {#if stage}
        {@const value = stage.value}
        <dl class="mt-3 grid grid-cols-[auto_1fr] gap-x-3 gap-y-1 border-t border-edge pt-3 text-sm">
          <dt class="text-muted">Package</dt>
          <dd class="font-mono">{value.id}</dd>
          <dt class="text-muted">SDK</dt>
          <dd>{value.compatible ? 'Compatible' : `Incompatible SDK · Declared SDK: ${value.sdk ?? 'not declared'}`}</dd>
          {#if value.revision}
            <dt class="text-muted">Commit</dt>
            <dd>
              <span class="font-mono" title={value.revision}>{shortRevision(value.revision)}</span>{#if value.subject}
                · {value.subject}{/if}
            </dd>
          {:else}<dt class="text-muted">Version</dt>
            <dd>Folder copy, no commit</dd>{/if}
          <dt class="text-muted">Adds</dt>
          <dd>{extensionAddsText(value.adds) || 'No providers, tools, commands or hooks declared.'}</dd>
          <dt class="text-muted">Installs on</dt>
          <dd>{hostLabel}</dd>
        </dl>
      {/if}
      <p class="mt-3 text-xs text-muted">Installing runs the package's dependency scripts now, as your account.</p>
      {#if error}<p role="alert" class="mt-2 text-sm text-danger-bright">{error}</p>{/if}
      <Dialog.Footer>
        <Button size="sm" disabled={committing || browsing} onclick={close}>Cancel</Button>
        <Button size="sm" disabled={committing || browsing || inspecting} onclick={() => inspect()}>
          {#if inspecting}<Spinner />{/if}Inspect
        </Button>
        <Button
          size="sm"
          variant="accent"
          disabled={committing || browsing || busy || bunUnavailable || !stage?.value.compatible}
          onclick={() => commit(false)}>{update ? 'Update' : 'Install'}</Button
        >
      </Dialog.Footer>
    {/if}
  </Dialog.Content>
</Dialog.Root>
