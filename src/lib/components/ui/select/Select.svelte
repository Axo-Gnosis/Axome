<script lang="ts" generics="T extends string">
  import { Select as SelectPrimitive } from 'bits-ui'
  import { ArrowDown01Icon, ArrowUp01Icon, CheckIcon } from '@hugeicons/core-free-icons'
  import Icon from '$lib/icons/Icon.svelte'
  import { cn } from '$lib/utils/cn'
  import { inputVariants } from '../input'
  import { menuContentVariants, menuItemClass } from '../menu-recipe'

  let {
    value = $bindable('' as T),
    items,
    placeholder = 'Select…',
    onValueChange,
    disabled = false,
    class: className,
    'aria-label': ariaLabel
  }: {
    value?: NoInfer<T>
    items: readonly (T | { value: T; label: string })[]
    placeholder?: string
    onValueChange?: (value: T) => void
    disabled?: boolean
    class?: string
    'aria-label'?: string
  } = $props()

  const options = $derived(items.map((item) => (typeof item === 'string' ? { value: item, label: item } : item)))
  const selected = $derived(options.find((item) => item.value === value))
  const scrollButton = 'flex cursor-default items-center justify-center py-1 text-muted'
</script>

<SelectPrimitive.Root
  type="single"
  {value}
  onValueChange={(next) => {
    value = next as T
    onValueChange?.(value)
  }}
  {disabled}
  items={options}
>
  <SelectPrimitive.Trigger
    data-slot="select-trigger"
    aria-label={ariaLabel}
    class={cn(
      inputVariants(),
      'flex min-w-32 cursor-pointer items-center justify-between gap-2 text-left data-disabled:cursor-not-allowed data-disabled:opacity-50 data-placeholder:text-subtle data-[state=open]:border-accent',
      className
    )}
  >
    <span class="min-w-0 truncate">{selected?.label ?? placeholder}</span>
    <Icon icon={ArrowDown01Icon} size={14} class="shrink-0 text-muted" />
  </SelectPrimitive.Trigger>
  <SelectPrimitive.Portal>
    <SelectPrimitive.Content
      data-slot="select-content"
      sideOffset={4}
      class={cn(menuContentVariants({ source: 'select' }), 'min-w-(--bits-select-anchor-width)')}
    >
      <SelectPrimitive.ScrollUpButton class={scrollButton}
        ><Icon icon={ArrowUp01Icon} size={14} /></SelectPrimitive.ScrollUpButton
      >
      <SelectPrimitive.Viewport>
        {#each options as item (item.value)}
          <SelectPrimitive.Item
            data-slot="select-item"
            value={item.value}
            label={item.label}
            class={cn(menuItemClass, 'pe-8')}
          >
            {#snippet children({ selected })}
              <span class="min-w-0 truncate">{item.label || item.value}</span>
              {#if selected}<Icon
                  icon={CheckIcon}
                  size={14}
                  class="pointer-events-none absolute end-3 text-accent"
                />{/if}
            {/snippet}
          </SelectPrimitive.Item>
        {/each}
      </SelectPrimitive.Viewport>
      <SelectPrimitive.ScrollDownButton class={scrollButton}
        ><Icon icon={ArrowDown01Icon} size={14} /></SelectPrimitive.ScrollDownButton
      >
    </SelectPrimitive.Content>
  </SelectPrimitive.Portal>
</SelectPrimitive.Root>
