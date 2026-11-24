<!--
  @component
  EffortPicker — model options: a trigger with an animated `EffortGlyph`
  and a popover with the reasoning slider and optional service tier choices.

  @param value - current effort level; `''` is the `defaultLabel` stop
  @param levels - supported levels for the active model (defaults to all 7)
  @param defaultLabel - label for a leading "inherit/default" stop (`''`)
  @param variant - `ghost` (composer toolbar) or `field` (form control)
  @param label - accessible name and tooltip
-->

<script lang="ts">
  import type { Snippet } from 'svelte'
  import { Button } from '$lib/components/ui/button'
  import { inputVariants } from '$lib/components/ui/input'
  import * as Popover from '$lib/components/ui/popover'
  import { Radio } from '$lib/components/ui/radio'
  import { Slider } from '$lib/components/ui/slider'
  import * as Tooltip from '$lib/components/ui/tooltip'
  import Icon from '$lib/icons/Icon.svelte'
  import type { ChatServiceTier, ChatThinking } from '$lib/types/backend'
  import { cn } from '$lib/utils/cn'
  import { ArrowDown01Icon } from '@hugeicons/core-free-icons'
  import EffortGlyph from './EffortGlyph.svelte'
  import { THINKING_LEVELS, EFFORT_DESCRIPTIONS, SERVICE_TIERS, TIER_LABELS } from './selector'

  let {
    value = '',
    levels = THINKING_LEVELS,
    defaultLabel,
    variant = 'field',
    label = 'Reasoning effort',
    shortcutHint,
    open = $bindable(false),
    disabled = false,
    side = 'bottom',
    align = 'end',
    class: className,
    tier = null,
    effectiveTier,
    tierDisabled = false,
    serviceTiers,
    modelName,
    tierDefaultLabel,
    tierNote,
    onTierSelect,
    onSelect
  }: {
    value?: ChatThinking | ''
    levels?: readonly ChatThinking[]
    defaultLabel?: string
    variant?: 'ghost' | 'field'
    label?: string
    shortcutHint?: string
    open?: boolean
    disabled?: boolean
    side?: 'top' | 'bottom'
    align?: 'start' | 'end'
    class?: string
    tier?: ChatServiceTier | null
    effectiveTier?: ChatServiceTier
    tierDisabled?: boolean
    serviceTiers?: Partial<Record<ChatServiceTier, number>>
    modelName?: string
    tierDefaultLabel?: string
    tierNote?: Snippet
    onTierSelect?: (tier: ChatServiceTier | null) => void
    onSelect: (level: ChatThinking | '') => void
  } = $props()

  const uid = $props.id()
  const stops: readonly (ChatThinking | '')[] = $derived([
    ...(defaultLabel ? ['' as const] : []),
    ...(levels.length ? levels : THINKING_LEVELS)
  ])
  const index = $derived(Math.max(0, stops.indexOf(value)))
  const currentStop = $derived(value || (defaultLabel ? '' : stops[index]))
  const displayName = $derived(currentStop || defaultLabel || 'off')
  const description = $derived(currentStop ? EFFORT_DESCRIPTIONS[currentStop] : 'Uses the model or @default effort')
  const choices = $derived(
    SERVICE_TIERS.filter((option) => option === 'standard' || serviceTiers?.[option] !== undefined)
  )
  const standardOnly = $derived(serviceTiers !== undefined && choices.length === 1)
  const remembered = $derived(tier && effectiveTier && tier !== effectiveTier ? tier : null)
  const shownTier = $derived(effectiveTier ?? tier)
  const unoffered = $derived(serviceTiers !== undefined && !effectiveTier && !!tier && !choices.includes(tier))

  function setIndex(nextIndex: number) {
    if (disabled) return
    const clamped = Math.min(stops.length - 1, Math.max(0, nextIndex))
    const next = stops[clamped] ?? ''
    if (next !== value) onSelect(next)
  }

  function pickTier(event: Event, next: ChatServiceTier | null) {
    // Keep the checked state tied to the saved/live selection, not the click or arrow key.
    if (event.type === 'click') event.preventDefault()
    else if (event.currentTarget instanceof HTMLInputElement) {
      const root = event.currentTarget.closest('fieldset') ?? document
      for (const input of root.querySelectorAll<HTMLInputElement>(
        `input[type="radio"][name="${CSS.escape(event.currentTarget.name)}"]`
      )) {
        input.checked = input.value === (tier ?? '')
      }
    }
    if (!disabled && !tierDisabled && next !== tier) onTierSelect?.(next)
  }
</script>

