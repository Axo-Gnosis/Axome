<script lang="ts">
  import type { Snippet } from 'svelte'
  import { Button } from '$lib/components/ui/button'
  import { cn } from '$lib/utils/cn'

  let { onclick, class: className, children }: { onclick: () => void; class?: string; children: Snippet } = $props()
</script>

<Button
  type="button"
  variant="text"
  size="xs"
  class={cn('min-w-0 justify-start p-0 font-mono text-xs text-fg hover:text-bright hover:underline', className)}
  {onclick}
>
  <span class="truncate">{@render children()}</span>
</Button>
