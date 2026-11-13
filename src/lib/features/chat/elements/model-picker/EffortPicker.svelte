<!--
  @component
  EffortPicker — reasoning effort chooser (`off` through `max`): a trigger
  button with an animated `EffortGlyph` + level name and a compact popover
  housing a ticked `Slider`.

  @param value - current effort level; `''` is the `defaultLabel` stop
  @param levels - supported levels for the active model (defaults to all 7)
  @param defaultLabel - label for a leading "inherit/default" stop (`''`)
  @param variant - `ghost` (composer toolbar) or `field` (form control)
  @param label - accessible name and tooltip
-->

<script lang="ts">
  import { Button } from '$lib/components/ui/button'
  import { inputVariants } from '$lib/components/ui/input'
  import * as Popover from '$lib/components/ui/popover'
  import { Slider } from '$lib/components/ui/slider'
  import * as Tooltip from '$lib/components/ui/tooltip'
  import Icon from '$lib/icons/Icon.svelte'
  import type { ChatThinking } from '$lib/types/backend'
  import { cn } from '$lib/utils/cn'
  import { ArrowDown01Icon } from '@hugeicons/core-free-icons'
  import EffortGlyph from './EffortGlyph.svelte'
  import { THINKING_LEVELS, EFFORT_DESCRIPTIONS } from './selector'

  let {
    value = '',
    levels = THINKING_LEVELS,
    defaultLabel,
    variant = 'field',
    label = 'Reasoning effort',
    open = $bindable(false),
    disabled = false,
    side = 'bottom',
    align = 'end',
    class: className,
    onSelect
  }: {
    value?: string
    levels?: readonly ChatThinking[]
    defaultLabel?: string
    variant?: 'ghost' | 'field'
    label?: string
    open?: boolean
    disabled?: boolean
    side?: 'top' | 'bottom'
    align?: 'start' | 'end'
    class?: string
    onSelect: (level: string) => void
  } = $props()

  const stops = $derived([...(defaultLabel ? [''] : []), ...(levels.length ? levels : THINKING_LEVELS)])
  const index = $derived(Math.max(0, stops.indexOf(value)))
  const currentStop = $derived(stops[index] ?? '')
  const displayName = $derived(currentStop || defaultLabel || 'off')
  const description = $derived(
    currentStop ? (EFFORT_DESCRIPTIONS[currentStop as ChatThinking] ?? '') : 'Uses the model or @default effort'
  )

  function setIndex(nextIndex: number) {
    const clamped = Math.min(stops.length - 1, Math.max(0, nextIndex))
    const next = stops[clamped] ?? ''
    if (next !== value) onSelect(next)
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
                  'flex w-34 shrink-0 cursor-pointer items-center gap-2 text-left disabled:cursor-not-allowed disabled:opacity-50 data-[state=open]:border-accent',
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
    {#if variant === 'ghost'}<Tooltip.Content>{label}</Tooltip.Content>{/if}
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

    <Slider aria-label={label} ticks min={0} max={stops.length - 1} value={index} onValueChange={setIndex} />
  </Popover.Content>
</Popover.Root>
