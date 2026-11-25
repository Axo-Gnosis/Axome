<!--
  @component
  PersonaCards — a new chat's persona choice: Default plus each persona as a radio card.

  @param agents - selectable personas, already filtered by the caller
  @param value - current persona name; null is Default
  @param disabled - lock the cards
  @param onSelect - called with the chosen persona name, or null for Default
-->
<script lang="ts" module>
  /** How the persona choices describe Default, which has no definition file. */
  export const DEFAULT_PERSONA_DESCRIPTION = "Axome's own agent with every tool."
</script>

<script lang="ts">
  import { Button } from '$lib/components/ui/button'
  import Icon from '$lib/icons/Icon.svelte'
  import type { ChatAgentInfo } from '$lib/types/backend'
  import { cn } from '$lib/utils/cn'
  import { UserIcon } from '@hugeicons/core-free-icons'

  let {
    agents,
    value,
    disabled,
    onSelect
  }: {
    agents: ChatAgentInfo[]
    value: string | null
    disabled: boolean
    onSelect: (agent: string | null) => void
  } = $props()

  const cards = $derived([
    { name: null, label: 'Default', description: DEFAULT_PERSONA_DESCRIPTION },
    ...agents.map((agent) => ({ name: agent.name, label: agent.name, description: agent.description }))
  ])
</script>

<div data-slot="persona-cards" class="flex w-full max-w-2xl flex-col items-center gap-3">
  <p class="text-sm text-muted">Who should work on this? You can change it later in Session.</p>
  <div
    role="radiogroup"
    aria-label="Persona"
    class="grid w-full grid-cols-[repeat(auto-fit,minmax(11rem,1fr))] gap-2.5"
  >
    {#each cards as card (card.label)}
      {@const selected = card.name === value}
      <Button
        variant="outline"
        role="radio"
        aria-checked={selected}
        {disabled}
        class={cn(
          'h-auto flex-col items-start gap-1 px-3 py-2.5 text-left font-normal',
          selected && 'border-accent bg-accent-bg'
        )}
        onclick={() => !selected && onSelect(card.name)}
      >
        <span class="flex items-center gap-1.5 text-bright"><Icon icon={UserIcon} size={14} />{card.label}</span>
        {#if card.description}<span class="line-clamp-2 text-xs text-muted">{card.description}</span>{/if}
      </Button>
    {/each}
  </div>
</div>
