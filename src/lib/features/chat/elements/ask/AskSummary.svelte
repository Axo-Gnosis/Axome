<script lang="ts">
  import type { ToolStep } from '../../turns'
  import { askAnswers, askOutcome } from './askResult'

  let { call }: { call: ToolStep } = $props()

  const answers = $derived(askAnswers(call))
  const outcome = $derived(askOutcome(call))
  const settled = $derived(
    outcome.tone !== 'answered' && outcome.text
      ? outcome.text[0].toUpperCase() + outcome.text.slice(1)
      : answers.some((answer) => answer.timedOut)
        ? 'Auto-selected after timeout'
        : 'Answered by you'
  )
</script>

<div data-slot="ask-summary" class="flex min-w-0 flex-col gap-3 px-3 py-2">
  {#each answers as answer (answer.id)}
    <div class="flex min-w-0 flex-col gap-1">
      <span class="text-base text-fg">{answer.question}</span>
      {#if answer.options.length}
        <ol class="flex min-w-0 flex-col">
          {#each answer.options as option, index (option.label)}
            <li
              class={[
                'flex min-w-0 gap-2 px-2 py-1',
                answer.selected.includes(option.label) ? 'bg-accent-bg text-bright' : 'text-fg'
              ]}
            >
              <span class="w-4 shrink-0 text-muted">{index + 1}</span>
              <span class="min-w-0">
                {option.label}{#if option.description}<span class="text-muted"> : {option.description}</span>{/if}
              </span>
            </li>
          {/each}
        </ol>
      {/if}
      {#if answer.custom !== null}
        <span class="min-w-0 px-2 whitespace-pre-wrap text-fg">Other · "{answer.custom}"</span>
      {/if}
    </div>
  {/each}
  <span class="font-mono text-xs text-muted">{settled}</span>
</div>
