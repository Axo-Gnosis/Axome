<!--
  @component
  SettingsSection — one titled block of a settings page. Lists itself under
  the page's sidebar entry while mounted; sections are separated by a hairline.

  @param title - section heading, also the sidebar sub-entry
  @param description - optional line under the heading; a snippet for inline code
  @param actions - optional controls at the heading's right (e.g. Refresh)
-->

<script lang="ts">
  import type { Snippet } from 'svelte'
  import { trackSection } from './state.svelte'

  let {
    title,
    description,
    actions,
    children
  }: {
    title: string
    description?: string | Snippet
    actions?: Snippet
    children?: Snippet
  } = $props()

  let id = $derived(title.toLowerCase().replace(/[^a-z0-9]+/g, '-'))
</script>

<section
  data-slot="settings-section"
  class="flex scroll-mt-8 flex-col gap-5 border-t border-edge py-8 first:border-t-0 first:pt-0"
  {@attach trackSection(id, title)}
>
  <header class="flex items-start justify-between gap-4">
    <div class="flex min-w-0 flex-col gap-1">
      <h2 class="text-xl font-semibold text-bright">{title}</h2>
      {#if typeof description === 'function'}
        <p class="text-sm text-muted">{@render description()}</p>
      {:else if description}
        <p class="text-sm text-muted">{description}</p>
      {/if}
    </div>
    {#if actions}<div class="flex shrink-0 items-center gap-2">{@render actions()}</div>{/if}
  </header>
  {#if children}{@render children()}{/if}
</section>