{#snippet summary()}
  <EffortGlyph level={currentStop} size={16} />
  <span
    class={cn(
      'truncate',
      currentStop === 'max'
        ? 'text-warning-bright'
        : currentStop
          ? variant === 'field'
            ? 'text-bright'
            : 'text-fg'
          : 'text-muted italic'
    )}
  >
    {displayName}
  </span>
  {#if remembered}
    <span class="truncate text-xs text-muted" title={`Standard (${TIER_LABELS[remembered]} unavailable)`}
      >· Standard ({TIER_LABELS[remembered]} unavailable)</span
    >
  {:else if shownTier && shownTier !== 'standard'}
    <span class="shrink-0 text-xs text-muted">· {TIER_LABELS[shownTier]}</span>
  {/if}
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
                data-slot="effort-picker"
                variant="ghost"
                size="sm"
                aria-label={label}
                aria-keyshortcuts={shortcutHint}
                {disabled}
                class={cn('hover:bg-raised', className)}
              >
                {@render summary()}
                <Icon icon={ArrowDown01Icon} size={12} />
              </Button>
            {:else}
              <button
                {...props}
                type="button"
                data-slot="effort-picker"
                aria-label={label}
                {disabled}
                class={cn(
                  inputVariants(),
                  'flex shrink-0 cursor-pointer items-center gap-2 text-left disabled:cursor-not-allowed disabled:opacity-50 data-[state=open]:border-accent',
                  onTierSelect ? 'w-44' : 'w-34',
                  className
                )}
              >
                <span class="flex min-w-0 flex-1 items-center gap-2">{@render summary()}</span>
                <Icon icon={ArrowDown01Icon} size={14} class="shrink-0 text-muted" />
              </button>
            {/if}
          {/snippet}
        </Popover.Trigger>
      {/snippet}
    </Tooltip.Trigger>
    {#if variant === 'ghost'}
      <Tooltip.Content>
        {label}
        {#if shortcutHint}<span class="block text-xs text-muted">Cycle reasoning effort ({shortcutHint})</span>{/if}
      </Tooltip.Content>
    {/if}
  </Tooltip.Root>

  <Popover.Content {side} {align} class="flex w-72 flex-col gap-3 p-3.5">
    <div class="flex items-center gap-2.5">
      <EffortGlyph level={currentStop} size={20} />
      <div class="flex min-w-0 flex-1 items-baseline justify-between gap-2">
        <span
          class={cn(
            'text-sm font-medium',
            currentStop === 'max' ? 'text-warning-bright' : currentStop ? 'text-bright' : 'text-muted'
          )}
        >
          {displayName}
        </span>
        <span class="text-xs text-subtle">{description}</span>
      </div>
    </div>

    <Slider
      aria-label="Reasoning effort"
      disabled={disabled || stops.length < 2}
      ticks
      min={0}
      max={stops.length - 1}
      value={index}
      onValueChange={setIndex}
    />

    {#if onTierSelect}
      <fieldset disabled={disabled || tierDisabled} class="flex flex-col gap-1.5 border-t border-edge pt-2">
        <legend class="sr-only">Service tier</legend>
        <div class="flex items-baseline justify-between gap-2">
          <span class="text-sm font-medium text-bright">Service tier</span>
          <span class="text-xs text-muted">est. cost vs Standard</span>
        </div>
        {#if remembered && modelName}
          <div class="rounded border border-warning/40 bg-warning-bg px-2 py-1.5 text-xs text-warning-bright">
            <p>
              {TIER_LABELS[remembered]} is remembered, but {modelName}
              {standardOnly ? 'can only use Standard' : `cannot use ${TIER_LABELS[remembered]}`}. Requests use Standard.
            </p>
            <Button
              variant="text"
              size="xs"
              disabled={disabled || tierDisabled}
              onclick={() => onTierSelect?.('standard')}
            >
              Use Standard
            </Button>
          </div>
        {/if}
        {#if tierDefaultLabel}
          {@render tierChoice(null, tierDefaultLabel)}
        {/if}
        {#each choices as option (option)}
          {@render tierChoice(option, TIER_LABELS[option], option === 'standard' ? 1 : serviceTiers?.[option])}
        {/each}
        {#if standardOnly && !remembered && !unoffered}
          <p class="text-xs text-muted">Standard only on this model.</p>
        {/if}
        {#if unoffered && tier}
          <p class="text-xs text-muted">{TIER_LABELS[tier]} is saved but isn't offered here.</p>
        {/if}
      </fieldset>
      {#if tierNote}{@render tierNote()}{/if}
    {/if}
  </Popover.Content>
</Popover.Root>

{#snippet tierChoice(option: ChatServiceTier | null, title: string, factor?: number)}
  <label
    class="flex cursor-pointer items-center gap-2 rounded border border-edge bg-surface px-2 py-1.5 text-sm
    has-checked:border-accent/40 has-disabled:cursor-not-allowed has-disabled:opacity-50"
  >
    <Radio
      name={`${uid}-tier`}
      value={option ?? ''}
      checked={tier === option}
      onclick={(event) => pickTier(event, option)}
      onchange={(event) => pickTier(event, option)}
    />
    <span class="text-fg">{title}</span>
    {#if option && option !== 'standard'}
      <span class="truncate text-xs text-muted">{option === 'flex' ? 'slower, cheaper' : 'priority'}</span>
    {/if}
    {#if factor !== undefined}
      <span class="ms-auto font-mono text-xs text-muted">{factor}×</span>
    {/if}
  </label>
{/snippet}
