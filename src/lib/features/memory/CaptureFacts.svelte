<script lang="ts">
  import { Button } from '$lib/components/ui/button'
  import type { MemoryCapturedFact } from '$lib/types/backend'
  import { cn } from '$lib/utils/cn'
  import { openNote, revertFact, revertLabel, type CaptureRun } from './captures.svelte'

  let { run, hostFolder }: { run: CaptureRun; hostFolder: string | null } = $props()

  let reverting = $state<number | null>(null)

  const OUTCOME = {
    new: { label: 'new', reverted: 'forgotten', tone: 'text-success' },
    updated: { label: 'updated', reverted: 'undone', tone: 'text-accent' },
    known: { label: 'known', reverted: 'known', tone: 'text-muted' }
  } as const

  async function revert(fact: MemoryCapturedFact) {
    reverting = fact.id
    try {
      await revertFact(hostFolder, fact.id)
    } finally {
      reverting = null
    }
  }
</script>

<div data-slot="capture-facts" class="overflow-hidden rounded-md border border-edge bg-surface">
  {#each run.facts as fact (fact.id)}
    {@const outcome = OUTCOME[fact.outcome]}
    {@const revertAction = revertLabel(fact)}
    <div
      class="grid grid-cols-[62px_minmax(0,1fr)_auto] items-baseline gap-x-3 border-t border-edge px-2.5 py-2 text-sm first:border-t-0"
    >
      <span class={cn('font-mono text-xs', fact.reverted ? 'text-muted' : outcome.tone)}
        >{fact.reverted ? outcome.reverted : outcome.label}</span
      >
      <div class="flex min-w-0 flex-col gap-0.5">
        {#if fact.outcome === 'updated' && fact.previous}
          <span class="text-muted line-through">{fact.previous}</span>
        {/if}
        <span class={fact.reverted ? 'text-muted line-through' : 'text-fg'}>{fact.text}</span>
        <span class="font-mono text-xs text-muted"
          >{fact.category} ·
          <button
            type="button"
            class="cursor-pointer underline underline-offset-2 hover:text-bright focus-visible:shadow-focus-ring focus-visible:outline-none"
            onclick={() => void openNote(hostFolder, fact)}>{fact.note}</button
          ></span
        >
      </div>
      <div class="flex items-center gap-3">
        <Button variant="text" size="xs" onclick={() => void openNote(hostFolder, fact)}>Open Note</Button>
        {#if revertAction}
          <Button variant="text" size="xs" disabled={reverting !== null} onclick={() => void revert(fact)}
            >{revertAction}</Button
          >
        {/if}
      </div>
    </div>
  {/each}
</div>
