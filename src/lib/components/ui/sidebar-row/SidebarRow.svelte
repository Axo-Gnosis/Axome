<!--
  @component
  Sidebar rows separate behavior, density and tone. The exported recipe also
  supports composed native controls. `on` chooses the backing surface; hover
  steps up one level only for interactive rows.
  Disclosure state stays caller-owned. `content` associates an expanded region
  with its trigger, while `actions` always renders sibling controls.
-->
<script lang="ts" module>
  import { tv, type VariantProps } from 'tailwind-variants'

  export const sidebarRowVariants = tv({
    base: 'flex w-full items-center gap-1.5 px-2.5 text-left text-sm',
    variants: {
      behavior: {
        display: '',
        action: 'focus-visible:shadow-focus-ring focus-visible:outline-none',
        disclosure: 'focus-visible:shadow-focus-ring focus-visible:outline-none'
      },
      density: {
        compact: 'h-6',
        normal: 'h-7'
      },
      tone: {
        default: '',
        muted: 'text-muted'
      },
      on: {
        ground: '',
        surface: ''
      },
      divider: {
        true: 'border-t border-edge/30',
        false: ''
      },
      pressed: {
        true: 'bg-raised',
        false: ''
      }
    },
    compoundVariants: [
      { behavior: ['action', 'disclosure'], on: 'ground', class: 'hover:bg-surface' },
      { behavior: ['action', 'disclosure'], on: 'surface', class: 'hover:bg-raised' },
      { behavior: ['action', 'disclosure'], tone: 'muted', class: 'hover:text-bright' }
    ],
    defaultVariants: {
      behavior: 'action',
      density: 'normal',
      tone: 'default',
      on: 'ground',
      divider: true,
      pressed: false
    }
  })

  export type SidebarRowBehavior = NonNullable<VariantProps<typeof sidebarRowVariants>['behavior']>
  export type SidebarRowDensity = NonNullable<VariantProps<typeof sidebarRowVariants>['density']>
  export type SidebarRowTone = NonNullable<VariantProps<typeof sidebarRowVariants>['tone']>
  export type SidebarRowOn = NonNullable<VariantProps<typeof sidebarRowVariants>['on']>
</script>

<script lang="ts">
  import { cn } from '$lib/utils/cn'
  import { treeIndent } from '$lib/components/ui/tree-indent'
  import type { Snippet } from 'svelte'
  import type { HTMLAttributes, HTMLButtonAttributes } from 'svelte/elements'

  const uid = $props.id()
  let {
    behavior = 'action',
    density = 'normal',
    tone = 'default',
    on = 'ground',
    expanded = false,
    pressed = false,
    divider = true,
    depth = 0,
    id,
    style: customStyle,
    class: className,
    children,
    actions,
    content,
    ...rest
  }: HTMLAttributes<HTMLElement> &
    Pick<HTMLButtonAttributes, 'disabled'> & {
      behavior?: SidebarRowBehavior
      density?: SidebarRowDensity
      tone?: SidebarRowTone
      on?: SidebarRowOn
      expanded?: boolean
      pressed?: boolean
      divider?: boolean
      depth?: number
      children?: Snippet
      actions?: Snippet
      content?: Snippet
    } = $props()

  const interactive = $derived(behavior !== 'display')
  const associated = $derived(behavior === 'disclosure' && !!content)
  const triggerId = $derived(id ?? (associated ? `${uid}-trigger` : undefined))
  const style = $derived(`${depth > 0 ? `padding-left: ${treeIndent(depth)};` : ''}${customStyle ?? ''}`)
</script>

{#snippet rowBody()}
  {#if children}{@render children()}{/if}
{/snippet}

{#if actions}
  <div
    data-slot="sidebar-row"
    class={cn(
      sidebarRowVariants({ behavior, density, tone, on, pressed, divider }),
      'group/sidebar-row pr-1',
      className
    )}
    {style}
  >
    {#if interactive}
      <button
        type="button"
        id={triggerId}
        aria-expanded={behavior === 'disclosure' ? expanded : undefined}
        aria-controls={associated ? `${uid}-content` : undefined}
        class="flex h-full min-w-0 flex-1 cursor-pointer items-center gap-1.5 text-left focus-visible:shadow-focus-ring focus-visible:outline-none"
        {...rest}
      >
        {@render rowBody()}
      </button>
    {:else}
      <div {id} class="flex min-w-0 flex-1 items-center gap-1.5" {...rest}>
        {@render rowBody()}
      </div>
    {/if}
    <div
      class="hidden shrink-0 items-center gap-0.5 group-hover/sidebar-row:flex group-has-[:focus-visible]/sidebar-row:flex"
    >
      {@render actions()}
    </div>
  </div>
{:else if interactive}
  <button
    type="button"
    id={triggerId}
    data-slot="sidebar-row"
    aria-expanded={behavior === 'disclosure' ? expanded : undefined}
    aria-controls={associated ? `${uid}-content` : undefined}
    class={cn(sidebarRowVariants({ behavior, density, tone, on, pressed, divider }), className)}
    {style}
    {...rest}
  >
    {@render rowBody()}
  </button>
{:else}
  <div
    {id}
    data-slot="sidebar-row"
    class={cn(sidebarRowVariants({ behavior, density, tone, on, pressed, divider }), className)}
    {style}
    {...rest}
  >
    {@render rowBody()}
  </div>
{/if}

{#if behavior === 'disclosure' && content && expanded}
  <div id={`${uid}-content`} role="region" aria-labelledby={triggerId}>
    {@render content()}
  </div>
{/if}
