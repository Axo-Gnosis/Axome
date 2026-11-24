<script lang="ts">
  import { MinusSignIcon, Tick02Icon } from '@hugeicons/core-free-icons'
  import Icon from '$lib/icons/Icon.svelte'

  /** One question and its answer; the enclosing question card draws the border. */
  let {
    question,
    answered,
    text,
    note,
    suggested = false
  }: {
    question: string
    answered: boolean
    text: string
    note: string | null
    suggested?: boolean
  } = $props()
</script>

<Icon
  icon={answered ? Tick02Icon : MinusSignIcon}
  size={12}
  class={answered ? 'mt-1 shrink-0 text-success' : 'mt-1 shrink-0 text-subtle'}
/>
<span class="flex min-w-0 flex-1 flex-col gap-0.5 text-start">
  <span class="text-sm wrap-anywhere text-muted">{question}</span>
  <span class="wrap-anywhere whitespace-pre-wrap {answered ? 'text-bright' : 'text-muted'}"
    >{text}{#if suggested}<span class="ms-2 font-mono text-xs text-subtle">suggested · no answer</span>{/if}</span
  >
  {#if note}<span class="text-sm wrap-anywhere whitespace-pre-wrap text-fg">Note: {note}</span>{/if}
</span>
