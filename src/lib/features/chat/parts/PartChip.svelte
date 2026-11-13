<script lang="ts">
  import { File01Icon, Folder01Icon, Image01Icon } from '@hugeicons/core-free-icons'
  import Icon from '$lib/icons/Icon.svelte'
  import { Button } from '$lib/components/ui/button'
  import { cn } from '$lib/utils/cn'
  import { chipVariants } from '$lib/components/ui/chip'
  import type { ChatPart } from '$lib/types/backend'
  import { backend } from '$lib/api/backend'
  import { platform, type AssetHandle } from '$lib/platform'
  import { basename, dirname, splitRemotePath } from '$lib/utils/paths'
  import {
    openFileLocation,
    openLink as openChatLink,
    resolveFileLocationPath
  } from '$lib/features/workbench/links/openLink'
  import { isSchemeUrl } from '$lib/features/workbench/links/parseLink'
  import { imageMime, isImagePart, partLabel } from './parts'

  let {
    part,
    owner,
    interactive = false
  }: {
    part: ChatPart
    owner: { folderPath: string; sessionId: string | null }
    interactive?: boolean
  } = $props()
  let expanded = $state(false)
  let thumbnail = $state<string | null>(null)
  let imageError = $state(false)
  const label = $derived(partLabel(part))
  const icon = $derived(
    isImagePart(part) ? Image01Icon : part.type === 'file' && part.path.endsWith('/') ? Folder01Icon : File01Icon
  )
  const lines = $derived(part.type === 'paste' ? part.text.split('\n').length : 0)
  const chipClass = cn(chipVariants(), 'in-data-selected:bg-selection in-data-selected:text-bright')
  const inlineThumbnail = $derived(
    part.type === 'content' && isImagePart(part) ? `data:${imageMime(part)};base64,${part.bytes}` : null
  )
  const savedImage = $derived(part.type === 'saved' && part.image ? part.url : null)
  const shown = $derived(inlineThumbnail ?? thumbnail)

  $effect(() => {
    const url = savedImage
    const { folderPath, sessionId } = owner
    thumbnail = null
    imageError = false
    if (!url || !sessionId) return
    const controller = new AbortController()
    let handle: AssetHandle | null = null
    void (async () => {
      const target = await backend.chat.resolveLink(folderPath, sessionId, url)
      if (target.kind !== 'file' || controller.signal.aborted) return
      const path = await resolveFileLocationPath(target.path, folderPath)
      const assets = platform.assets
      if ('load' in assets) {
        const loaded = await assets.load(dirname(path), basename(path), controller.signal)
        if (controller.signal.aborted) return loaded.dispose()
        handle = loaded
        thumbnail = loaded.url
      } else if (!controller.signal.aborted && !splitRemotePath(path)) {
        thumbnail = assets.url(dirname(path), basename(path))
      }
    })().catch(() => {
      if (!controller.signal.aborted) imageError = true
    })
    return () => {
      controller.abort()
      handle?.dispose()
    }
  })

  function activate() {
    if (part.type === 'paste') expanded = !expanded
    else if (part.type === 'file')
      void (isSchemeUrl(part.path)
        ? openChatLink(part.path, owner)
        : openFileLocation(part.path, null, owner.folderPath))
    else if (part.type === 'saved') void openChatLink(part.url, owner)
  }
</script>

<!-- Sits in the text flow like one wide character: inline, baseline-aligned, never taller than the
     line, wrapping as a unit. `in-data-selected` lights it up while a text selection covers it. -->
{#snippet content()}
  {#if shown && !imageError}
    <img
      src={shown}
      alt=""
      class="me-1 inline-block size-4 rounded-sm object-cover align-text-bottom"
      onerror={() => (imageError = true)}
    />
  {:else}
    <Icon {icon} size={12} class="me-1 inline-block align-middle" />
  {/if}{label}{#if lines}<span class="ms-1 text-subtle">{lines} lines</span>{/if}
{/snippet}

{#if interactive}
  <Button
    data-slot="part-chip"
    variant="ghost"
    title={label}
    onclick={activate}
    aria-expanded={part.type === 'paste' ? expanded : undefined}
    class={cn(chipClass, 'h-auto gap-0 py-0 font-normal hover:bg-overlay hover:text-bright')}
    >{@render content()}</Button
  >
{:else}
  <span data-slot="part-chip" class={chipClass} title={label}>{@render content()}</span>
{/if}<!-- No whitespace here: it would render as a stray space after the chip.
-->{#if interactive && expanded && part.type === 'paste'}
  <span class="my-1 block rounded border border-edge bg-surface p-2 font-mono text-sm whitespace-pre-wrap"
    >{part.text}</span
  >
{/if}
