<script lang="ts">
  import * as Tooltip from '$lib/components/ui/tooltip'
  import { Conversation } from '$lib/features/chat/elements/conversation'
  import { PromptInput } from '$lib/features/chat/elements/prompt-input'
  import { ChatKeys, setChatKeys } from '$lib/features/chat/keymap/router.svelte'
  import { TranscriptNav } from '$lib/features/chat/keymap/nav.svelte'
  let sent = $state(0)
  let conversation = $state<Conversation>()
  let composer = $state<PromptInput>()
  const keys = setChatKeys(new ChatKeys())
  const nav = new TranscriptNav(() => composer?.focus())
</script>

<Tooltip.Provider>
  <main class="flex h-screen flex-col bg-ground text-fg" {@attach keys.listen}>
    <button onclick={() => conversation?.scrollPage(-1)}>Page Up</button>
    <Conversation bind:this={conversation} {nav}>
      {#each Array.from({ length: 60 }, (_, index) => index + 1) as message}
        <p class="py-4" data-turn-id={message} {@attach nav.stop(`turn:${message}`)}>Chat message {message}</p>
      {/each}
      {#snippet footer()}
        <PromptInput
          bind:this={composer}
          parts={[{ type: 'text', text: 'Composer content' }]}
          placeholder="Message"
          commands={[]}
          owner={{ folderPath: '/repo/rust/Axome', sessionId: null }}
          onSubmit={() => sent++}
          onLeaveUp={() => nav.enter()}
        />
        <output class="sr-only">{sent}</output>
      {/snippet}
    </Conversation>
  </main>
</Tooltip.Provider>
