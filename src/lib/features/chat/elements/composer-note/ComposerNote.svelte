<!--
  @component
  ComposerNote — the one-line bar that sits just above the composer.

  @param tone - leading status dot; omit for none
  @param children - the note's text
  @param action - trailing control, pushed to the end
-->
<script lang="ts">
  import { StatusDot, type StatusTone } from '$lib/components/ui/status-dot'
  import type { Snippet } from 'svelte'

  let { tone, children, action }: { tone?: StatusTone; children: Snippet; action?: Snippet } = $props()
</script>

<div
  data-slot="composer-note"
  class="mb-1.5 flex min-h-8 min-w-0 items-center gap-x-2 rounded-lg border border-edge bg-surface py-1 ps-3 pe-1 text-sm text-muted"
>
  {#if tone}<StatusDot {tone} size="sm" />{/if}
  {@render children()}
  {#if action}<span class="ms-auto shrink-0">{@render action()}</span>{/if}
</div>
