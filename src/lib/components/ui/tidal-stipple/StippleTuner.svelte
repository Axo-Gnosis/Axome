<script lang="ts">
  import {
    ArrowDown01Icon,
    ArrowRight01Icon,
    CheckIcon,
    Copy01Icon,
    RotateCcwIcon,
    SlidersHorizontalIcon,
    XIcon
  } from '@hugeicons/core-free-icons'
  import { Button, IconButton } from '$lib/components/ui/button'
  import * as Collapsible from '$lib/components/ui/collapsible'
  import { Select } from '$lib/components/ui/select'
  import { Slider } from '$lib/components/ui/slider'
  import Icon from '$lib/icons/Icon.svelte'
  import { STIPPLE_KNOBS, STIPPLE_SHAPES, type StippleKnob } from './params'
  import { resetStippleTuner, stippleTuner } from './tuner.svelte'

  interface KnobSpec {
    key: StippleKnob
    label: string
    step?: number
  }

  const COLOR_TOKENS: readonly string[] = [
    '--color-warm',
    '--color-subtle',
    '--color-muted',
    '--color-edge-strong',
    '--color-accent-dim',
    '--color-accent',
    '--color-accent-bright',
    '--color-peach',
    '--color-bright',
    '--color-warning'
  ]

  const SECTIONS: readonly { title: string; knobs: readonly KnobSpec[] }[] = [
    {
      title: 'Grid & Dots',
      knobs: [
        { key: 'intensity', label: 'Intensity' },
        { key: 'cellPx', label: 'Cell Px' },
        { key: 'scale', label: 'Scale' },
        { key: 'jitter', label: 'Grid Jitter' },
        { key: 'dotTop', label: 'Size Top' },
        { key: 'dotBottom', label: 'Size Bottom' },
        { key: 'sizeJitter', label: 'Size Jitter' },
        { key: 'edgeSoft', label: 'Edge Soft' }
      ]
    },
    {
      title: 'Density Curve',
      knobs: [
        { key: 'density', label: 'Density' },
        { key: 'topDensity', label: 'Top Density' },
        { key: 'peakDensity', label: 'Peak Density' },
        { key: 'curveStart', label: 'Curve Start' },
        { key: 'curveEnd', label: 'Curve End' },
        { key: 'curveExp', label: 'Curve Exp' },
        { key: 'growWidth', label: 'Grow Width' }
      ]
    },
    {
      title: 'Wave & Shimmer',
      knobs: [
        { key: 'speed', label: 'Speed' },
        { key: 'tide', label: 'Tide' },
        { key: 'tidePeriod', label: 'Tide Period (ms)', step: 250 },
        { key: 'waveLength', label: 'Wave Length' },
        { key: 'waveMix', label: 'Wave Mix' },
        { key: 'crest', label: 'Crest' },
        { key: 'glint', label: 'Glint' },
        { key: 'shimmerTop', label: 'Shimmer Top' },
        { key: 'shimmerBottom', label: 'Shimmer Bottom' },
        { key: 'colorVar', label: 'Color Var' },
        { key: 'rimDarken', label: 'Rim Darken' }
      ]
    },
    {
      title: 'Seed & Post',
      knobs: [
        { key: 'seed', label: 'Seed' },
        { key: 'grain', label: 'Grain' },
        { key: 'vignette', label: 'Vignette' }
      ]
    }
  ]

  let open = $state(false)
  let copied = $state(false)
  let sectionOpen = $state([true, false, false, false])

  async function copyPreset() {
    const payload = JSON.stringify($state.snapshot(stippleTuner), null, 2)
    await navigator.clipboard?.writeText(payload)
    copied = true
    setTimeout(() => {
      copied = false
    }, 1500)
  }
</script>

