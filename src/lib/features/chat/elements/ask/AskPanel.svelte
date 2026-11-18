<script lang="ts">
  import { tick, untrack } from 'svelte'
  import { ArrowDown01Icon, CheckIcon } from '@hugeicons/core-free-icons'
  import { Badge } from '$lib/components/ui/badge'
  import { Button, IconButton } from '$lib/components/ui/button'
  import { Checkbox } from '$lib/components/ui/checkbox'
  import {
    PaletteFooter,
    commandItemVariants,
    paletteSurfaceVariants,
    type PaletteHint
  } from '$lib/components/ui/command'
  import { Input, Textarea } from '$lib/components/ui/input'
  import { Kbd, KbdGroup } from '$lib/components/ui/kbd'
  import { StatusDot } from '$lib/components/ui/status-dot'
  import * as Tabs from '$lib/components/ui/tabs'
  import Icon from '$lib/icons/Icon.svelte'
  import type { ChatAnswer, ChatQuestion, ChatQuestions } from '$lib/types/backend'
  import { answerText, questionLabel } from './askResult'

  // Mounted inside `{#key questions.callId}`, so every piece of state below belongs to one ask call.
  let {
    questions,
    queued,
    typing,
    onAnswer,
    onDismiss
  }: {
    questions: ChatQuestions
    /** Further pending ask calls behind this one. */
    queued: number
    /** The composer holds a draft; never steal focus from it. */
    typing: boolean
    onAnswer: (callId: string, answers: ChatAnswer[]) => void
    onDismiss: (callId: string) => void
  } = $props()

  let root = $state<HTMLElement | null>(null)
  let index = $state(0)
  let highlight = $state(initialHighlight(0))
  let selected = $state<Record<string, string[]>>({})
  let custom = $state<Record<string, string>>({})
  // Multi-select questions count as answered once confirmed, since selecting none is a valid answer.
  let confirmed = $state<Record<string, true>>({})
  let otherOpen = $state(false)
  let minimized = $state(false)

  const list = $derived(questions.questions)
  const several = $derived(list.length > 1)
  const question = $derived(list[index])
  // Options plus the trailing Other item; free-text questions have no items.
  const other = $derived(question.options.length)
  const itemCount = $derived(question.options.length ? question.options.length + 1 : 0)
  const kicker = $derived(
    [
      question.header,
      question.multi && itemCount ? 'pick any' : null,
      several ? `${index + 1} of ${list.length}` : null
    ]
      .filter(Boolean)
      .join(' · ')
  )
  const previews = $derived(question.options.some((option) => option.preview))
  const last = $derived(list.every((q, i) => i === index || answered(q)))
  const picked = $derived(selected[question.id]?.length ?? 0)

  const hints = $derived.by((): PaletteHint[] => {
    const next = { keys: ['↵'], label: last ? 'answer' : 'next' }
    const body: PaletteHint[] = !itemCount
      ? [next, { keys: ['Shift', '↵'], label: 'new line' }]
      : question.multi
        ? [{ keys: ['Space'], label: 'toggle' }, next]
        : [{ keys: ['1', '–', String(itemCount)], label: 'pick' }, next]
    return [
      ...(several ? [{ keys: ['←', '→'], label: 'question' }] : []),
      ...(previews ? [{ keys: ['↑', '↓'], label: 'navigate, preview follows' }] : []),
      ...body
    ]
  })

  function initialHighlight(i: number): number {
    const q = questions.questions[i]
    return q.recommended !== null && q.recommended < q.options.length ? q.recommended : 0
  }

  function answered(q: ChatQuestion): boolean {
    return (selected[q.id]?.length ?? 0) > 0 || !!custom[q.id]?.trim() || (q.multi && !!confirmed[q.id])
  }

  /** The panel keeps focus while it moves between questions, unless the user is typing elsewhere. */
  function mayFocus(): boolean {
    const active = document.activeElement
    // Acting inside the panel (opening Other, changing question) always moves focus with it.
    if (root?.contains(active)) return true
    if (untrack(() => typing)) return false
    return !(active instanceof HTMLElement && active.closest('input, textarea, select, [contenteditable="true"]'))
  }

  const autofocus = (element: HTMLElement) => {
    if (mayFocus()) element.focus()
  }

  async function refocus() {
    await tick()
    if (root && !root.contains(document.activeElement)) root.focus()
  }

  export function open(): void {
    minimized = false
    void tick().then(() => root?.focus())
  }

  function go(i: number) {
    if (i < 0 || i >= list.length) return
    index = i
    const q = list[i]
    otherOpen = !!custom[q.id] && q.options.length > 0
    const chosen = q.options.findIndex((option) => selected[q.id]?.includes(option.label))
    highlight = otherOpen ? q.options.length : chosen >= 0 ? chosen : initialHighlight(i)
    void refocus()
  }

  function advance() {
    const next = list.findIndex((q, i) => i !== index && !answered(q))
    if (next >= 0) go(next)
    else
      onAnswer(
        questions.callId,
        list.map((q) => ({ id: q.id, selected: selected[q.id] ?? [], custom: custom[q.id]?.trim() || null }))
      )
  }

  function closeOther() {
    otherOpen = false
    custom[question.id] = ''
    void refocus()
  }

  function toggle(item: number) {
    if (item === other) {
      otherOpen = true
      return
    }
    const label = question.options[item].label
    const current = selected[question.id] ?? []
    selected[question.id] = current.includes(label) ? current.filter((l) => l !== label) : [...current, label]
  }

  /** Enter, or a click on an item; `item` is the highlighted one. */
  function enter(item: number) {
    if (!itemCount) {
      if (custom[question.id]?.trim()) advance()
    } else if (question.multi) {
      confirmed[question.id] = true
      advance()
    } else if (item === other) {
      if (!otherOpen) otherOpen = true
      else if (custom[question.id]?.trim()) {
        selected[question.id] = []
        advance()
      }
    } else {
      selected[question.id] = [question.options[item].label]
      custom[question.id] = ''
      otherOpen = false
      advance()
    }
  }

  function choose(item: number) {
    highlight = item
    if (question.multi) toggle(item)
    else enter(item)
  }

  // Delegated keys; handled ones stop here so the surface keymap does not move focus to the composer.
  function keydown(event: KeyboardEvent) {
    if (event.isComposing || event.ctrlKey || event.metaKey || event.altKey) return
    const target = event.target instanceof HTMLElement ? event.target : null
    const inField = !!target?.closest('input, textarea, select, [contenteditable="true"]')
    const inOther = !!target?.closest('[data-ask-other]')
    // Focused buttons and tabs keep native activation and arrow navigation.
    const onButton = !!target?.closest('button, [role="tab"]')
    const key = event.key
    let handled = true
    if (key === 'Escape') {
      if (inOther) closeOther()
      else onDismiss(questions.callId)
    } else if ((key === 'ArrowUp' || key === 'ArrowDown') && itemCount && !onButton) {
      highlight = (highlight + (key === 'ArrowDown' ? 1 : itemCount - 1)) % itemCount
    } else if ((key === 'ArrowLeft' || key === 'ArrowRight') && several && !inField && !onButton) {
      go(index + (key === 'ArrowRight' ? 1 : -1))
    } else if (/^[1-9]$/.test(key) && !inField && Number(key) <= itemCount) {
      if (question.multi) toggle(Number(key) - 1)
      else highlight = Number(key) - 1
    } else if (key === ' ' && question.multi && itemCount && !inField && !onButton) {
      toggle(highlight)
    } else if (key === 'Enter' && !event.shiftKey && !onButton) {
      enter(inOther ? other : highlight)
    } else handled = false
    if (!handled) return
    event.preventDefault()
    event.stopPropagation()
  }
