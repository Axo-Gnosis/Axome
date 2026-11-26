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
  import { getSettingsPages, loadIntegrationCatalog } from '$lib/features/integrations/catalog.svelte'
  import FileIcon from '$lib/icons/FileIcon.svelte'
  import { Button, IconButton } from '$lib/components/ui/button'
  import { WindowDialog } from '$lib/components/ui/dialog'
  import { ScrollArea } from '$lib/components/ui/scroll-area'
  import { Spinner } from '$lib/components/ui/progress'
  import * as Tooltip from '$lib/components/ui/tooltip'
  import {
    AppWindowIcon,
    BrainIcon,
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
  import { loadSettings } from '$lib/features/settings/state/settings.svelte'
  import { localHostLabel } from '$lib/features/browser/state.svelte'
  import { getErrorMessage } from '$lib/utils/client-error'
  import { hostRoot, splitRemotePath } from '$lib/utils/paths'
  import { platform } from '$lib/platform'
  import { onMount, untrack, type Component } from 'svelte'

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
  import ExtensionsView from '$lib/features/settings/views/ExtensionsView.svelte'
  import MemoryView from '$lib/features/settings/views/MemoryView.svelte'
  import PredictView from '$lib/features/settings/views/PredictView.svelte'
  import ProjectAccessView from '$lib/features/settings/views/ProjectAccessView.svelte'
  import { getSaveStatus } from '../views/autoSaver.svelte'
  import {
    closeExtensionDetail,
    getExtensionContextFolder,
    getExtensionDetail,
    getPageSections,
    getSettingsLoadFolder,
    getSettingsPage,
    isExtensionProjectScroll,
    setSettingsPage
  } from './state.svelte'

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
          id: 'extensions',
          view: ExtensionsView,
          host: true,
          label: 'Extensions',
          description:
            'Packages that add tools, commands and model providers to chats. They run as your account on {host}. Changes also update open root chats.',
          icon: icon(PackageIcon)
        },
        {
          id: 'memory',
          view: MemoryView,
          host: true,
          label: 'Memory',
          description: 'Long-term notes the agent keeps across sessions. Off by default.',
          icon: icon(BrainIcon)
        },
        {
          id: 'predictive-text',
          view: PredictView,
          label: 'Predictive Text',
          description: 'Inline ghost-text completions learned locally on this device.',
          icon: icon(SparklesIcon)
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

  const loadFolder = $derived(getSettingsLoadFolder())
  const server = $derived(loadFolder ? (splitRemotePath(loadFolder)?.server ?? null) : null)
  const host = $derived(hostRoot(loadFolder))
  const hostLabel = $derived(server ?? localHostLabel().toLowerCase())
  let languagePages = $derived(getSettingsPages(host))
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

  const hostSectionServer = $derived(activeItem.host ? server : null)
  const extensionDetail = $derived(activeItem.id === 'extensions' ? getExtensionDetail() : null)
  const extensionFolder = $derived(getExtensionContextFolder())

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
    if (id === 'extensions' && extensionDetail) {
      closeExtensionDetail()
      return
    }
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

  let load = $state<{ folder: string | null; error: string | null } | null>(null)
  let attempt = $state(0)
  $effect(() => {
    if (!open) return
    const folder = host
    void attempt
    let active = true
    load = null
    void untrack(() => loadSettings(folder)).then(
      () => {
        if (active) load = { folder, error: null }
      },
      (error: unknown) => {
        if (active) load = { folder, error: getErrorMessage(error) }
      }
    )
    void untrack(() => loadIntegrationCatalog(folder)).catch(() => {})
    return () => {
      active = false
      load = null
    }
  })

  $effect(() => {
    void extensionDetail?.id
    untrack(() => {
      if (extensionDetail || !isExtensionProjectScroll()) viewport?.scrollTo({ top: 0 })
    })
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
      {#if extensionDetail}
        <div class="flex min-w-0 items-center gap-2 text-md">
          <Button variant="text" size="sm" onclick={closeExtensionDetail}>Extensions</Button>
          <span aria-hidden="true" class="text-muted">/</span>
          <h1 class="truncate font-mono text-bright">{extensionDetail.id}</h1>
        </div>
      {:else}
        <h1 class="truncate text-md font-semibold text-bright">{activeItem.label}</h1>
      {/if}
      <IconButton tooltip="Close" onclick={onClose}><Icon icon={Cancel01Icon} size={14} /></IconButton>
    </header>

    <ScrollArea class="min-h-0 flex-1" bind:viewportRef={viewport}>
      <div bind:this={content} class="mx-auto flex w-full max-w-4xl flex-col px-10 py-10">
        <div class="mb-8 flex flex-col gap-1.5 border-b border-edge pb-6">
          <p class="text-base text-muted">{activeItem.description.replace('{host}', hostLabel)}</p>
          {#if hostSectionServer}
            <p class="text-xs text-subtle">
              These settings live on <span class="font-mono text-fg">{hostSectionServer}</span>
            </p>
          {/if}
        </div>

        <!-- Own container so the view's first section is a first child (no top hairline). -->
        <div class="flex flex-col">
          {#if open}
            {#if activeItem.host && (load === null || load.folder !== host)}
              <p class="flex items-center gap-2 text-sm text-muted"><Spinner /> Loading settings…</p>
            {:else if activeItem.host && load?.error}
              <p role="alert" class="mb-2 text-sm text-danger-bright">{load.error}</p>
              <Button class="self-start" size="sm" onclick={() => attempt++}>Try Again</Button>
            {:else if activeItem.id === 'extensions'}
              {#key JSON.stringify([server, extensionFolder])}
                <ExtensionsView {host} folderPath={extensionFolder} {hostLabel} />
              {/key}
            {:else}
              {#key activeItem.host ? `${host ?? 'local'}/${activeItem.id}` : activeItem.id}
                <activeItem.view {...activeItem.props} />
              {/key}
            {/if}
          {/if}
        </div>
      </div></ScrollArea
    >
  </main>
</WindowDialog>
