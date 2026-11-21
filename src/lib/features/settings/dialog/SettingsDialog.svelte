<!--
  @component
  SettingsDialog — settings in a WindowDialog over the app.

  Grouped sidebar on the left: page entries, and the open page's sections
  (tracked by SettingsSection, highlighted as they scroll past) when the page
  is long enough to scroll. Right: a title bar with Close, then the page as a
  centred column. Esc, a backdrop click or Close dismiss it.

  @param open - whether settings are shown
  @param onClose - dismiss settings
-->

<script lang="ts">
  import { getSettingsPages, loadExtensionCatalog } from '$lib/features/extensions/catalog.svelte'
  import FileIcon from '$lib/icons/FileIcon.svelte'
  import { IconButton } from '$lib/components/ui/button'
  import { WindowDialog } from '$lib/components/ui/dialog'
  import { ScrollArea } from '$lib/components/ui/scroll-area'
  import { Spinner } from '$lib/components/ui/progress'
  import * as Tooltip from '$lib/components/ui/tooltip'
  import {
    AppWindowIcon,
    Cancel01Icon,
    CloudIcon,
    CommandLineIcon,
    KeyboardIcon,
    PackageIcon,
    PaintBoardIcon,
    SaveIcon,
    ServerStack01Icon,
    ShieldKeyIcon,
    SlidersHorizontalIcon,
    SparklesIcon,
    Settings01Icon,
    Wrench01Icon
  } from '@hugeicons/core-free-icons'
  import Icon, { type IconData } from '$lib/icons/Icon.svelte'
  import { getSettingsHost, getSettingsServer, loadSettings } from '$lib/features/settings/state/settings.svelte'
  import { getActiveFolderPath } from '$lib/features/workbench/state.svelte'
  import { platform } from '$lib/platform'
  import { onMount, type Component } from 'svelte'

  import AppearanceView from '$lib/features/settings/views/AppearanceView.svelte'
  import KeyboardShortcutsView from '$lib/features/settings/views/KeyboardShortcutsView.svelte'
  import LanguageSettingsView from '$lib/features/settings/views/LanguageSettingsView.svelte'
  import NixView from '$lib/features/settings/views/NixView.svelte'
  import HarnessesView from '$lib/features/settings/views/HarnessesView.svelte'
  import ShellView from '$lib/features/settings/views/ShellView.svelte'
  import WindowView from '$lib/features/settings/views/WindowView.svelte'
  import RemoteServersView from '$lib/features/settings/views/RemoteServersView.svelte'
  import ProvidersView from '$lib/features/settings/views/ProvidersView.svelte'
  import ModelsView from '$lib/features/settings/views/ModelsView.svelte'
  import BehaviorView from '$lib/features/settings/views/BehaviorView.svelte'
  import ToolsView from '$lib/features/settings/views/ToolsView.svelte'
  import ProjectAccessView from '$lib/features/settings/views/ProjectAccessView.svelte'
  import { getSaveStatus } from '../views/autoSaver.svelte'
  import { getPageSections, getSettingsPage, setSettingsPage } from './state.svelte'

  let { open, onClose }: { open: boolean; onClose: () => void } = $props()

  // NAV //

  type NavIcon = { kind: 'icon'; icon: IconData } | { kind: 'file'; filename: string }
  type NavItem = {
    id: string
    label: string
    description: string
    icon: NavIcon
    native?: true
    host?: true
    view: Component<any>
    props?: Record<string, unknown>
  }
  type NavGroup = { title: string; items: NavItem[] }

  const AXO_NOTE = 'Changes apply to chats started or reopened afterwards.'
  const icon = (data: IconData): NavIcon => ({ kind: 'icon', icon: data })

  const STATIC_GROUPS: NavGroup[] = [
    {
      title: 'App',
      items: [
        {
          id: 'appearance',
          view: AppearanceView,
          label: 'Appearance',
          description: 'Theme, fonts and zoom.',
          icon: icon(PaintBoardIcon)
        },
        {
          id: 'window',
          view: WindowView,
          label: 'Window',
          description: 'Startup, title bar, and how files and folders open from outside Axome.',
          icon: icon(AppWindowIcon),
          native: true
        },
        {
          id: 'keyboard-shortcuts',
          view: KeyboardShortcutsView,
          label: 'Keyboard Shortcuts',
          description: 'Rebind any command.',
          icon: icon(KeyboardIcon)
        },
        {
          id: 'remote-servers',
          view: RemoteServersView,
          label: 'Remote Servers',
          description: 'This device’s identity and the servers it connects to.',
          icon: icon(ServerStack01Icon),
          native: true
        }
      ]
    },
    {
      title: 'Agent',
      items: [
        {
          id: 'providers',
          view: ProvidersView,
          host: true,
          label: 'Providers',
          description: 'Credentials for model providers. Open chats pick up changes right away.',
          icon: icon(CloudIcon)
        },
        {
          id: 'models',
          view: ModelsView,
          host: true,
          label: 'Models',
          description: `Default model, thinking level and model roles. ${AXO_NOTE}`,
          icon: icon(SparklesIcon)
        },
        {
          id: 'behavior',
          view: BehaviorView,
          host: true,
          label: 'Behavior',
          description: `Message queues, context files, keywords, compaction and retries. ${AXO_NOTE}`,
          icon: icon(SlidersHorizontalIcon)
        },
        {
          id: 'tools',
          view: ToolsView,
          host: true,
          label: 'Tools',
          description: `Bash, subagents, web search and docs cache. ${AXO_NOTE}`,
          icon: icon(Wrench01Icon)
        },
        {
          id: 'project-access',
          view: ProjectAccessView,
          host: true,
          label: 'Project Access',
          description: `What agents load and may touch in opened projects. ${AXO_NOTE}`,
          icon: icon(ShieldKeyIcon)
        }
      ]
    },
    {
      title: 'Workspace',
      items: [
        {
          id: 'harnesses',
          view: HarnessesView,
          host: true,
          label: 'Harnesses',
          description: 'Coding-agent CLIs offered in New Session.',
          icon: icon(PackageIcon)
        },
        {
          id: 'shell',
          view: ShellView,
          host: true,
          label: 'Shell',
          description: 'Terminal shell and presets.',
          icon: icon(CommandLineIcon)
        }
      ]
    }
  ]

  let languagePages = $derived(getSettingsPages(getSettingsHost()))
  let groups = $derived<NavGroup[]>([
    ...STATIC_GROUPS.map((group) => ({
      ...group,
      items: group.items.filter((item) => !item.native || platform.native !== null)
    })),
    {
      title: 'Languages',
      items: languagePages.map((definition) => ({
        id: definition.id,
        host: true,
        view: definition.kind === 'nix' ? NixView : LanguageSettingsView,
        props: { definition },
        label: definition.label,
        description: `Formatting and language servers for ${definition.label}.`,
        icon: { kind: 'file', filename: definition.icon_filename }
      }))
    }
  ])
  let items = $derived(groups.flatMap((group) => group.items))

  let activeItem = $derived(items.find((item) => item.id === getSettingsPage()) ?? items[0])

  // Host sections show the loaded owner's values and save to the host captured
  // by each editor, even if the active folder moves before a write flushes.
  let hostSectionServer = $derived(activeItem.host ? getSettingsServer() : null)

  // Host-section editors seed drafts from the loaded owner's values, so they
  // re-mount when ownership moves rather than flushing stale drafts to a new
  // host. The key is stable while the desktop owns the sections.
  let hostKey = $derived(getSettingsHost() ?? 'local')

  // SECTIONS //

  let viewport = $state<HTMLElement | null>(null)
  let sections = $derived(getPageSections())
  let currentSection = $state<string | null>(null)
  // A clicked sub-entry stays current while its smooth scroll runs, even when
  // the page bottoms out before that section reaches the top.
  let pinnedUntil = 0
  let content = $state<HTMLElement | null>(null)
  // Sub-entries only help when there is something to scroll to.
  let overflowing = $state(false)

  // Scroll-spy: the last section whose top has passed the viewport's top
  // edge, or the last one once the page is scrolled to the bottom.
  function spy() {
    if (!viewport || sections.length === 0 || performance.now() < pinnedUntil) return
    const edge = viewport.getBoundingClientRect().top + 48
    let current = sections[0].id
    for (const section of sections) {
      if (section.element.getBoundingClientRect().top <= edge) current = section.id
    }
    if (overflowing && viewport.scrollTop + viewport.clientHeight >= viewport.scrollHeight - 2) {
      current = sections.at(-1)!.id
    }
    currentSection = current
  }

  $effect(() => {
    const element = viewport
    if (!element || !content) return
    const measure = () => {
      overflowing = element.scrollHeight > element.clientHeight + 1
      spy()
    }
    const observer = new ResizeObserver(measure)
    observer.observe(element)
    observer.observe(content)
    element.addEventListener('scroll', spy, { passive: true })
    return () => {
      observer.disconnect()
      element.removeEventListener('scroll', spy)
    }
  })

  $effect(() => {
    void sections
    spy()
  })

  function openPage(id: string) {
    setSettingsPage(id)
    viewport?.scrollTo({ top: 0 })
  }

  function jumpTo(id: string) {
    const section = sections.find((entry) => entry.id === id)
    if (!section) return
    currentSection = id
    pinnedUntil = performance.now() + 800
    section.element.scrollIntoView({ behavior: 'smooth', block: 'start' })
  }

  // SAVE STATUS //

  let saveStatus = $derived(getSaveStatus())
  let saveTooltip = $derived(
    saveStatus === 'saving'
      ? 'Saving changes…'
      : saveStatus === 'saved'
        ? 'All changes saved'
        : 'Changes autosave 400ms after you stop typing.'
  )

  // SETTINGS LOAD //

  // Host-section reads probe harness binaries on the owning machine, so load
  // only while open. Reloading on every active-folder change keeps the
  // values, the caption and the file a save lands in on one host.
  $effect(() => {
    if (!open) return
    void loadSettings(getActiveFolderPath())
    void loadExtensionCatalog(getActiveFolderPath()).catch(() => {})
  })

  // VERSION //

  let version = $state<string | null>(null)

  onMount(() => {
    void platform.app
      .version()
      .then((v) => (version = v.split('+')[1] ?? (v === '0.0.0' ? 'dev' : v)))
      .catch(() => {})
  })