</script>

{#snippet otherInput()}
  <Input
    data-ask-other
    aria-label="Other"
    placeholder="Type your own answer"
    value={custom[question.id] ?? ''}
    oninput={(event) => {
      custom[question.id] = event.currentTarget.value
    }}
    onfocus={() => (highlight = other)}
    {@attach autofocus}
  />
{/snippet}

{#snippet items()}
  <div
    role="listbox"
    aria-label={question.question}
    aria-multiselectable={question.multi}
    class="flex min-w-0 flex-col"
  >
    {#each [...question.options, { label: 'Other', description: 'type your own answer', preview: null }] as option, item (item)}
      {#if item === other && otherOpen}
        {@render otherInput()}
      {:else}
        {@const chosen =
          item === other ? !!custom[question.id]?.trim() : !!selected[question.id]?.includes(option.label)}
        <!-- svelte-ignore a11y_click_events_have_key_events (keys are delegated to the panel root) -->
        <div
          role="option"
          tabindex="-1"
          aria-selected={chosen}
          data-selected={highlight === item ? '' : undefined}
          class={commandItemVariants({ class: 'min-w-0' })}
          onpointermove={() => (highlight = item)}
          onclick={() => choose(item)}
        >
          <span
            class={[
              'inline-flex size-5 shrink-0 items-center justify-center rounded-md border bg-surface font-mono text-xs',
              highlight === item ? 'border-accent text-accent' : 'border-edge'
            ]}>{item + 1}</span
          >
          {#if question.multi}
            <Checkbox checked={chosen} tabindex={-1} aria-hidden="true" class="pointer-events-none" />
          {/if}
          <span class="shrink-0 text-base">{option.label}</span>
          {#if option.description}
            <span class="min-w-0 truncate text-sm text-muted">{option.description}</span>
          {/if}
          {#if question.recommended === item}
            <Badge variant="accent" class="ms-auto">Recommended</Badge>
          {/if}
        </div>
      {/if}
    {/each}
  </div>
{/snippet}

{#if minimized}
  <div data-slot="ask-panel" class={paletteSurfaceVariants({ class: 'mb-2 h-9 flex-row items-center gap-2 px-3' })}>
    <StatusDot tone="waiting" />
    <span class="shrink-0 text-sm text-bright">axo is waiting on your answer</span>
    <span class="min-w-0 flex-1 truncate text-sm text-muted">{question.question}</span>
    <KbdGroup><Kbd>Alt</Kbd><Kbd>A</Kbd></KbdGroup>
    <Button variant="accent" size="xs" onclick={open}>Answer</Button>
  </div>
{:else}
  <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
  <div
    bind:this={root}
    data-slot="ask-panel"
    role="group"
    aria-label="Answer questions"
    tabindex="-1"
    class={paletteSurfaceVariants({ class: 'mb-2 max-h-[min(26rem,50vh)] outline-none' })}
    onkeydown={keydown}
    {@attach (element) => {
      if (!element.contains(document.activeElement) && mayFocus()) element.focus()
    }}
  >
    <div class="flex h-9 shrink-0 items-center gap-2 border-b border-edge px-3">
      <StatusDot tone="waiting" />
      <span class="shrink-0 text-sm font-medium text-bright">
        {several ? `axo has ${list.length} questions` : 'axo needs an answer'}
      </span>
      <span class="min-w-0 flex-1 truncate font-mono text-xs text-muted">
        · asked by axo{queued > 0 ? ` · ${queued} more waiting` : ''}
      </span>
      {#if several}
        <Tabs.Root value={question.id} onValueChange={(value) => go(list.findIndex((q) => q.id === value))}>
          <Tabs.List>
            {#each list as q, i (q.id)}
              <Tabs.Trigger value={q.id} class="flex items-center gap-1">
                {questionLabel(q, i)}
                {#if answered(q)}<Icon icon={CheckIcon} size={12} />{/if}
              </Tabs.Trigger>
            {/each}
          </Tabs.List>
        </Tabs.Root>
      {/if}
      <IconButton tooltip="Minimize" onclick={() => (minimized = true)}>
        <Icon icon={ArrowDown01Icon} size={14} />
      </IconButton>
    </div>

    <div class="flex min-h-0 flex-col gap-2.5 overflow-y-auto p-3">
      {#if several}
        {#each list as q, i (q.id)}
          {#if i !== index && answered(q)}
            <div class="flex min-w-0 items-center gap-2 text-sm">
              <span class="shrink-0 font-mono text-xs text-muted">{questionLabel(q, i)}</span>
              <span class="min-w-0 truncate text-fg">
                {answerText(selected[q.id] ?? [], custom[q.id]?.trim() || null)}
              </span>
              <Button variant="text" size="xs" class="p-0" onclick={() => go(i)}>Change</Button>
            </div>
          {/if}
        {/each}
      {/if}
      <div class="flex min-w-0 flex-col gap-1">
        {#if kicker}<span class="font-mono text-xs text-muted">{kicker}</span>{/if}
        <span class="text-md font-semibold text-bright">{question.question}</span>
      </div>
      {#if !itemCount}
        <label class="flex flex-col gap-1.5 text-sm text-muted">
          Your answer
          <Textarea
            rows={3}
            value={custom[question.id] ?? ''}
            oninput={(event) => {
              custom[question.id] = event.currentTarget.value
            }}
            {@attach autofocus}
          />
        </label>
      {:else if previews}
        {@const preview = highlight < other ? question.options[highlight] : null}
        <div class="grid grid-cols-[1fr_1.15fr]">
          <div class="min-w-0 pe-2">{@render items()}</div>
          <div class="flex min-w-0 flex-col gap-2 border-l border-edge bg-surface p-3">
            <span class="font-mono text-xs text-muted">Preview{preview ? ` · ${preview.label}` : ''}</span>
            <pre class="m-0 font-mono text-xs whitespace-pre-wrap text-fg">{preview?.preview ?? ''}</pre>
          </div>
        </div>
      {:else}
        {@render items()}
      {/if}
    </div>

    {#if question.multi && itemCount}
      <div class="flex shrink-0 items-center justify-end gap-2 px-3 pb-2 text-xs text-muted">
        {picked} selected
        <Button variant="accent" size="xs" onclick={() => enter(highlight)}>Answer</Button>
      </div>
    {/if}
    <PaletteFooter {hints} navigate={!!itemCount && !previews} dismiss="dismiss" />
  </div>
{/if}
