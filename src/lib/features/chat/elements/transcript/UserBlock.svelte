<script lang="ts" module>
  import type { ChatKeywordMatch } from '$lib/types/backend'

  // Sent messages never change, so their marks are fetched once per app run, keyed by entry;
  // remounts (scrolling, switching tabs) render them straight away.
  // ponytail: unbounded, a few bytes per message with keywords; goes away once the backend sends
  // match offsets with the user entry.
  const markCache = new Map<string, ChatKeywordMatch[][]>()
</script>

<script lang="ts">
  import { untrack } from 'svelte'
  import { Button, IconButton } from '$lib/components/ui/button'
  import { Chip } from '$lib/components/ui/chip'
  import * as Tooltip from '$lib/components/ui/tooltip'
  import { PromptInput } from '$lib/features/chat/elements/prompt-input'
  import { formatTime } from '$lib/features/chat/format'
  import PartChip from '$lib/features/chat/parts/PartChip.svelte'
  import SkillChip from '$lib/features/chat/parts/SkillChip.svelte'
  import { hasContent, invokedCommand, partsText } from '$lib/features/chat/parts/parts'
  import { backend } from '$lib/api/backend'
  import type { ChatCommand, ChatModel } from '$lib/types/backend'
  import type { ChatController } from '$lib/features/chat/controller.svelte'
  import type { UserEntry } from '$lib/features/chat/turns'
  import { ChevronLeftIcon, ChevronRightIcon, CopyIcon, GitForkIcon, PencilIcon } from '@hugeicons/core-free-icons'
  import Icon from '$lib/icons/Icon.svelte'
  import { copyToClipboard } from '$lib/utils/clipboard'
  import { cn } from '$lib/utils/cn'
  import { KeyHint } from '../key-hint'
  import { chatShortcut } from '$lib/features/chat/keymap/bindings.svelte'

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
      info: controller.state?.toolbox.keywords.find((keyword) => keyword.name === name) ?? null
    }))
  )
  // The text rendered for each part, '' for non-text parts so matches line up with part indexes.
  const texts = $derived(
    entry.body.parts.map((part, index) =>
      part.type !== 'text' ? '' : index === 0 && skill ? part.text.slice(skill.name.length + 1) : part.text
    )
  )
  /** Per part, the first match of each fired keyword across the message; null until loaded. */
  let marks = $state.raw<ChatKeywordMatch[][] | null>(null)
  // A string naming the entry, so a re-created entry or catalog for the same message does not refetch.
  // The skill is part of it: once commands resolve, the first text part loses the skill name and offsets shift.
  const key = $derived(`${owner.folderPath}\n${owner.sessionId}\n${entry.id}\n${skill?.name ?? ''}`)
  $effect(() => {
    const cacheKey = key
    const { fired, sent, folderPath, sessionId } = untrack(() => ({
      fired: entry.body.keywords,
      sent: texts,
      folderPath: owner.folderPath,
      sessionId: owner.sessionId
    }))
    let stale = false
    marks = markCache.get(cacheKey) ?? null
    if (marks || !fired.length || !sessionId) return
    backend.chat.keywordMatches(folderPath, sessionId, sent).then(
      (matches) => {
        const seen = new Set<string>()
        const found = sent.map((_, index) =>
          (matches[index] ?? []).filter((match) => {
            if (!fired.includes(match.name) || seen.has(match.name)) return false
            seen.add(match.name)
            return true
          })
        )
        markCache.set(cacheKey, found)
        if (!stale) marks = found
      },
      // Unlocatable: every fired keyword falls back to a chip.
      () => {
        if (!stale) marks = sent.map(() => [])
      }
    )
    return () => {
      stale = true
    }
  })
  const marked = $derived(new Set(marks?.flat().map((match) => match.name)))
  // Chips wait for the marks, so a keyword that will be underlined never shows as a chip first.
  const unmarked = $derived(marks ? keywords.filter((keyword) => !marked.has(keyword.name)) : [])

  /** Splits `text` at `matches` (UTF-16 offsets) into plain and keyword segments; overlaps keep the first. */
  function segments(text: string, matches: ChatKeywordMatch[]) {
    const out: { text: string; match: ChatKeywordMatch | null }[] = []
    let at = 0
    for (const match of [...matches].sort((a, b) => a.start - b.start)) {
      if (match.start < at) continue
      if (match.start > at) out.push({ text: text.slice(at, match.start), match: null })
      out.push({ text: text.slice(match.start, match.end), match })
      at = match.end
    }
    if (at < text.length) out.push({ text: text.slice(at), match: null })
    return out
  }
  const editing = $derived(controller.editing?.entryId === entry.id)
  const branchIndex = $derived(entry.body.siblings.indexOf(entry.id))
  const time = $derived(formatTime(entry.timestamp))

  $effect(() => {
    if (editing) input?.focus()
  })
