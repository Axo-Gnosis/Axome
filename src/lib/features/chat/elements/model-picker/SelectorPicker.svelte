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
  import { joinSelector, modelForSelector, splitSelector, type SelectorParts } from './selector'

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

  const parts = $derived(splitSelector(value, models))
  const model = $derived(modelForSelector(value, models, roles))

  function matches(selector: string, wanted: SelectorParts): boolean {
    const actual = splitSelector(selector, models)
    return actual?.model === wanted.model && actual.effort === wanted.effort && actual.tier === wanted.tier
  }

  const savedSelector = $derived.by(() => {
    if (!parts) return value
    const canonical = joinSelector(parts.model, parts.effort, parts.tier)
    return matches(canonical, parts) ? canonical : value
  })

  function save(next: SelectorParts, original?: string) {
    const canonical = joinSelector(next.model, next.effort, next.tier)
    if (matches(canonical, next)) return onSelect(canonical)
    if (original && matches(original, next)) return onSelect(original)
    // Some model IDs are spelled like selectors. Never save options for a different model.
    if (next.tier && next.effort) {
      const reversed = `${next.model}:${next.tier}:${next.effort}`
      if (matches(reversed, next)) return onSelect(reversed)
    }
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
      const picked = splitSelector(next, models)
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
    value={parts?.effort ?? ''}
    levels={model?.thinking}
    tier={parts?.tier}
    serviceTiers={model?.serviceTiers}
    modelName={model?.name}
    tierDefaultLabel="Chat's tier"
    {savedSelector}
    defaultLabel="Default"
    disabled={disabled || !parts}
    onSelect={(level) => parts && save({ ...parts, effort: level || null })}
    onTierSelect={(tier) => parts && save({ ...parts, tier })}
  />
</div>
