<script lang="ts">
  import * as Tooltip from '$lib/components/ui/tooltip'
  import { Conversation } from '$lib/features/chat/elements/conversation'
  import { PromptInput } from '$lib/features/chat/elements/prompt-input'
  let sent = $state(0)
  let conversation = $state<Conversation>()
</script>

<Tooltip.Provider>
  <main class="flex h-screen flex-col bg-ground text-fg">
    <button onclick={() => conversation?.scrollPage(-1)}>Page Up</button>
    <Conversation bind:this={conversation}>
      {#each Array.from({ length: 60 }, (_, index) => index + 1) as message}
        <p class="py-4">Chat message {message}</p>
      {/each}
      {#snippet footer()}
        <PromptInput
          parts={[{ type: 'text', text: 'Composer content' }]}
          placeholder="Message"
          commands={[]}
          owner={{ folderPath: '/repo/rust/Axome', sessionId: null }}
          onSubmit={() => sent++}
          onAltSubmit={() => sent++}
        />
        <output class="sr-only">{sent}</output>
      {/snippet}
    </Conversation>
  </main>
</Tooltip.Provider>
