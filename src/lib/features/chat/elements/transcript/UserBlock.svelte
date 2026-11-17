<script lang="ts">
  import { Button, IconButton } from '$lib/components/ui/button'
  import { Chip } from '$lib/components/ui/chip'
  import * as Tooltip from '$lib/components/ui/tooltip'
  import { PromptInput } from '$lib/features/chat/elements/prompt-input'
  import PartChip from '$lib/features/chat/parts/PartChip.svelte'
  import { hasContent, invokedCommand, partsText } from '$lib/features/chat/parts/parts'
  import type { ChatCommand, ChatModel } from '$lib/types/backend'
  import type { ChatController } from '$lib/features/chat/controller.svelte'
  import type { UserEntry } from '$lib/features/chat/turns'
  import {
    ChevronLeftIcon,
    ChevronRightIcon,
    CopyIcon,
    GitForkIcon,
    PencilIcon,
    SparklesIcon
  } from '@hugeicons/core-free-icons'
  import Icon from '$lib/icons/Icon.svelte'
  import { copyToClipboard } from '$lib/utils/clipboard'
  import { cn } from '$lib/utils/cn'
  import { KeyHint } from '../key-hint'

  let {
    entry,
    controller,
    canBranch,
    owner,
    commands,
    models
  }: {
    entry: UserEntry
    controller: ChatController
    canBranch: boolean
    owner: { folderPath: string; sessionId: string | null }
    commands: ChatCommand[]
    models: ChatModel[]
  } = $props()

  let input = $state<PromptInput>()
  const skill = $derived(invokedCommand(entry.body.parts, commands, 'skill'))
  const keywords = $derived(
    entry.body.keywords.map((name) => ({
      name,
      description: controller.state?.toolbox.keywords.find((keyword) => keyword.name === name)?.description || null
    }))
  )
  const editing = $derived(controller.editing?.entryId === entry.id)
  const branchIndex = $derived(entry.body.siblings.indexOf(entry.id))
  const time = $derived(
    new Date(entry.timestamp).toLocaleTimeString([], {
      hour12: false,
      hour: '2-digit',
      minute: '2-digit',
      second: '2-digit'
    })
  )

  $effect(() => {
    if (editing) input?.focus()
  })
</script>

<div data-slot="user-block" class="flex min-w-0 flex-col items-end gap-1">
  <div class={cn('rounded-lg bg-surface px-4 py-3', editing ? 'flex w-full flex-col gap-2' : 'max-w-4/5 min-w-0')}>
    {#if editing && controller.editing}
      <PromptInput
        bind:this={input}
        bind:parts={controller.editing.parts}
        {owner}
        commands={commands.filter((command) => command.kind === 'skill')}
        {models}
        running={false}
        disabled={!canBranch}
        placeholder="Edit message"
        onSubmit={() => void controller.submitEdit()}
        onCancel={() => controller.cancelEdit()}
      />
      <div class="flex justify-end gap-2">
        <Button type="button" variant="ghost" size="xs" onclick={() => controller.cancelEdit()}
          >Cancel <KeyHint keys="Esc" /></Button
        >
        <Button
          type="button"
          variant="ghost"
          size="xs"
          disabled={!canBranch || !hasContent(controller.editing.parts)}
          onclick={() => void controller.submitEdit()}>Send <KeyHint keys="Enter" /></Button
        >
      </div>
    {:else}
      <div class="text-base break-words whitespace-pre-wrap text-fg">
        {#each entry.body.parts as part, index}{#if part.type === 'text'}{#if index === 0 && skill}<span
                class="inline-flex items-center gap-1 rounded-full border border-edge bg-raised px-2 py-0.5 text-xs text-accent"
                ><Icon icon={SparklesIcon} size={12} />{'/' + skill.name}</span
              >{part.text.slice(skill.name.length + 1)}{:else}{part.text}{/if}{:else}<PartChip
              {part}
              {owner}
              interactive
            />{/if}{/each}
      </div>
      {#if keywords.length}
        <div class="mt-2 flex flex-wrap gap-1.5">
          {#each keywords as keyword (keyword.name)}
            {#if keyword.description}
              <Tooltip.Root>
                <Tooltip.Trigger>
                  {#snippet child({ props })}
                    <Chip
                      {...props}
                      tone="accent"
                      tabindex={0}
                      class="font-mono focus-visible:shadow-focus-ring focus-visible:outline-none">{keyword.name}</Chip
                    >
                  {/snippet}
                </Tooltip.Trigger>
                <Tooltip.Content side="top">{keyword.description}</Tooltip.Content>
              </Tooltip.Root>
            {:else}
              <Chip tone="accent" class="font-mono">{keyword.name}</Chip>
            {/if}
          {/each}
        </div>
      {/if}
    {/if}
  </div>
  <div class="flex min-h-5 flex-wrap items-center justify-end gap-2 text-xs text-subtle">
    {#if entry.body.from}
      <span class="text-muted">From {entry.body.from.name ?? 'parent agent'}</span>
    {/if}
    <time datetime={entry.timestamp} class="font-mono tabular-nums">{time}</time>
    {#if entry.body.siblings.length > 1}
      <div class="flex items-center gap-1 font-mono" aria-label="Message branches">
        <IconButton
          tooltip="Previous Branch"
          disabled={!canBranch || branchIndex <= 0}
          onclick={() => void controller.selectBranch(entry.body.siblings[branchIndex - 1])}
        >
          <Icon icon={ChevronLeftIcon} size={12} />
        </IconButton>
        <span class="tabular-nums">{branchIndex + 1}/{entry.body.siblings.length}</span>
        <IconButton
          tooltip="Next Branch"
          disabled={!canBranch || branchIndex >= entry.body.siblings.length - 1}
          onclick={() => void controller.selectBranch(entry.body.siblings[branchIndex + 1])}
        >
          <Icon icon={ChevronRightIcon} size={12} />
        </IconButton>
      </div>
    {/if}
    <div class="flex items-center gap-1">
      <IconButton tooltip="Edit" class="text-subtle" disabled={!canBranch} onclick={() => controller.startEdit(entry)}
        ><Icon icon={PencilIcon} size={14} /></IconButton
      >
      <IconButton
        tooltip="Fork"
        class="text-subtle"
        disabled={!canBranch}
        onclick={() => void controller.fork(entry.id)}><Icon icon={GitForkIcon} size={14} /></IconButton
      >
      <IconButton tooltip="Copy" class="text-subtle" onclick={() => void copyToClipboard(partsText(entry.body.parts))}
        ><Icon icon={CopyIcon} size={14} /></IconButton
      >
    </div>
  </div>
</div>
