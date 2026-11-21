<!--
  @component
  ModelPicker — the one model chooser: a trigger showing the current choice
  and a searchable popover listing roles (as @chips) and models grouped by
  provider. Used by the chat composer and the agent settings.

  Values are axo selectors: `provider/model` or `@role`; `''` is the "none"
  choice when `none` is given.

  @param value - current selector
  @param models - available models
  @param loaded - false while the model list is still unknown; an unlisted value is then shown plainly, not as unavailable
  @param roles - roles offered first; omit where a role can't be chosen
  @param none - label of the empty choice (e.g. "Inherits @default"); omitted = no empty choice
  @param custom - offer typed `provider/model` or `@role` text that isn't listed
  @param variant - `ghost` (composer toolbar) or `field` (form control)
  @param label - accessible name and tooltip
-->
<script lang="ts">
  import { Button } from '$lib/components/ui/button'
  import { Chip } from '$lib/components/ui/chip'
  import * as Command from '$lib/components/ui/command'
  import { PaletteFooter, paletteSurfaceVariants } from '$lib/components/ui/command'
  import { inputVariants } from '$lib/components/ui/input'
  import * as Popover from '$lib/components/ui/popover'
  import * as Tooltip from '$lib/components/ui/tooltip'
  import { formatTokens } from '$lib/features/chat/format'
  import Icon from '$lib/icons/Icon.svelte'
  import type { ChatModel, ChatModelRole } from '$lib/types/backend'
  import { cn } from '$lib/utils/cn'
  import { splitTokens } from '$lib/utils/queryTokens'
  import { ArrowDown01Icon, CheckIcon } from '@hugeicons/core-free-icons'
  import { splitSelector, ROLE_NAME } from './selector'

  let {
    value,
    models,
    loaded = true,
    roles = [],
    none,
    custom = false,
    variant = 'field',
    label,
    open = $bindable(false),
    disabled = false,
    side = 'bottom',
    align = 'start',
    class: className,
    onSelect
  }: {
    value: string
    models: ChatModel[]
    loaded?: boolean
    roles?: ChatModelRole[]
    none?: string
    custom?: boolean
    variant?: 'ghost' | 'field'
    label: string
    open?: boolean
    disabled?: boolean
    side?: 'top' | 'bottom'
    align?: 'start' | 'end'
    class?: string
    onSelect: (value: string) => void
  } = $props()

  let search = $state('')

  const providers = $derived([...Map.groupBy(models, (model) => model.provider)])

  /** A selector's model and effort suffix; the suffix only counts when the rest names a known model. */
  function resolve(selector: string | null): { model: ChatModel | null; effort: string | null } {
    if (!selector) return { model: null, effort: null }
    const exact = models.find((model) => model.selector === selector)
    if (exact) return { model: exact, effort: null }
    const split = splitSelector(selector)
    const base = models.find((model) => model.selector === split.model)
    return base ? { model: base, effort: split.effort } : { model: null, effort: null }
  }

  const current = $derived(resolve(value))
  const currentRole = $derived(value.startsWith('@') ? roles.find((role) => `@${role.name}` === value) : undefined)
  const typed = $derived(search.trim())
  const offerTyped = $derived(
    custom &&
      (typed.includes('/') || (roles.length > 0 && typed.startsWith('@') && ROLE_NAME.test(typed.slice(1)))) &&
      !models.some((model) => model.selector === typed) &&
      !roles.some((role) => `@${role.name}` === typed)
  )

  function roleTarget(role: ChatModelRole): string {
    if (!role.model) return 'Not set'
    const { model, effort } = resolve(role.model)
    return model ? `${model.name}${effort ? ` · ${effort}` : ''}` : role.model
  }

  function choose(next: string) {
    onSelect(next)
    open = false
  }

  $effect(() => {
    if (!open) search = ''
  })
  // Every typed word must appear in the row's selector, name or provider; rows keep catalog order.
  // (Command's default fuzzy score matches scattered letters, so "sol" hit "SOnnet … (Latest)".)
  function filter(value: string, search: string, keywords: string[] = []): number {
    const haystack = [value, ...keywords].join(' ').toLowerCase()
    return splitTokens(search.toLowerCase()).every((word) => haystack.includes(word)) ? 1 : 0
  }
</script>

