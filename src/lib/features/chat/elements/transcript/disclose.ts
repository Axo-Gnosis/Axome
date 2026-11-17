import { quartOut } from 'svelte/easing'
import type { TransitionConfig } from 'svelte/transition'

/** Fast height + opacity disclosure that keeps the toggle anchored while content opens beneath it. */
export function disclose(node: Element, { duration = 150 }: { duration?: number } = {}): TransitionConfig {
  if (duration <= 0 || window.matchMedia('(prefers-reduced-motion: reduce)').matches) {
    return { duration: 0 }
  }
  const style = getComputedStyle(node)
  const height = parseFloat(style.height) || 0
  const paddingTop = parseFloat(style.paddingTop) || 0
  const paddingBottom = parseFloat(style.paddingBottom) || 0
  const opacity = parseFloat(style.opacity) || 1
  return {
    duration,
    easing: quartOut,
    css: (t) =>
      `overflow: hidden; height: ${t * height}px; padding-top: ${t * paddingTop}px; padding-bottom: ${t * paddingBottom}px; opacity: ${Math.min(1, t * 1.4) * opacity};`
  }
}
