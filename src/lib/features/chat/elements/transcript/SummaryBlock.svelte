<script lang="ts">
  import { Button } from '$lib/components/ui/button'
  import * as Message from '$lib/features/chat/elements/message'
  import { formatTokens } from '$lib/features/chat/format'
  import type { Step } from '$lib/features/chat/turns'
  import type { BlockView } from '$lib/features/chat/view'
  import { GitForkIcon, File01Icon } from '@hugeicons/core-free-icons'
  import Icon from '$lib/icons/Icon.svelte'

  let {
    step,
    view,
    onView,
    folderPath,
    sessionId
  }: {
    step: Extract<Step, { kind: 'compaction' | 'branchSummary' }>
    view: BlockView
    onView: (stepId: string, view: BlockView) => void
    folderPath: string
    sessionId: string | null
  } = $props()
</script>

<div data-slot="summary-block" data-view-id={step.id} class="min-w-0">
  <Button
    type="button"
    variant="text"
    size="xs"
    aria-expanded={view !== 'collapsed'}
    class="flex gap-2 px-0 py-1 text-sm text-muted"
    onclick={() => onView(step.id, view === 'collapsed' ? 'preview' : 'collapsed')}
  >
    <Icon icon={step.kind === 'compaction' ? File01Icon : GitForkIcon} size={14} />
    <span>{step.kind === 'compaction' ? `Compacted ${formatTokens(step.tokensBefore)} tokens` : 'Branch summary'}</span>
  </Button>
  {#if view !== 'collapsed'}
    <Message.Response text={step.summary} streaming={false} {folderPath} {sessionId} />
  {/if}
</div>
