<!--
  @component
  SelectorPicker — paired model + effort picker for axo selectors (`provider/model:effort` or `@role`).
-->
<script lang="ts">
  import type { ChatModel, ChatModelRole } from '$lib/types/backend'
  import { cn } from '$lib/utils/cn'
  import EffortPicker from './EffortPicker.svelte'
  import ModelPicker from './ModelPicker.svelte'
  import { joinSelector, levelsForSelector, splitSelector } from './selector'

  let {
    value,
    models,
    roles = [],
    none,
    modelLabel,
    effortLabel = `Effort for ${modelLabel}`,
    disabled = false,
    class: className,
    onSelect
  }: {
    value: string
    models: ChatModel[]
    roles?: ChatModelRole[]
    none?: string
    modelLabel: string
    effortLabel?: string
    disabled?: boolean
    class?: string
    onSelect: (selector: string) => void
  } = $props()

  const parts = $derived(splitSelector(value))
</script>

<div class={cn('flex min-w-0 items-center gap-2', className)}>
  <ModelPicker
    label={modelLabel}
    class="min-w-0 flex-1"
    value={parts.model}
    {models}
    {roles}
    {none}
    custom
    {disabled}
    onSelect={(next) => {
      const picked = splitSelector(next)
      onSelect(joinSelector(picked.model, picked.effort || parts.effort))
    }}
  />
  <EffortPicker
    label={effortLabel}
    value={parts.effort}
    levels={levelsForSelector(parts.model, models, roles)}
    defaultLabel="Default"
    disabled={disabled || !parts.model || parts.model.startsWith('@')}
    onSelect={(level) => onSelect(joinSelector(parts.model, level))}
  />
</div>
