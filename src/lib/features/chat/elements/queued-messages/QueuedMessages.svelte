<script lang="ts">
  import { tick } from 'svelte'
  import * as Alert from '$lib/components/ui/alert'
  import { Button, IconButton } from '$lib/components/ui/button'
  import { DisclosureChevron } from '$lib/components/ui/disclosure-chevron'
  import PartChip from '$lib/features/chat/parts/PartChip.svelte'
  import { partsText } from '$lib/features/chat/parts/parts'
  import type { ChatController } from '$lib/features/chat/controller.svelte'
  import type { ChatQueued } from '$lib/types/backend'
  import Icon from '$lib/icons/Icon.svelte'
  import { cn } from '$lib/utils/cn'
  import {
    ArrowRight02Icon,
    Delete02Icon,
    Navigation03Icon,
    PencilEdit02Icon,
    Queue01Icon
  } from '@hugeicons/core-free-icons'

  let {
    controller,
    owner,
    onEdited
  }: {
    controller: ChatController
    owner: { folderPath: string; sessionId: string | null }
    onEdited?: () => void
  } = $props()

  const live = $derived(controller.state?.live)
  const queue = $derived(live?.queue ?? [])
  const running = $derived(live?.running ?? false)
  const autoPaused = $derived(live?.autoPaused ?? false)
  const disabled = $derived(controller.connection !== 'live')

  let open = $state(true)
  let busyId = $state<string | null>(null)

  const held = $derived(autoPaused && !running)
  const hasSteer = $derived(queue.some((item) => item.kind === 'steer'))
  const hasFollowUp = $derived(queue.some((item) => item.kind === 'followUp'))
  const summary = $derived(
    held
      ? 'Held until resumed or new input arrives'
      : running
        ? hasSteer && hasFollowUp
          ? 'Steers cut in next · follow-ups wait for turn end'
          : hasSteer
            ? 'Steers cut in after current step'
            : 'Follow-ups send when turn finishes'
        : 'Sending next'
  )

  async function act(submissionId: string, operation: () => Promise<void>) {
    if (disabled || busyId !== null) return
    busyId = submissionId
    try {
      await operation()
    } finally {
      busyId = null
    }
  }

  function sendTooltip(item: ChatQueued): string {
    if (held) return item.kind === 'followUp' ? 'Promote and resume now' : 'Resume and send held messages'
    return item.kind === 'followUp' ? 'Promote to steer' : 'Interrupt turn and send now'
  }
</script>

{#if autoPaused}
  <Alert.Root variant="warning" class="mb-2 flex items-center justify-between gap-3">
    <Alert.Description>
      {queue.length
        ? 'Paused after an error or stop. Held messages and background wakes wait for Resume.'
        : 'Background wakes paused after an error or stop.'}
    </Alert.Description>
    <Button variant="outline" size="xs" onclick={() => void controller.resumeWakes()}>Resume</Button>
  </Alert.Root>
{/if}
{#if queue.length}
  <section
    data-slot="queued-messages"
    aria-label="Queued messages"
    class="mb-2 overflow-hidden rounded-xl border border-edge bg-surface"
  >
    <button
      type="button"
      aria-expanded={open}
      onclick={() => (open = !open)}
      class="flex w-full cursor-pointer items-center gap-2 px-3 py-1.5 text-left transition-colors hover:bg-raised/40 focus-visible:shadow-focus-ring focus-visible:outline-none"
    >
      <span class="shrink-0 text-xs font-medium text-bright">Queued Messages</span>
      <span
        class="inline-flex h-4 min-w-4 shrink-0 items-center justify-center rounded-full border border-edge bg-raised px-1 font-mono text-2xs text-fg tabular-nums"
        >{queue.length}</span
      >
      <span class={cn('min-w-0 flex-1 truncate text-xs', held ? 'text-warning' : 'text-muted')}>{summary}</span>
      <DisclosureChevron {open} size={14} class="text-muted" />
    </button>

    {#if open}
      <ul class="divide-y divide-edge/60 border-t border-edge">
        {#each queue as item (item.submissionId)}
          {@const rowBusy = disabled || busyId === item.submissionId}
          <li class="flex items-start gap-2 px-3 py-1.5">
            <span
              class={cn(
                'mt-0.5 inline-flex shrink-0 items-center gap-1 rounded border px-1.5 py-0.5 text-2xs font-medium',
                held
                  ? 'border-warning/35 bg-warning-bg text-warning-bright'
                  : item.kind === 'steer'
                    ? 'border-accent/35 bg-accent-bg text-accent-bright'
                    : 'border-edge bg-raised text-muted'
              )}
            >
              <Icon icon={item.kind === 'steer' ? Navigation03Icon : Queue01Icon} size={11} />
              {item.kind === 'steer' ? 'Steer' : 'Follow-up'}
            </span>

            <div
              class="flex min-w-0 flex-1 flex-wrap items-center gap-1 pt-0.5 text-sm break-words whitespace-pre-wrap text-fg"
              aria-label={partsText(item.parts)}
            >
              {#each item.parts as part}
                {#if part.type === 'text'}{part.text}{:else}<PartChip {part} {owner} />{/if}
              {/each}
            </div>

            <div class="flex shrink-0 items-center gap-0.5">
              {#if running || held}
                <IconButton
                  size="sm"
                  class="hover:bg-raised"
                  tooltip={sendTooltip(item)}
                  tooltipSide="top"
                  disabled={rowBusy}
                  onclick={() => void act(item.submissionId, () => controller.sendQueuedNow(item))}
                >
                  <Icon icon={ArrowRight02Icon} size={13} />
                </IconButton>
              {/if}
              <IconButton
                size="sm"
                class="hover:bg-raised"
                tooltip="Edit in composer"
                tooltipSide="top"
                disabled={rowBusy}
                onclick={() =>
                  void act(item.submissionId, async () => {
                    if (await controller.editQueued(item)) {
                      await tick()
                      onEdited?.()
                    }
                  })}
              >
                <Icon icon={PencilEdit02Icon} size={13} />
              </IconButton>
              <IconButton
                size="sm"
                tone="danger"
                tooltip="Cancel queued message"
                tooltipSide="top"
                disabled={rowBusy}
                onclick={() =>
                  void act(item.submissionId, async () => {
                    await controller.withdraw(item.submissionId)
                  })}
              >
                <Icon icon={Delete02Icon} size={13} />
              </IconButton>
            </div>
          </li>
        {/each}
      </ul>
    {/if}
  </section>
{/if}
