<script lang="ts">
  import { formatDuration } from '$lib/features/chat/format'

  let { duration }: { duration: number | null } = $props()
</script>

<span data-slot="working-indicator" class="inline-flex min-w-0 items-center gap-2 text-sm whitespace-nowrap text-muted">
  <span aria-hidden="true" class="inline-flex shrink-0 items-center gap-1">
    {#each [0, 200, 400] as delay}
      <span
        class="size-1 animate-working-dot rounded-full bg-current motion-reduce:animate-none"
        style:animation-delay={`${delay}ms`}
      ></span>
    {/each}
  </span>
  <span
    class="animate-working-wave truncate bg-linear-to-r from-muted via-accent-bright to-muted bg-size-[200%_100%] bg-clip-text text-transparent tabular-nums motion-reduce:animate-none motion-reduce:bg-none motion-reduce:text-muted"
    >Working{duration !== null ? ` for ${formatDuration(duration)}` : ''}</span
  >
</span>