{#snippet summary()}
  {#if value.startsWith('@')}
    <Chip tone="accent" class="font-mono">{value}</Chip>
    {#if variant === 'field'}
      <span class="truncate text-muted">{currentRole ? roleTarget(currentRole) : 'Unknown role'}</span>
    {/if}
  {:else if current.model}
    {#if variant === 'field'}<span class="shrink-0 text-muted">{current.model.provider} /</span>{/if}
    <span class="truncate {variant === 'field' ? 'text-bright' : ''}">{current.model.name}</span>
    {#if current.effort}<span class="shrink-0 text-xs text-subtle">· {current.effort}</span>{/if}
    {#if variant === 'field'}
      <span class="shrink-0 text-xs text-subtle">{formatTokens(current.model.contextWindow)}</span>
    {/if}
  {:else if value}
    <span class="truncate">{value}</span>
    {#if loaded}<span class="shrink-0 text-danger">· unavailable</span>{/if}
  {:else}
    <span class="truncate text-subtle italic">{none ?? 'Choose a model'}</span>
  {/if}
{/snippet}

{#snippet check(selected: boolean)}
  {#if selected}<Icon icon={CheckIcon} size={12} class="shrink-0 text-accent" />{/if}
{/snippet}

<Popover.Root bind:open>
  <Tooltip.Root>
    <Tooltip.Trigger {disabled}>
      {#snippet child({ props: tooltipProps })}
        <Popover.Trigger {...tooltipProps} {disabled}>
          {#snippet child({ props })}
            {#if variant === 'ghost'}
              <Button
                {...props}
                data-slot="model-picker"
                variant="ghost"
                size="sm"
                aria-label={label}
                {disabled}
                class={cn('max-w-64 hover:bg-raised', className)}
              >
                {@render summary()}
                <Icon icon={ArrowDown01Icon} size={12} />
              </Button>
            {:else}
              <button
                {...props}
                type="button"
                data-slot="model-picker"
                aria-label={label}
                {disabled}
                class={cn(
                  inputVariants(),
                  'flex min-w-0 cursor-pointer items-center gap-1.5 text-left disabled:cursor-not-allowed disabled:opacity-50 data-[state=open]:border-accent',
                  className
                )}
              >
                <span class="flex min-w-0 flex-1 items-center gap-1.5">{@render summary()}</span>
                <Icon icon={ArrowDown01Icon} size={14} class="shrink-0 text-muted" />
              </button>
            {/if}
          {/snippet}
        </Popover.Trigger>
      {/snippet}
    </Tooltip.Trigger>
    {#if variant === 'ghost'}<Tooltip.Content>{label}</Tooltip.Content>{/if}
  </Tooltip.Root>

  <Popover.Content {side} {align} class={paletteSurfaceVariants({ class: 'w-104 p-0' })}>
    <Command.Root class="rounded-lg bg-transparent" {label} {filter}>
      <Command.Input placeholder="Search models and roles…" bind:value={search} />
      <Command.List class="max-h-80 overflow-y-auto">
        {#if !offerTyped}<Command.Empty>No matches</Command.Empty>{/if}
        {#if none}
          <Command.Group>
            <Command.Item value={`none ${none}`} onSelect={() => choose('')}>
              <span class="flex-1 text-muted italic">{none}</span>
              {@render check(value === '')}
            </Command.Item>
          </Command.Group>
        {/if}
        {#if offerTyped}
          <Command.Group forceMount>
            <Command.Item value={`typed ${typed}`} forceMount onSelect={() => choose(typed)}>
              <span class="flex-1 truncate">Use <span class="font-mono text-bright">{typed}</span></span>
            </Command.Item>
          </Command.Group>
        {/if}
        {#if roles.length}
          <Command.Group heading="Roles">
            {#each roles as role (role.name)}
              <Command.Item
                value={`@${role.name}`}
                keywords={[role.name, role.model ?? '']}
                onSelect={() => choose(`@${role.name}`)}
              >
                <Chip tone="accent" class="font-mono">@{role.name}</Chip>
                <span class="min-w-0 flex-1 truncate text-xs text-muted">{roleTarget(role)}</span>
                {@render check(value === `@${role.name}`)}
              </Command.Item>
            {/each}
          </Command.Group>
        {/if}
        {#each providers as [provider, list] (provider)}
          <Command.Group heading={provider}>
            {#each list as model (model.selector)}
              <Command.Item
                value={model.selector}
                keywords={[model.name, model.provider]}
                onSelect={() => choose(model.selector)}
              >
                <span class="min-w-0 flex-1 truncate">{model.name}</span>
                <span class="shrink-0 text-xs text-subtle">{formatTokens(model.contextWindow)}</span>
                {@render check(current.model?.selector === model.selector && !value.startsWith('@'))}
              </Command.Item>
            {/each}
          </Command.Group>
        {/each}
      </Command.List>
    </Command.Root>
    <PaletteFooter hints={[{ keys: ['↵'], label: 'select' }]} />
  </Popover.Content>
</Popover.Root>
