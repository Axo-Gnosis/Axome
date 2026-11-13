<script lang="ts">
  import { untrack } from 'svelte'
  import { Portal } from '$lib/components/ui/portal'
  import { cn } from '$lib/utils/cn'
  import type { HTMLAttributes } from 'svelte/elements'
  import {
    FRAG_SHADER,
    hexToRgb,
    STIPPLE_FLOAT_UNIFORMS,
    STIPPLE_UNIFORMS,
    VERT_SHADER,
    type StippleFloatUniform
  } from './shader'
  import { resolveParams, STIPPLE_DEFAULTS, STIPPLE_KNOB_KEYS, STIPPLE_SHAPES, type StippleParams } from './params'
  import { STIPPLE_TUNER, stippleTuner } from './tuner.svelte'
  import StippleTuner from './StippleTuner.svelte'

  const OTHER_UNIFORMS = [
    'uInvRes',
    'uAspect',
    'uCell',
    'uBaseMaxR',
    'uKx',
    'uPhase',
    'uSwell',
    'uTime',
    'uShape',
    'uBg',
    'uBase',
    'uAccent'
  ] as const

  let {
    class: className,
    params,
    transitionMs = 0,
    ...rest
  }: Omit<HTMLAttributes<HTMLDivElement>, 'children'> & {
    class?: string
    /** Overrides on top of the defaults. Pass a stable reference: a new identity is what starts a transition. */
    params?: Partial<StippleParams>
    transitionMs?: number
  } = $props()

  let rootEl = $state<HTMLDivElement | null>(null)
  let canvasEl = $state<HTMLCanvasElement | null>(null)
  let canShowTuner = $state(false)

  const resolved = $derived(resolveParams(STIPPLE_TUNER ? stippleTuner : STIPPLE_DEFAULTS, params))

  let pushParams: ((next: StippleParams, presetChanged: boolean) => void) | null = null
  let lastParams: Partial<StippleParams> | undefined

  $effect(() => {
    const presetChanged = params !== lastParams
    lastParams = params
    pushParams?.(resolved, presetChanged)
  })

  $effect(() => {
    const root = rootEl
    const canvas = canvasEl
    if (!root || !canvas) return
    let current = untrack(() => resolved)

    const gl = canvas.getContext('webgl2', {
      alpha: true,
      antialias: false,
      depth: false,
      stencil: false,
      powerPreference: 'low-power'
    })
    if (!gl) return

    const compileShader = (type: number, src: string): WebGLShader | null => {
      const shader = gl.createShader(type)
      if (!shader) return null
      gl.shaderSource(shader, src)
      gl.compileShader(shader)
      if (!gl.getShaderParameter(shader, gl.COMPILE_STATUS)) {
        console.error('TidalStipple shader:', gl.getShaderInfoLog(shader))
        gl.deleteShader(shader)
        return null
      }
      return shader
    }

    const vert = compileShader(gl.VERTEX_SHADER, VERT_SHADER)
    const frag = compileShader(gl.FRAGMENT_SHADER, FRAG_SHADER)
    if (!vert || !frag) {
      if (vert) gl.deleteShader(vert)
      if (frag) gl.deleteShader(frag)
      return
    }

    const prog = gl.createProgram()
    if (!prog) {
      gl.deleteShader(vert)
      gl.deleteShader(frag)
      return
    }
    gl.attachShader(prog, vert)
    gl.attachShader(prog, frag)
    gl.linkProgram(prog)
    gl.deleteShader(vert)
    gl.deleteShader(frag)

    if (!gl.getProgramParameter(prog, gl.LINK_STATUS)) {
      console.error('TidalStipple link:', gl.getProgramInfoLog(prog))
      gl.deleteProgram(prog)
      return
    }

    gl.useProgram(prog)

    const buf = gl.createBuffer()
    gl.bindBuffer(gl.ARRAY_BUFFER, buf)
    gl.bufferData(gl.ARRAY_BUFFER, new Float32Array([-1, -1, 3, -1, -1, 3]), gl.STATIC_DRAW)
    const posLoc = gl.getAttribLocation(prog, 'aPos')
    gl.enableVertexAttribArray(posLoc)
    gl.vertexAttribPointer(posLoc, 2, gl.FLOAT, false, 0, 0)

    const loc = Object.fromEntries(
      [...OTHER_UNIFORMS, ...STIPPLE_FLOAT_UNIFORMS].map((name) => [name, gl.getUniformLocation(prog, name)])
    ) as Record<(typeof OTHER_UNIFORMS)[number] | StippleFloatUniform, WebGLUniformLocation | null>

    const motionQuery = window.matchMedia('(prefers-reduced-motion: reduce)')
    let reduced = motionQuery.matches

    let cw = 1
    let ch = 1
    let hasSize = false
    let isVisible = true

    const syncViewportUniforms = (p: StippleParams) => {
      const cell = Math.max((p.cellPx / 800) * ch * (p.scale / 100), 1.5)
      const reach = p.shape === 'square' ? Math.SQRT2 : 1
      const baseMaxR = (0.5 * cell * (Math.max(p.dotTop, p.dotBottom) / 100) * (1 + p.sizeJitter / 100) + 0.5) * reach
      gl.viewport(0, 0, cw, ch)
      gl.uniform2f(loc.uInvRes, 1 / cw, 1 / ch)
      gl.uniform1f(loc.uAspect, ch / Math.max(cw, 1))
      gl.uniform1f(loc.uCell, cell)
      gl.uniform1f(loc.uBaseMaxR, baseMaxR)
      gl.uniform1f(loc.uKx, (2 * Math.PI) / Math.max((p.waveLength / 100) * ch, 1))
    }

    const applyColorUniforms = (p: StippleParams) => {
      const styles = getComputedStyle(root)
      const set = (location: WebGLUniformLocation | null, token: string) => {
        const rgb = hexToRgb(styles.getPropertyValue(token))
        if (rgb) gl.uniform3f(location, ...rgb)
      }
      set(loc.uBg, '--color-ground')
      set(loc.uBase, p.baseColor)
      set(loc.uAccent, p.accentColor)
    }

    const applyParamUniforms = (p: StippleParams) => {
      gl.uniform1i(loc.uShape, STIPPLE_SHAPES.indexOf(p.shape))
      for (const name of STIPPLE_FLOAT_UNIFORMS) gl.uniform1f(loc[name], STIPPLE_UNIFORMS[name](p))
      syncViewportUniforms(p)
    }

    let tween: { from: StippleParams; to: StippleParams; start: number; duration: number } | null = null

    // seed is excluded: interpolating it would reshuffle the dot pattern every frame.
    const mix = (a: StippleParams, b: StippleParams, t: number): StippleParams => {
      const out = { ...b }
      for (const key of STIPPLE_KNOB_KEYS) if (key !== 'seed') out[key] = a[key] + (b[key] - a[key]) * t
      return out
    }

    pushParams = (next, presetChanged) => {
      if (next.baseColor !== current.baseColor || next.accentColor !== current.accentColor) applyColorUniforms(next)
      if (presetChanged && transitionMs > 0 && !reduced && next.shape === current.shape) {
        tween = { from: current, to: next, start: performance.now(), duration: transitionMs }
        return
      }
      tween = null
      current = next
      applyParamUniforms(current)
    }

    applyColorUniforms(current)
    applyParamUniforms(current)

    const onMotionChange = (e: MediaQueryListEvent) => {
      reduced = e.matches
    }
    motionQuery.addEventListener('change', onMotionChange)

    let clock = 0
    let raf = 0
    let last = -1

    const frame = (now: number) => {
      raf = 0
      if (!hasSize || !isVisible || document.hidden) {
        last = -1
        return
      }
      raf = requestAnimationFrame(frame)

      if (tween) {
        const raw = Math.min(1, (now - tween.start) / tween.duration)
        current = raw >= 1 ? tween.to : mix(tween.from, tween.to, 1 - Math.pow(1 - raw, 4))
        if (raw >= 1) tween = null
        applyParamUniforms(current)
      }

      const dt = last < 0 ? 0 : Math.min((now - last) / 1000, 1 / 20)
      last = now

      clock = (clock + dt * (current.speed / 50) * (reduced ? 0.25 : 1)) % 3600
      const phase = (2 * Math.PI * clock) / (current.tidePeriod / 1000)

      gl.uniform1f(loc.uTime, clock)
      gl.uniform1f(loc.uPhase, phase)
      gl.uniform1f(loc.uSwell, Math.sin(phase))
      gl.drawArrays(gl.TRIANGLES, 0, 3)
    }

    const scheduleFrame = () => {
      canShowTuner = hasSize && isVisible
      if (!raf && hasSize && isVisible && !document.hidden) {
        last = -1
        raf = requestAnimationFrame(frame)
      }
    }

    const resize = () => {
      const w = root.clientWidth
      const h = root.clientHeight
      if (w <= 0 || h <= 0) {
        hasSize = false
        canShowTuner = false
        return
      }
      hasSize = true
      const dpr = Math.min(window.devicePixelRatio || 1, 2)
      const nextW = Math.max(1, Math.round(w * dpr))
      const nextH = Math.max(1, Math.round(h * dpr))
      if (nextW !== cw || nextH !== ch || canvas.width !== nextW || canvas.height !== nextH) {
        cw = nextW
        ch = nextH
        canvas.width = cw
        canvas.height = ch
        syncViewportUniforms(current)
        gl.drawArrays(gl.TRIANGLES, 0, 3)
      }
      scheduleFrame()
    }

    const ro = new ResizeObserver(resize)
    ro.observe(root)
    resize()

    const io = new IntersectionObserver((entries) => {
      isVisible = entries[0]?.isIntersecting ?? true
      canShowTuner = hasSize && isVisible
      if (isVisible) scheduleFrame()
    })
    io.observe(root)

    const onVisibilityChange = () => {
      if (!document.hidden) scheduleFrame()
    }
    document.addEventListener('visibilitychange', onVisibilityChange)

    return () => {
      pushParams = null
      canShowTuner = false
      if (raf) cancelAnimationFrame(raf)
      ro.disconnect()
      io.disconnect()
      document.removeEventListener('visibilitychange', onVisibilityChange)
      motionQuery.removeEventListener('change', onMotionChange)
      gl.deleteBuffer(buf)
      gl.deleteProgram(prog)
      gl.getExtension('WEBGL_lose_context')?.loseContext()
    }
  })
</script>

<div
  data-slot="tidal-stipple"
  class={cn('pointer-events-none absolute inset-0 overflow-hidden', className)}
  bind:this={rootEl}
  aria-hidden="true"
  {...rest}
>
  <canvas bind:this={canvasEl} class="block h-full w-full"></canvas>
</div>

{#if STIPPLE_TUNER && canShowTuner}
  <Portal>
    <StippleTuner />
  </Portal>
{/if}
