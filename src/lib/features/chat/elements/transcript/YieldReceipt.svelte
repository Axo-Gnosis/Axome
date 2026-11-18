<script lang="ts">
  import { Button } from '$lib/components/ui/button'
  import { chipVariants } from '$lib/components/ui/chip'
  import type { ToolStep } from '$lib/features/chat/turns'
  import { openOwnedPath } from '$lib/features/workbench/links/openLink'
  import type { ChatReview } from '$lib/types/backend'
  import { cn } from '$lib/utils/cn'
  import { record } from '../tool/payload'

  let {
    call,
    review,
    folderPath,
    sessionId,
    onOpenPlan
  }: {
    call: ToolStep
    /** The live review this `yield` is awaiting, if any. */
    review?: ChatReview
    folderPath: string
    sessionId: string | null
    onOpenPlan?: () => void
  } = $props()

  const details = $derived(record(call.details))
</script>

<!-- A pending yield has no details yet (axo journals them with the outcome); the live review names the mode. -->
<div data-slot="yield-receipt" class="flex min-w-0 items-center gap-2 text-sm text-muted">
  {#if review?.plan}
    <span class="truncate">Plan ready for review</span>
    {#if onOpenPlan}
      <Button type="button" size="xs" onclick={onOpenPlan}>Open Plan</Button>
    {/if}
  {:else if review}
    <span class="truncate">Awaiting review · {review.mode}</span>
  {:else if typeof details.mode === 'string'}
    {@const saved = details.outcome === 'accepted' ? record(details.result).file : undefined}
    <span class="truncate"
      >{`${details.mode} result${typeof details.outcome === 'string' ? ` · ${details.outcome}` : ''}`}</span
    >
    {#if typeof saved === 'string' && saved}
      <button
        type="button"
        class={cn(
          chipVariants(),
          'min-w-0 cursor-pointer truncate font-mono text-xs hover:text-bright focus-visible:shadow-focus-ring focus-visible:outline-none'
        )}
        title={saved}
        onclick={() => void openOwnedPath(saved, undefined, { folderPath, sessionId })}>{saved}</button
      >
    {/if}
  {:else}
    <span class="truncate">Yielded result</span>
  {/if}
</div>
