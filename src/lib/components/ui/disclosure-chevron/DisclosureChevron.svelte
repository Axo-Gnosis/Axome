<script lang="ts">
  import { ChevronRightIcon } from '@hugeicons/core-free-icons'
  import Icon from '$lib/icons/Icon.svelte'
  import { cn } from '$lib/utils/cn'

  let { open, size = 14, class: className }: { open: boolean; size?: number; class?: string } = $props()
</script>

<Icon
  icon={ChevronRightIcon}
  {size}
  aria-hidden="true"
  class={cn('shrink-0 transition-transform', open && 'rotate-90', className)}
/>