{#snippet knobRow(spec: KnobSpec)}
  <div class="flex flex-col gap-1">
    <div class="flex items-center justify-between text-xs">
      <span class="text-fg">{spec.label}</span>
      <span class="font-mono text-muted">{stippleTuner[spec.key]}</span>
    </div>
    <Slider
      aria-label={spec.label}
      min={STIPPLE_KNOBS[spec.key].min}
      max={STIPPLE_KNOBS[spec.key].max}
      step={spec.step ?? 1}
      value={stippleTuner[spec.key]}
      onValueChange={(next) => (stippleTuner[spec.key] = next)}
      class="h-4"
    />
  </div>
{/snippet}

<div data-slot="stipple-tuner" class="fixed bottom-3 left-12 z-50 flex flex-col items-start gap-2">
  {#if open}
    <div
      role="region"
      aria-label="Tidal Stipple Dev Tuner"
      class="flex max-h-[75vh] w-88 flex-col overflow-hidden rounded-lg border border-edge bg-raised text-base shadow-popover"
    >
      <div class="flex shrink-0 items-center justify-between border-b border-edge px-3 py-2">
        <span class="text-sm font-semibold text-bright">Tidal Stipple Dev</span>
        <div class="flex items-center gap-1">
          <IconButton tooltip={copied ? 'Copied' : 'Copy JSON'} size="sm" onclick={() => void copyPreset()}>
            <Icon icon={copied ? CheckIcon : Copy01Icon} size={13} class={copied ? 'text-success' : undefined} />
          </IconButton>
          <IconButton tooltip="Reset Defaults" size="sm" onclick={resetStippleTuner}>
            <Icon icon={RotateCcwIcon} size={13} />
          </IconButton>
          <IconButton tooltip="Close" size="sm" onclick={() => (open = false)}>
            <Icon icon={XIcon} size={13} />
          </IconButton>
        </div>
      </div>

      <div class="flex flex-col gap-3 overflow-y-auto p-3">
        <div class="flex flex-col gap-2">
          <div class="flex flex-col gap-1">
            <span class="text-xs text-muted">Shape</span>
            <Select
              aria-label="Shape"
              items={STIPPLE_SHAPES}
              value={stippleTuner.shape}
              onValueChange={(v) => (stippleTuner.shape = v)}
            />
          </div>
          <div class="flex flex-col gap-1">
            <span class="text-xs text-muted">Base Color</span>
            <Select
              aria-label="Base Color"
              items={COLOR_TOKENS}
              value={stippleTuner.baseColor}
              onValueChange={(v) => (stippleTuner.baseColor = v)}
            />
          </div>
          <div class="flex flex-col gap-1">
            <span class="text-xs text-muted">Accent Color</span>
            <Select
              aria-label="Accent Color"
              items={COLOR_TOKENS}
              value={stippleTuner.accentColor}
              onValueChange={(v) => (stippleTuner.accentColor = v)}
            />
          </div>
        </div>

        {#each SECTIONS as section, i (section.title)}
          <Collapsible.Root bind:open={sectionOpen[i]} class="border-t border-edge pt-2.5">
            <Collapsible.Trigger
              class="flex w-full cursor-pointer items-center justify-between py-0.5 focus-visible:shadow-focus-ring focus-visible:outline-none"
            >
              <span class="flex items-center gap-1.5 text-xs font-semibold tracking-wide text-muted uppercase">
                <Icon icon={sectionOpen[i] ? ArrowDown01Icon : ArrowRight01Icon} size={12} />
                {section.title}
              </span>
            </Collapsible.Trigger>
            <Collapsible.Content class="flex flex-col gap-2.5 pt-2">
              {#each section.knobs as spec (spec.key)}{@render knobRow(spec)}{/each}
            </Collapsible.Content>
          </Collapsible.Root>
        {/each}
      </div>
    </div>
  {/if}

  <Button
    variant="default"
    size="sm"
    aria-expanded={open}
    onclick={() => (open = !open)}
    class="bg-surface/90 shadow-popover backdrop-blur-sm"
  >
    <Icon icon={SlidersHorizontalIcon} size={13} class="text-accent" />
    <span>Stipple</span>
  </Button>
</div>
