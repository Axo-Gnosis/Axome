export const STIPPLE_SHAPES = ['diamond', 'circle', 'square', 'cross'] as const
export type StippleShape = (typeof STIPPLE_SHAPES)[number]

/** Every numeric knob in slider units. STIPPLE_UNIFORMS in shader.ts owns the conversion to shader units. */
export const STIPPLE_KNOBS = {
  intensity: { default: 40, min: 0, max: 100 },
  density: { default: 45, min: 0, max: 100 },
  scale: { default: 100, min: 10, max: 300 },
  dotTop: { default: 5, min: 5, max: 200 },
  dotBottom: { default: 155, min: 5, max: 200 },
  speed: { default: 50, min: 0, max: 100 },
  tide: { default: 16, min: 0, max: 20 },
  seed: { default: 7, min: 0, max: 999 },
  cellPx: { default: 4, min: 2, max: 40 },
  jitter: { default: 6, min: 0, max: 100 },
  sizeJitter: { default: 10, min: 0, max: 50 },
  topDensity: { default: 25, min: 0, max: 300 },
  peakDensity: { default: 100, min: 50, max: 150 },
  curveStart: { default: 30, min: -50, max: 100 },
  curveEnd: { default: 95, min: 0, max: 150 },
  curveExp: { default: 275, min: 20, max: 400 },
  growWidth: { default: 29, min: 0, max: 50 },
  edgeSoft: { default: 50, min: 5, max: 300 },
  colorVar: { default: 8, min: 0, max: 50 },
  rimDarken: { default: 10, min: 0, max: 80 },
  glint: { default: 18, min: 0, max: 100 },
  shimmerTop: { default: 8, min: 0, max: 50 },
  shimmerBottom: { default: 4, min: 0, max: 50 },
  grain: { default: 0, min: 0, max: 100 },
  vignette: { default: 0, min: 0, max: 100 },
  tidePeriod: { default: 7250, min: 1000, max: 30000 },
  waveLength: { default: 150, min: 20, max: 400 },
  waveMix: { default: 70, min: 0, max: 100 },
  crest: { default: 30, min: 0, max: 100 }
} as const satisfies Record<string, { default: number; min: number; max: number }>

export type StippleKnob = keyof typeof STIPPLE_KNOBS
export const STIPPLE_KNOB_KEYS = Object.keys(STIPPLE_KNOBS) as StippleKnob[]

/** Colours are theme token names (`--color-*`) whose value in src/app.css is `#rrggbb`. */
export type StippleParams = Record<StippleKnob, number> & {
  shape: StippleShape
  baseColor: string
  accentColor: string
}

export const STIPPLE_DEFAULTS: StippleParams = {
  shape: 'cross',
  baseColor: '--color-warm',
  accentColor: '--color-accent',
  ...(Object.fromEntries(STIPPLE_KNOB_KEYS.map((key) => [key, STIPPLE_KNOBS[key].default])) as Record<
    StippleKnob,
    number
  >)
}

/** Overrides win over base; every knob is clamped to its range, non-finite values fall back to the default. */
export function resolveParams(base: StippleParams, overrides?: Partial<StippleParams>): StippleParams {
  const out: StippleParams = {
    ...base,
    shape: overrides?.shape ?? base.shape,
    baseColor: overrides?.baseColor ?? base.baseColor,
    accentColor: overrides?.accentColor ?? base.accentColor
  }
  for (const key of STIPPLE_KNOB_KEYS) {
    const { default: fallback, min, max } = STIPPLE_KNOBS[key]
    const value = overrides?.[key] ?? base[key]
    out[key] = Math.max(min, Math.min(max, Number.isFinite(value) ? value : fallback))
  }
  return out
}
