<!--
  @component
  SidebarPanel — sidebar content under its own header (title, actions).

  @param title - panel heading (uppercase label)
  @param headerActions - snippet rendered right of the title (e.g. view action buttons)
  @param children - main content area
-->

<script lang="ts">
  import SidebarHeader from './SidebarHeader.svelte'
  import type { Snippet } from 'svelte'

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

<aside class="flex h-full flex-col bg-ground">
  <SidebarHeader {title} actions={headerActions} />
  <div class="min-h-0 flex-1 overflow-hidden">
    {@render children?.()}
  </div>
</aside>
