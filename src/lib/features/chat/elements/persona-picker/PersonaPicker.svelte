<!--
  @component
  PersonaPicker — the composer's persona chooser: a ghost trigger showing the
  current persona (or Default) opening a radio menu of Default and the given
  personas, with a help tooltip beside it.

  @param agents - selectable personas, already filtered by the caller
  @param value - current persona name; null is Default
  @param disabled - lock the trigger
  @param onSelect - called with the chosen persona name, or null for Default
-->
<script lang="ts">
  import { Button } from '$lib/components/ui/button'
  import * as DropdownMenu from '$lib/components/ui/dropdown-menu'
  import { InfoTooltip } from '$lib/components/ui/tooltip'
  import Icon from '$lib/icons/Icon.svelte'
  import type { ChatAgentInfo } from '$lib/types/backend'
  import { ArrowDown01Icon } from '@hugeicons/core-free-icons'

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

  // Radio values are strings; '' stands for Default (no persona).
  function select(next: string) {
    const agent = next || null
    if (agent !== value) onSelect(agent)
  }
</script>

<div data-slot="persona-picker" class="flex items-center">
  <DropdownMenu.Root>
    <DropdownMenu.Trigger {disabled}>
      {#snippet child({ props })}
        <Button {...props} variant="ghost" size="sm" aria-label="Persona" {disabled} class="max-w-64 hover:bg-raised">
          <span class="truncate">{value ?? 'Default'}</span>
          <Icon icon={ArrowDown01Icon} size={12} />
        </Button>
      {/snippet}
    </DropdownMenu.Trigger>
    <DropdownMenu.Content side="top" align="start" class="max-w-80">
      <DropdownMenu.RadioGroup value={value ?? ''} onValueChange={select}>
        <DropdownMenu.RadioItem value="" textValue="Default">Default</DropdownMenu.RadioItem>
        {#each agents as agent (agent.name)}
          <DropdownMenu.RadioItem value={agent.name} textValue={agent.name}>
            <span class="flex min-w-0 flex-col">
              <span class="truncate">{agent.name}</span>
              {#if agent.description}<span class="line-clamp-2 text-xs text-muted">{agent.description}</span>{/if}
            </span>
          </DropdownMenu.RadioItem>
        {/each}
      </DropdownMenu.RadioGroup>
    </DropdownMenu.Content>
  </DropdownMenu.Root>
  <InfoTooltip ariaLabel="About personas" contentClass="max-w-64">
    A persona replaces the agent's instructions and tool set and may switch the model. Harness rules stay in place.
  </InfoTooltip>
</div>
