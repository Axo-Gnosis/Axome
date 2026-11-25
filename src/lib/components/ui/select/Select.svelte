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
    variant = 'field',
    class: className,
    'aria-label': ariaLabel
  }: {
    value?: NoInfer<T>
    items: readonly (T | { value: T; label: string; description?: string })[]
    placeholder?: string
    onValueChange?: (value: T) => void
    disabled?: boolean
    /** `field` is a form input; `inline` reads as text in a row, its hover pad hanging into the gutter. */
    variant?: 'field' | 'inline'
    class?: string
    'aria-label'?: string
  } = $props()

  const options = $derived(
    items.map((item): { value: T; label: string; description?: string } =>
      typeof item === 'string' ? { value: item, label: item } : item
    )
  )
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
      'flex cursor-pointer items-center text-left data-disabled:cursor-not-allowed data-disabled:opacity-50 data-placeholder:text-subtle',
      variant === 'field'
        ? [inputVariants(), 'min-w-32 justify-between gap-2 data-[state=open]:border-accent']
        : '-ms-1.5 h-5.5 max-w-48 min-w-0 gap-1 rounded-md px-1.5 text-sm text-fg hover:bg-raised focus-visible:shadow-focus-ring focus-visible:outline-none data-[state=open]:bg-raised',
      className
    )}
  >
    <span class="min-w-0 truncate">{selected?.label ?? placeholder}</span>
    <Icon icon={ArrowDown01Icon} size={variant === 'field' ? 14 : 10} class="shrink-0 text-muted" />
  </SelectPrimitive.Trigger>
  <SelectPrimitive.Portal>
    <SelectPrimitive.Content
      data-slot="select-content"
      sideOffset={4}
      align={variant === 'inline' ? 'start' : 'center'}
      class={cn(
        menuContentVariants({ source: 'select' }),
        // An inline trigger is narrower than its options; cap the menu so descriptions wrap.
        variant === 'field' ? 'min-w-(--bits-select-anchor-width)' : 'max-w-80'
      )}
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
              {#if item.description}
                <span class="flex min-w-0 flex-col">
                  <span class="truncate">{item.label || item.value}</span>
                  <span class="line-clamp-2 text-xs text-muted">{item.description}</span>
                </span>
              {:else}
                <span class="min-w-0 truncate">{item.label || item.value}</span>
              {/if}
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
