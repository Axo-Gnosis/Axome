<script lang="ts">
  import type { Snippet } from 'svelte'
  import { Switch } from '$lib/components/ui/switch'
  import { Badge } from '$lib/components/ui/badge'
  import { AlertCircleIcon, ChevronDownIcon } from '@hugeicons/core-free-icons'
  import Icon from '$lib/icons/Icon.svelte'

  let {
    label,
    status,
    path,
    message,
    enabled,
    disabled,
    onEnabledChange,
    meta,
    children
  }: {
    label: string
    status: string
    path: string
    message?: string | null
    enabled: boolean
    disabled?: boolean
    onEnabledChange: (value: boolean) => void
    meta?: Snippet
    children: Snippet
  } = $props()
  let open = $state(false)
  const statusVariant = (status: string) =>
    status === 'connected' ? 'success' : status === 'missing' ? 'warning' : status === 'error' ? 'danger' : 'default'
</script>

<div>
  <div class="flex items-center gap-3 py-4">
    <Switch aria-label={label} checked={enabled} {disabled} onCheckedChange={onEnabledChange} />
    <button
      type="button"
      aria-expanded={open}
      onclick={() => (open = !open)}
      class="group flex min-w-0 flex-1 cursor-pointer items-center gap-3 text-left hover:bg-surface/40 focus-visible:shadow-focus-ring focus-visible:outline-none"
    >
      <span class="flex min-w-0 flex-1 flex-col gap-0.5">
        <span class="flex items-center gap-2">
          <span class="text-sm font-semibold text-bright">{label}</span>
          <Badge variant={statusVariant(status)}>{status}</Badge>
          {#if meta}{@render meta()}{/if}
        </span>
        <span class="truncate font-mono text-xs text-muted">{path}</span>
      </span>
      <Icon
        icon={ChevronDownIcon}
        size={14}
        class="duration-fast shrink-0 text-subtle transition-transform group-hover:text-fg {open ? 'rotate-180' : ''}"
      />
    </button>
  </div>
  {#if message}
    <div class="flex items-center gap-2 border-t border-danger/30 bg-danger-bg py-2 text-sm text-danger-bright">
      <Icon icon={AlertCircleIcon} size={14} /><span>{message}</span>
    </div>
  {/if}
  {#if open}<div class="flex flex-col gap-5 border-t border-edge py-5">{@render children()}</div>{/if}
</div>
