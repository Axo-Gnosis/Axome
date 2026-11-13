<!--
  @component
  EffortGlyph — animated radial gauge for reasoning effort (`off` through `max`).

  `off` through `xhigh` smoothly sweep a radial arc from empty (`0`) through
  1/8 (`minimal`), 1/4 (`low`), 1/2 (`medium`), 3/4 (`high`) to a full ring
  (`xhigh`). `max` ignites the full ring with a warm core, a glowing halo and
  a gently rotating four-point star flare.
-->

<script lang="ts">
  import { cn } from '$lib/utils/cn'

  const CIRCUMFERENCE = 2 * Math.PI * 8

  const FRACTIONS: Record<string, number> = {
    '': 0,
    off: 0,
    minimal: 1 / 8,
    low: 1 / 4,
    medium: 2 / 4,
    high: 3 / 4,
    xhigh: 1,
    max: 1
  }

  let {
    level = '',
    size = 16,
    class: className
  }: {
    level?: string
    size?: number
    class?: string
  } = $props()

  const fraction = $derived(FRACTIONS[level] ?? 0)
  const isMax = $derived(level === 'max')
  const offset = $derived(CIRCUMFERENCE * (1 - fraction))
</script>

<svg
  xmlns="http://www.w3.org/2000/svg"
  width={size}
  height={size}
  viewBox="0 0 24 24"
  fill="none"
  aria-hidden="true"
  data-slot="effort-glyph"
  data-level={level || 'default'}
  class={cn('shrink-0 overflow-visible', className)}
>
  <!-- Soft solar aura behind the ring on max -->
  <circle
    cx="12"
    cy="12"
    r="8"
    class={cn(
      'duration-med origin-center fill-warning/25 transition-[opacity,transform] ease-out-quart',
      isMax ? 'scale-110 animate-pulse opacity-100' : 'scale-50 opacity-0'
    )}
  />

  <!-- Secondary diagonal solar rays (counter-rotating, rooted at the r=8 ring) -->
  <g
    class={cn(
      'duration-slow origin-center transition-[opacity,transform] ease-out-expo',
      isMax
        ? 'scale-100 opacity-100 *:origin-[12px_12px] *:animate-[spin_14s_linear_infinite_reverse]'
        : 'scale-50 opacity-0'
    )}
  >
    <path
      d="M21 3 L18.9 7.5 L16.5 5.1 Z
         M21 21 L16.5 18.9 L18.9 16.5 Z
         M3 21 L5.1 16.5 L7.5 18.9 Z
         M3 3 L7.5 5.1 L5.1 7.5 Z"
      class="fill-warning"
    />
  </g>

  <!-- Primary cardinal nova spikes (rotating, rooted at the r=8 ring) -->
  <g
    class={cn(
      'duration-slow origin-center transition-[opacity,transform] ease-out-expo',
      isMax ? 'scale-100 opacity-100 *:origin-[12px_12px] *:animate-[spin_9s_linear_infinite]' : 'scale-50 opacity-0'
    )}
  >
    <path
      d="M12 -2.6 L14.4 4.8 L9.6 4.8 Z
         M26.6 12 L19.2 14.4 L19.2 9.6 Z
         M12 26.6 L9.6 19.2 L14.4 19.2 Z
         M-2.6 12 L4.8 9.6 L4.8 14.4 Z"
      class="fill-warning-bright"
    />
  </g>

  <!-- Dim base track -->
  <circle
    cx="12"
    cy="12"
    r="8"
    stroke-width="2.25"
    class={cn(
      'duration-fast transition-colors ease-out-quart',
      level === '' ? 'stroke-subtle/70 [stroke-dasharray:2_2.5]' : 'stroke-edge-strong'
    )}
  />

  <!-- Animated progress arc (starts at 12 o'clock; stays at r=8 and ignites on max) -->
  <circle
    cx="12"
    cy="12"
    r="8"
    stroke-width={isMax ? 2.5 : 2.25}
    stroke-linecap="round"
    stroke-dasharray={CIRCUMFERENCE}
    stroke-dashoffset={offset}
    class={cn(
      'duration-med origin-center -rotate-90 transition-[stroke-dashoffset,stroke,stroke-width,opacity] ease-out-expo',
      fraction === 0 ? 'opacity-0' : 'opacity-100',
      isMax ? 'stroke-warning-bright' : 'stroke-accent'
    )}
  />

  <!-- Inner core pip that grows with effort and blazes white-hot inside the ring on max -->
  <circle
    cx="12"
    cy="12"
    r={isMax ? 3.5 : fraction >= 1 ? 2.2 : fraction > 0 ? 1.5 : 0}
    class={cn(
      'duration-med origin-center transition-all ease-out-expo',
      fraction === 0 ? 'scale-0 opacity-0' : 'scale-100 opacity-100',
      isMax ? 'fill-max' : 'fill-accent'
    )}
  />
</svg>
