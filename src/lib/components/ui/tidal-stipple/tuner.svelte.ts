import { STIPPLE_DEFAULTS, type StippleParams } from './params'

/**
 * Dev tuner switch. When true, every visible TidalStipple mounts the tuner panel and uses the tuner state as its
 * base instead of STIPPLE_DEFAULTS; a caller's `params` still win, so knobs a caller sets are inert in the panel.
 */
export const STIPPLE_TUNER: boolean = false

export const stippleTuner = $state<StippleParams>({ ...STIPPLE_DEFAULTS })

export function resetStippleTuner(): void {
  Object.assign(stippleTuner, STIPPLE_DEFAULTS)
}