</script>

{#snippet keywordMark(word: string, name: string)}
  {@const info = keywords.find((keyword) => keyword.name === name)?.info ?? null}
  <!-- No whitespace between trigger and content: it would render inside the message text. -->
  <Tooltip.Root>
    <Tooltip.Trigger>
      {#snippet child({ props })}
        <!-- svelte-ignore a11y_no_noninteractive_tabindex: focusable so keyboard users can open the tooltip -->
        <span
          {...props}
          tabindex={0}
          class="text-accent underline decoration-dotted underline-offset-4 focus-visible:shadow-focus-ring focus-visible:outline-none"
          >{word}</span
        >
      {/snippet}
    </Tooltip.Trigger><Tooltip.Content side="top" class="max-w-80">
      <div><span class="font-medium text-bright">{name}</span> · keyword</div>
      {#if info?.description}<div>{info.description}</div>{/if}
      {#if info?.model}<div class="text-muted">Switched the model to {info.model}</div>{/if}
      {#if info?.path}<div class="font-mono text-xs text-muted">{info.path}</div>{/if}
    </Tooltip.Content>
  </Tooltip.Root>
{/snippet}

<div data-slot="user-block" class="flex min-w-0 flex-col items-end gap-1">
  <div
    class={cn(
      'rounded-lg border border-edge bg-surface px-3 py-2.5',
      editing ? 'flex w-full flex-col gap-2' : 'max-w-4/5 min-w-0'
    )}
  >
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
          >Cancel <KeyHint keys={chatShortcut('leave')} /></Button
        >
        <Button
          type="button"
          variant="ghost"
          size="xs"
          disabled={!canBranch || !hasContent(controller.editing.parts)}
          onclick={() => void controller.submitEdit()}>Send <KeyHint keys={chatShortcut('send')} /></Button
        >
      </div>
    {:else}
      <div class="text-base break-words whitespace-pre-wrap text-fg">
        {#each entry.body.parts as part, index}{#if part.type === 'text'}{#if index === 0 && skill}<SkillChip
                name={skill.name}
                info={controller.state?.toolbox.skills.find((item) => item.name === skill.name) ?? null}
                {owner}
              />{/if}{#each segments(texts[index], marks?.[index] ?? []) as segment}{#if segment.match}{@render keywordMark(
                  segment.text,
                  segment.match.name
                )}{:else}{segment.text}{/if}{/each}{:else}<PartChip {part} {owner} interactive />{/if}{/each}
      </div>
      {#if unmarked.length}
        <div class="mt-2 flex flex-wrap gap-1.5">
          {#each unmarked as keyword (keyword.name)}
            {#if keyword.info?.description}
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
                <Tooltip.Content side="top">{keyword.info.description}</Tooltip.Content>
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
    {#if entry.body.from?.kind === 'agent'}
      <span class="text-muted">From {entry.body.from.name ?? 'parent agent'}</span>
    {:else if entry.body.from?.kind === 'extension'}
      <span class="text-muted">sent by <span class="font-mono">{entry.body.from.id}</span> ·</span>
    {/if}
    {#if time}<time datetime={entry.timestamp} class="font-mono tabular-nums">{time}</time>{/if}
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
