<script lang="ts">
  import type { Snippet } from 'svelte'
  import SidebarHeader from './SidebarHeader.svelte'

  let {
    title,
    headerActions,
    children
  }: {
    title: string
    headerActions?: Snippet
    children?: Snippet
  } = $props()
</script>

<section data-slot="sidebar-division" aria-label={title} class="border-t border-edge">
  <SidebarHeader {title} actions={headerActions} />
  <div data-slot="sidebar-division-body">
    {#if children}{@render children()}{/if}
  </div>
</section>
