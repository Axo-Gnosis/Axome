<!--
  @component
  AppMenu — TEMPORARY WORKAROUND, to be removed.

  The hamburger above the Settings button holds the old app menu that used to
  open from the Settings icon, so that icon can go straight to Settings. Most
  entries already live in the command palette; remove this component once the rest have a proper home. The settings-issue details
  listed at the top need a new home too (the Settings button carries only the
  count).
-->

<script lang="ts">
  import * as DropdownMenu from '$lib/components/ui/dropdown-menu'
  import { TriangleAlertIcon, Menu01Icon } from '@hugeicons/core-free-icons'
  import Icon from '$lib/icons/Icon.svelte'
  import { getSettingsDiagnostics, getSettingsIssueStatus } from '$lib/features/settings/state/diagnostics.svelte'
  import RailButton from '../RailButton.svelte'
  import { getAppCommandDefinitions } from '$lib/features/command-palette/commands/registry.svelte'
  import { getEffectiveBindings } from '$lib/features/command-palette/shortcuts/overrides.svelte'
  import { formatShortcut } from '$lib/features/command-palette/shortcuts/spec'

  const MENU_ORDER = [
    'new-window',
    'reopen-closed-tab',
    'close-workbench',
    null,
    'reveal-in-file-manager',
    'open-in-external-terminal',
    'open-folder-settings',
    null,
    'toggle-sidebar',
    'zoom-in',
    'zoom-out',
    'zoom-reset'
  ] as const

  const MENU_LABELS: Partial<Record<string, string>> = {
    'reveal-in-file-manager': 'Reveal Folder in File Manager',
    'open-in-external-terminal': 'Open Folder in External Terminal',
    'open-folder-settings': 'Folder Settings…'
  }

  const SHOWN_DIAGNOSTICS = 5

  let open = $state(false)
  const definitions = $derived(new Map(getAppCommandDefinitions().map((d) => [d.id, d])))
  let diagnostics = $derived(getSettingsDiagnostics())
  let status = $derived(getSettingsIssueStatus())
</script>

<DropdownMenu.Root bind:open>
  <DropdownMenu.Trigger>
    {#snippet child({ props })}
      <RailButton {...props} label="Menu" active={open}><Icon icon={Menu01Icon} size={16} /></RailButton>
    {/snippet}
  </DropdownMenu.Trigger>

  <DropdownMenu.Content side="right" align="end" sideOffset={6}>
    {#if status}
      <div class="max-w-80 px-2.5 py-1.5">
        <div class="flex items-center gap-1.5 text-xs font-medium text-warning-bright">
          <Icon icon={TriangleAlertIcon} size={11} />
          {status.text}
        </div>
        {#each diagnostics.slice(0, SHOWN_DIAGNOSTICS) as diagnostic}
          <div class="mt-1 font-mono text-2xs break-all text-muted">
            {diagnostic.origin === 'host' ? 'host ' : ''}{diagnostic.layer}: {diagnostic.path}{diagnostic.pointer
              ? ` ${diagnostic.pointer}`
              : ''}
            <span class="font-sans text-warning-bright">{diagnostic.message}</span>
          </div>
        {/each}
        {#if diagnostics.length > SHOWN_DIAGNOSTICS}
          <div class="mt-1 text-2xs text-subtle">+{diagnostics.length - SHOWN_DIAGNOSTICS} more</div>
        {/if}
      </div>
      <DropdownMenu.Separator />
    {/if}
    {#each MENU_ORDER as id, i (i)}
      {#if id === null}
        <DropdownMenu.Separator />
      {:else}
        {@const definition = definitions.get(id)}
        {#if definition}
          {@const shortcut =
            id === 'new-window'
              ? formatShortcut(getEffectiveBindings(id, definition.defaultKeybindings ?? [])[0])
              : undefined}
          <DropdownMenu.Item onSelect={() => void definition.run()} disabled={!(definition.visible?.() ?? true)}>
            <span
              >{MENU_LABELS[id] ??
                (typeof definition.label === 'function' ? definition.label() : definition.label)}</span
            >
            {#if shortcut}<DropdownMenu.Shortcut>{shortcut}</DropdownMenu.Shortcut>{/if}
          </DropdownMenu.Item>
        {/if}
      {/if}
    {/each}
  </DropdownMenu.Content>
</DropdownMenu.Root>
