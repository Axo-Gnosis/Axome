<script lang="ts">
  import { IconButton } from '$lib/components/ui/button'
  import { CopyIcon } from '@hugeicons/core-free-icons'
  import Icon from '$lib/icons/Icon.svelte'
  import { copyToClipboard } from '$lib/utils/clipboard'
  import { highlightSpans, type HighlightSpan } from '$lib/utils/syntaxHighlight'

  let { command, inline = false }: { command: string; inline?: boolean } = $props()
  const renderedCommand = $derived(inline ? command.trim().split(/\r?\n/, 1)[0] : command)

  let highlighted = $state.raw<{ command: string; tokens: HighlightSpan[] } | null>(null)

  $effect(() => {
    const source = renderedCommand
    let cancelled = false
    async function highlight(): Promise<void> {
      const tokens = await highlightSpans(source, 'shellscript')
      if (cancelled || !tokens) return
      highlighted = { command: source, tokens }
    }
    void highlight().catch((cause) => console.warn('Bash syntax highlighting failed', cause))
    return () => {
      cancelled = true
    }
  })
</script>

{#snippet source()}
  {#if highlighted?.command === renderedCommand}{#each highlighted.tokens as token}<span style:color={token.color}
        >{token.text}</span
      >{/each}{:else}{renderedCommand}{/if}
{/snippet}

{#if inline}
  <code data-slot="bash-command-inline" class="block min-w-0 truncate font-mono text-xs text-fg"
    >{@render source()}{#if command.trim().includes('\n')}<span class="text-subtle"> …</span>{/if}</code
  >
{:else}
  <div data-slot="bash-command" class="flex min-w-0 items-start gap-2 font-mono text-xs leading-normal">
    <span class="shrink-0 text-subtle select-none" aria-hidden="true">$</span>
    <pre
      data-slot="bash-command-source"
      class="m-0 min-w-0 flex-1 cursor-text font-mono text-xs leading-normal wrap-anywhere whitespace-pre-wrap text-fg"><code
        >{@render source()}</code
      ></pre>
    <IconButton tooltip="Copy Command" class="shrink-0" onclick={() => void copyToClipboard(command)}
      ><Icon icon={CopyIcon} size={12} /></IconButton
    >
  </div>
{/if}
