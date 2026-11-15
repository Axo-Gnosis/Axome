<script lang="ts">
  import type { Snippet } from 'svelte'
  import SidebarRow from './SidebarRow.svelte'

  let {
    children,
    ...rest
  }: {
    depth?: number
    class?: string
    children?: Snippet
  } = $props()
</script>

<SidebarRow behavior="display" density="compact" tone="muted" divider={false} {...rest}>
  {#if children}{@render children()}{/if}
</SidebarRow>
