<!--
  @component
  SelectorPicker — paired model and options controls for one axo selector.
-->
<script lang="ts">
  import { notify } from '$lib/features/notifications/state.svelte'
  import type { ChatModel, ChatModelRole } from '$lib/types/backend'
  import { cn } from '$lib/utils/cn'
  import EffortPicker from './EffortPicker.svelte'
  import ModelPicker from './ModelPicker.svelte'
  import { canonicalSelector, resolveSelector, type SelectorParts } from './selector'

  let {
    value,
    models,
    roles,
    none,
    modelLabel,
    disabled = false,
    class: className,
    onSelect
  }: {
    value: string
    models: ChatModel[]
    roles?: ChatModelRole[]
    none?: string
    modelLabel: string
    disabled?: boolean
    class?: string
    onSelect: (selector: string) => void
  } = $props()

  const resolved = $derived(resolveSelector(value, models, roles))
  const parts = $derived(resolved?.parts ?? null)
  const model = $derived(resolved?.model)
  const savedSelector = $derived(parts ? (canonicalSelector(parts, models, value) ?? value) : value)

  function save(next: SelectorParts, original?: string) {
    const target = canonicalSelector(next, models, original)
    if (target !== null) return onSelect(target)
    notify.error('Model options not saved', 'Another model ID takes precedence over this selector.')
  }
</script>

<div class={cn('flex min-w-0 items-center gap-2', className)}>
  <ModelPicker
    label={modelLabel}
    class="min-w-0 flex-1"
    {value}
    {models}
    {roles}
    {none}
    custom
    {disabled}
    onSelect={(next) => {
      if (!next) return onSelect('')
      const picked = resolveSelector(next, models)?.parts
      if (picked)
        save(
          {
            model: picked.model,
            effort: picked.effort ?? parts?.effort ?? null,
            tier: picked.tier ?? parts?.tier ?? null
          },
          next
        )
    }}
  />
  <EffortPicker
    label="Model options"
    value={parts?.effort ?? ''}
    levels={model?.thinking}
    tier={parts?.tier}
    serviceTiers={model?.serviceTiers}
    modelName={model?.name}
    tierDefaultLabel="Chat's tier"
    defaultLabel="Default"
    disabled={disabled || !parts}
    onSelect={(level) => parts && save({ ...parts, effort: level || null })}
    onTierSelect={(tier) => parts && save({ ...parts, tier })}
  >
    {#snippet tierNote()}
      {#if savedSelector}
        <p class="text-xs break-all text-muted">
          Saved as <span class="font-mono">{savedSelector}</span>
        </p>
      {/if}
    {/snippet}
  </EffortPicker>
</div>
