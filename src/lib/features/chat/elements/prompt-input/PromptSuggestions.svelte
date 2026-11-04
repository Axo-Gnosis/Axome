<script lang="ts" module>
  import type { IconData } from '$lib/icons/Icon.svelte'

  /** One row of the composer's suggestion panel; consecutive rows sharing `group` sit under one heading. */
  export type Suggestion = {
    key: string
    group: string
    label: string
    icon?: IconData
    /** Field names shown as pills after the label. */
    fields?: string[]
    detail?: string
    /** Why the row can't be picked right now. */
    blocked?: string
    /** A folder's path that Tab types in to list its contents. */
    complete?: string
  }
</script>

<script lang="ts">
  import Icon from '$lib/icons/Icon.svelte'
  import { Kbd } from '$lib/components/ui/kbd'
  import { PaletteFooter, commandGroupHeadingVariants, commandItemVariants } from '$lib/components/ui/command'

  let {
    id,
    label,
    items,
    active,
    onPick,
    onActive
  }: {
    id: string
    label: string
    items: Suggestion[]
    active: number
    onPick: (index: number) => void
    onActive: (index: number) => void
  } = $props()

  const groups = $derived(
    items.reduce<{ heading: string; rows: { item: Suggestion; index: number }[] }[]>((groups, item, index) => {
      if (groups.at(-1)?.heading !== item.group) groups.push({ heading: item.group, rows: [] })
      groups.at(-1)!.rows.push({ item, index })
      return groups
    }, [])
  )

  $effect(() => {
    document.getElementById(`${id}-${active}`)?.scrollIntoView({ block: 'nearest' })
  })
</script>

<!-- Pointer-down keeps focus in the composer; the composer owns the keyboard. -->
<div
  data-slot="prompt-suggestions"
  role="presentation"
  onpointerdown={(event) => event.preventDefault()}
  class="absolute inset-x-0 bottom-full z-20 mb-2 flex max-h-[min(26rem,50vh)] flex-col overflow-hidden rounded-xl border border-edge bg-raised shadow-popover"
>
  <div
    {id}
    role="listbox"
    aria-label={label}
    tabindex="-1"
    class="min-h-0 flex-1 [scroll-padding-block:0.25rem] overflow-y-auto p-1"
  >
    {#each groups as group (group.heading)}
      <div role="group" aria-label={group.heading}>
        <div aria-hidden="true" class={commandGroupHeadingVariants()}>{group.heading}</div>
        {#each group.rows as { item, index } (item.key)}
          <!-- The composer drives these through aria-activedescendant and owns their keys. -->
          <!-- svelte-ignore a11y_click_events_have_key_events -->
          <div
            id={`${id}-${index}`}
            role="option"
            tabindex="-1"
            aria-selected={index === active}
            aria-disabled={item.blocked ? true : undefined}
            data-selected={index === active ? '' : undefined}
            class={commandItemVariants({ class: item.blocked ? 'opacity-50' : '' })}
            onpointermove={() => {
              if (index !== active) onActive(index)
            }}
            onclick={() => onPick(index)}
          >
            {#if item.icon}<Icon icon={item.icon} />{/if}
            <span class="shrink-0">{item.label}</span>
            {#each item.fields ?? [] as field (field)}
              <span class="shrink-0 rounded-sm bg-overlay px-1.5 text-xs text-muted">{field}</span>
            {/each}
            {#if item.detail}<span class="ml-auto min-w-0 truncate ps-4 text-xs text-subtle">{item.detail}</span>{/if}
            {#if item.blocked}<span class="shrink-0 text-xs text-muted {item.detail ? '' : 'ml-auto'}"
                >{item.blocked}</span
              >{/if}
          </div>
        {/each}
      </div>
    {:else}
      <p class="px-2 py-1.5 text-sm text-muted">No matches</p>
    {/each}
  </div>
  <PaletteFooter>
    <div class="flex items-center gap-3">
      <span class="flex items-center gap-1.5"><Kbd>↑</Kbd><Kbd>↓</Kbd> navigate</span>
      <span class="flex items-center gap-1.5"><Kbd>↵</Kbd> select</span>
      {#if items[active]?.complete}<span class="flex items-center gap-1.5"><Kbd>Tab</Kbd> open folder</span>{/if}
    </div>
    <span class="flex items-center gap-1.5"><Kbd>Esc</Kbd> close</span>
  </PaletteFooter>
</div>