</script>

<WindowDialog
  {open}
  onOpenChange={(next) => {
    if (!next) onClose()
  }}
  label="Settings"
  onModalClose={onClose}
>
  <aside class="flex w-60 shrink-0 flex-col border-r border-edge bg-surface">
    <header class="flex h-12 shrink-0 items-center gap-2 border-b border-edge px-4 text-md font-semibold text-bright">
      <Icon icon={Settings01Icon} size={14} class="text-accent" />
      Settings
    </header>
    <nav class="flex min-h-0 flex-1 flex-col gap-0.5 overflow-y-auto px-2 pb-3" aria-label="Settings pages">
      {#each groups as group (group.title)}
        {#if group.items.length}
          <div
            class="mt-3 border-edge px-2 pt-3 pb-1 text-xs font-medium tracking-wide text-subtle uppercase not-first:border-t"
          >
            {group.title}
          </div>
          {#each group.items as item (item.id)}
            {@const selected = item.id === activeItem.id}
            <button
              type="button"
              aria-current={selected ? 'page' : undefined}
              class="flex w-full cursor-pointer items-center gap-2.5 rounded-md px-2 py-1.5 text-left text-base {selected
                ? 'bg-accent-bg text-bright [&_svg]:text-accent'
                : 'text-muted hover:bg-raised hover:text-fg'}"
              onclick={() => openPage(item.id)}
            >
              {#if item.icon.kind === 'icon'}
                <Icon icon={item.icon.icon} size={14} class="shrink-0" />
              {:else}
                <FileIcon filename={item.icon.filename} size={14} />
              {/if}
              <span class="truncate">{item.label}</span>
            </button>
            {#if selected && overflowing && sections.length > 1}
              <ul class="my-1 ml-4 border-l border-edge">
                {#each sections as section (section.id)}
                  {@const current = section.id === currentSection}
                  <li>
                    <button
                      type="button"
                      class="-ml-px w-full cursor-pointer border-l py-1 pr-2 pl-3 text-left text-sm
                             {current ? 'border-accent text-bright' : 'border-transparent text-muted hover:text-fg'}"
                      onclick={() => jumpTo(section.id)}>{section.title}</button
                    >
                  </li>
                {/each}
              </ul>
            {/if}
          {/each}
        {/if}
      {/each}
    </nav>

    <footer class="flex h-10 shrink-0 items-center justify-between border-t border-edge px-4">
      <span class="font-mono text-xs text-subtle"
        >{#if version}Axome {version}{/if}</span
      >
      <Tooltip.Root>
        <Tooltip.Trigger class="flex items-center rounded-md px-1.5 py-0.5" aria-label={saveTooltip}>
          {#if saveStatus === 'saving'}
            <Spinner size={12} class="text-muted" />
          {:else}
            <Icon icon={SaveIcon} size={12} class={saveStatus === 'saved' ? 'text-success' : 'text-subtle'} />
          {/if}
        </Tooltip.Trigger>
        <Tooltip.Content side="top">{saveTooltip}</Tooltip.Content>
      </Tooltip.Root>
    </footer>
  </aside>

  <main class="flex min-w-0 flex-1 flex-col">
    <header class="flex h-12 shrink-0 items-center justify-between border-b border-edge px-5">
      <h1 class="truncate text-md font-semibold text-bright">{activeItem.label}</h1>
      <IconButton tooltip="Close" onclick={onClose}><Icon icon={Cancel01Icon} size={14} /></IconButton>
    </header>

    <ScrollArea class="min-h-0 flex-1" bind:viewportRef={viewport}>
      <div bind:this={content} class="mx-auto flex w-full max-w-4xl flex-col px-10 py-10">
        <div class="mb-8 flex flex-col gap-1.5 border-b border-edge pb-6">
          <p class="text-base text-muted">{activeItem.description}</p>
          {#if hostSectionServer}
            <p class="text-xs text-subtle">
              These settings live on <span class="font-mono text-fg">{hostSectionServer}</span>
            </p>
          {/if}
        </div>

        <!-- Own container so the view's first section is a first child (no top hairline). -->
        <div class="flex flex-col">
          {#key activeItem.host ? `${hostKey}/${activeItem.id}` : activeItem.id}
            <activeItem.view {...activeItem.props} />
          {/key}
        </div>
      </div></ScrollArea
    >
  </main>
</WindowDialog>
