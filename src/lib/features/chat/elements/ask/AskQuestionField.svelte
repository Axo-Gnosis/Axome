<script lang="ts">
  import { tick } from 'svelte'
  import { MinusSignIcon, PlusSignIcon } from '@hugeicons/core-free-icons'
  import { Badge } from '$lib/components/ui/badge'
  import { Button } from '$lib/components/ui/button'
  import { Checkbox } from '$lib/components/ui/checkbox'
  import { Input, Textarea } from '$lib/components/ui/input'
  import { Radio } from '$lib/components/ui/radio'
  import Icon from '$lib/icons/Icon.svelte'
  import { cn } from '$lib/utils/cn'
  import {
    askDecimalPlaces,
    askNumberStep,
    askNumberValue,
    buildQuestionAnswer,
    type AskQuestion,
    type AskQuestionDraft
  } from './askDocument'

  /** Small whole-number ranges render as a segmented picker instead of a stepper. */
  const MAX_SEGMENTS = 11

  let {
    question,
    draft,
    error,
    disabled,
    onChange
  }: {
    question: AskQuestion
    draft: AskQuestionDraft
    error: string | null
    disabled: boolean
    onChange: () => void
  } = $props()

  const uid = $props.id()
  const labelId = `${uid}-question`
  const errorId = `${uid}-error`
  const numberHelpId = `${uid}-number`
  const noteHelpId = `${uid}-note-help`
  let otherInput = $state<HTMLInputElement | null>(null)
  let noteInput = $state<HTMLTextAreaElement | null>(null)
  let otherToggle = $state<HTMLElement | null>(null)
  const numericValue = $derived(question.type === 'number' ? askNumberValue(draft.text) : null)
  const step = $derived(question.type === 'number' ? askNumberStep(question) : 1)
  const shiftStep = $derived(Number((step * 10).toPrecision(12)))
  const bounds = $derived.by(() => {
    if (question.type !== 'number') return { lower: -Infinity, upper: Infinity }
    const { min, max, integer } = question
    return {
      lower: min === null ? -Infinity : integer ? Math.ceil(min) : min,
      upper: max === null ? Infinity : integer ? Math.floor(max) : max
    }
  })
  const segments = $derived.by(() => {
    if (question.type !== 'number' || !question.integer) return null
    const { lower, upper } = bounds
    const count = upper - lower + 1
    if (!Number.isFinite(count) || count < 2 || count > MAX_SEGMENTS) return null
    return Array.from({ length: count }, (_, index) => lower + index)
  })
  const range = $derived.by(() => {
    if (question.type !== 'number') return null
    const { min, max } = question
    if (min !== null && max !== null) return `${min}–${max}`
    if (min !== null) return `min ${min}`
    return max !== null ? `max ${max}` : null
  })
  const meta = $derived.by(() => {
    const parts = question.optional ? ['optional'] : []
    if (question.type === 'choice') parts.push(question.multi ? 'pick any' : 'pick one')
    else if (question.type === 'number' && !segments) {
      parts.push(question.integer ? 'whole number' : 'any number')
      if (range) parts.push(range)
    }
    return parts.join(' · ')
  })
  const hint = $derived(
    question.type === 'number'
      ? [
          question.default !== null ? `Suggested: ${question.default}` : null,
          !segments && !question.integer ? 'decimals allowed' : null,
          segments ? null : `↑↓ step ${step}, Shift for ${shiftStep}`
        ]
          .filter(Boolean)
          .join(' · ')
      : ''
  )
  const canAddNote = $derived(question.type !== 'text' && !draft.noteOpen)
  const canNote = $derived.by(() => {
    if (question.type === 'text') return false
    const { error: questionError, hasAnswer } = buildQuestionAnswer(question, { ...draft, noteOpen: false })
    return !questionError && hasAnswer
  })

  function changed() {
    draft.skipped = false
    onChange()
  }

  function choose(value: string, checked: boolean, multi: boolean) {
    if (multi) {
      draft.selected = checked ? [...draft.selected, value] : draft.selected.filter((item) => item !== value)
    } else {
      draft.selected = [value]
      draft.otherOpen = false
      draft.other = ''
    }
    changed()
  }

  async function toggleOther(checked: boolean, multi: boolean) {
    draft.otherOpen = checked
    if (checked && !multi) draft.selected = []
    if (!checked) draft.other = ''
    changed()
    if (checked) {
      await tick()
      otherInput?.focus()
    }
  }

  function otherKey(event: KeyboardEvent) {
    if (event.key !== 'Escape' || event.isComposing) return
    event.preventDefault()
    event.stopPropagation()
    void toggleOther(false, true)
    otherToggle?.querySelector<HTMLElement>('input, button')?.focus()
  }

  function toggleSkip() {
    if (draft.skipped) draft.skipped = false
    else
      Object.assign(draft, {
        selected: [],
        text: '',
        otherOpen: false,
        other: '',
        noteOpen: false,
        note: '',
        skipped: true
      })
    onChange()
  }

  async function toggleNote() {
    draft.noteOpen = !draft.noteOpen
    if (!draft.noteOpen) draft.note = ''
    onChange()
    if (draft.noteOpen) {
      await tick()
      noteInput?.focus()
    }
  }

  function setNumber(value: number) {
    draft.text = String(value)
    changed()
  }

  function stepBy(direction: 1 | -1, large: boolean) {
    if (question.type !== 'number' || (numericValue === null && draft.text.trim())) return
    const { lower, upper } = bounds
    const start = numericValue ?? question.default ?? Math.max(lower, Math.min(upper, 0))
    const base = question.integer ? Math.round(start) : start
    const increment = large ? shiftStep : step
    const next = Math.max(lower, Math.min(upper, base + direction * increment))
    if (!Number.isFinite(next)) return
    const digits = Math.max(askDecimalPlaces(base), askDecimalPlaces(increment))
    const rounded = Number(next.toFixed(Math.min(100, digits)))
    const value = !rounded && next && digits > 100 ? next : rounded
    setNumber(Math.max(lower, Math.min(upper, value)))
  }

  function stepNumber(event: KeyboardEvent) {
    if (event.isComposing || event.altKey || event.ctrlKey || event.metaKey) return
    const direction = event.key === 'ArrowUp' ? 1 : event.key === 'ArrowDown' ? -1 : 0
    if (!direction) return
    event.preventDefault()
    stepBy(direction, event.shiftKey)
  }
</script>

{#snippet choiceControl(
  id: string,
  checked: boolean,
  described: string | undefined,
  multi: boolean,
  onPick: (checked: boolean) => void
)}
  {#if multi}
    <Checkbox
      {id}
      {checked}
      {disabled}
      aria-labelledby={`${id}-label`}
      aria-describedby={described}
      aria-invalid={!!error}
      onCheckedChange={onPick}
      class="mt-0.5"
    />
  {:else}
    <Radio
      {id}
      name={uid}
      {checked}
      aria-labelledby={`${id}-label`}
      aria-describedby={described}
      onchange={() => onPick(true)}
      class="mt-0.5"
    />
  {/if}
{/snippet}

{#snippet optionText(id: string, label: string, description: string | null, selected: boolean)}
  <span class="flex min-w-0 flex-1 flex-col">
    <span id={`${id}-label`} class={cn('wrap-anywhere', selected ? 'text-bright' : 'text-fg')}>{label}</span>
    {#if description}<span id={`${id}-description`} class="text-sm wrap-anywhere text-muted">{description}</span>{/if}
  </span>
{/snippet}

<fieldset
  data-slot="ask-question"
  data-ask-question-id={question.id}
  aria-labelledby={labelId}
  data-invalid={error ? '' : undefined}
  class={cn(
    'flex min-w-0 flex-col gap-2 rounded-lg border bg-surface px-3 pt-2.5 pb-2',
    error ? 'border-danger-border' : 'border-edge'
  )}
  {disabled}
>
  <div class="flex min-w-0 items-baseline justify-between gap-3">
    <span id={labelId} class="text-md font-semibold wrap-anywhere text-bright">{question.question}</span>
    {#if meta}<span class="shrink-0 font-mono text-xs text-muted">{meta}</span>{/if}
  </div>

  {#if question.type === 'choice'}
    <div
      role={question.multi ? undefined : 'radiogroup'}
      aria-labelledby={question.multi ? undefined : labelId}
      aria-invalid={question.multi ? undefined : !!error}
      class="-mx-1.5 flex min-w-0 flex-col gap-0.5"
    >
      {#each question.options as option, index (option.value)}
        {@const id = `${uid}-option-${index}`}
        {@const selected = draft.selected.includes(option.value)}
        {@const described =
          [option.description ? `${id}-description` : null, error ? errorId : null].filter(Boolean).join(' ') ||
          undefined}
        <label
          for={id}
          class={cn(
            'flex min-w-0 cursor-pointer items-start gap-2.5 rounded-md px-1.5 py-1.5',
            selected ? 'bg-accent-bg' : 'hover:bg-raised'
          )}
        >
          {@render choiceControl(id, selected, described, question.multi, (checked) =>
            choose(option.value, checked, question.multi)
          )}
          {@render optionText(id, option.label, option.description, selected)}
          {#if option.recommended}<Badge variant="accent" class="shrink-0">Recommended</Badge>{/if}
        </label>
      {/each}
      <div class={cn('min-w-0 rounded-md', draft.otherOpen ? 'bg-accent-bg' : 'hover:bg-raised')}>
        <label
          bind:this={otherToggle}
          for={`${uid}-other`}
          class="flex min-w-0 cursor-pointer items-start gap-2.5 px-1.5 py-1.5"
        >
          {@render choiceControl(
            `${uid}-other`,
            draft.otherOpen,
            error ? errorId : undefined,
            question.multi,
            (checked) => void toggleOther(checked, question.multi)
          )}
          {@render optionText(
            `${uid}-other`,
            'Other',
            draft.otherOpen ? null : 'Type your own answer',
            draft.otherOpen
          )}
        </label>
        {#if draft.otherOpen}
          <div class="ps-7.5 pe-1.5 pb-2">
            <Input
              bind:ref={otherInput}
              value={draft.other}
              placeholder="Type your own answer"
              aria-label="Other answer"
              aria-describedby={error ? errorId : undefined}
              aria-invalid={!!error}
              onkeydown={otherKey}
              oninput={(event) => {
                draft.other = event.currentTarget.value
                changed()
              }}
              class={cn('bg-ground py-1', error && 'border-danger-border')}
            />
          </div>
        {/if}
      </div>
    </div>
  {:else if question.type === 'text'}
    {#if question.multiline}
      <Textarea
        value={draft.text}
        placeholder={question.placeholder ?? undefined}
        rows={3}
        aria-labelledby={labelId}
        aria-describedby={error ? errorId : undefined}
        aria-invalid={!!error}
        oninput={(event) => {
          draft.text = event.currentTarget.value
          changed()
        }}
        class="bg-ground"
      />
    {:else}
      <Input
        value={draft.text}
        placeholder={question.placeholder ?? undefined}
        aria-labelledby={labelId}
        aria-describedby={error ? errorId : undefined}
        aria-invalid={!!error}
        oninput={(event) => {
          draft.text = event.currentTarget.value
          changed()
        }}
        class="bg-ground"
      />
    {/if}
  {:else if segments}
    <div
      role="radiogroup"
      aria-labelledby={labelId}
      aria-describedby={error ? errorId : hint ? numberHelpId : undefined}
      aria-invalid={!!error}
      class="flex w-fit max-w-full flex-wrap overflow-hidden rounded border border-edge bg-ground"
    >
      {#each segments as value (value)}
        <label class="border-e border-edge last:border-e-0">
          <!-- Native radios keep arrow-key selection; the span is the visible segment. -->
          <input
            type="radio"
            name={`${uid}-number`}
            class="peer sr-only"
            checked={numericValue === value}
            onchange={() => setNumber(value)}
          />
          <span
            class="block min-w-10 cursor-pointer px-3 py-1 text-center font-mono text-sm text-muted tabular-nums peer-checked:bg-accent-bg peer-checked:text-accent peer-focus-visible:shadow-focus-ring peer-disabled:cursor-default not-peer-checked:hover:bg-raised not-peer-checked:hover:text-bright"
            >{value}</span
          >
        </label>
      {/each}
    </div>
  {:else}
    <div
      class={cn(
        'flex w-fit items-stretch overflow-hidden rounded border bg-ground transition-colors focus-within:border-accent',
        error ? 'border-danger-border' : 'border-edge'
      )}
    >
      <Button
        variant="text"
        tabindex={-1}
        aria-label="Decrease"
        onclick={() => stepBy(-1, false)}
        class="h-auto w-7 border-e border-edge px-0"
      >
        <Icon icon={MinusSignIcon} size={12} />
      </Button>
      <Input
        type="text"
        inputmode="decimal"
        role="spinbutton"
        value={draft.text}
        aria-labelledby={labelId}
        aria-describedby={error ? errorId : numberHelpId}
        aria-invalid={!!error}
        aria-valuenow={numericValue ?? undefined}
        aria-valuemin={question.min ?? undefined}
        aria-valuemax={question.max ?? undefined}
        onkeydown={stepNumber}
        oninput={(event) => {
          draft.text = event.currentTarget.value
          changed()
        }}
        class="w-24 rounded-none border-0 bg-transparent px-2 py-1 text-end font-mono tabular-nums"
      />
      <Button
        variant="text"
        tabindex={-1}
        aria-label="Increase"
        onclick={() => stepBy(1, false)}
        class="h-auto w-7 border-s border-edge px-0"
      >
        <Icon icon={PlusSignIcon} size={12} />
      </Button>
    </div>
  {/if}

  {#if error || hint || question.optional || canAddNote}
    <div class="flex min-w-0 items-start justify-between gap-3 text-sm">
      {#if error}
        <p id={errorId} role="alert" class="min-w-0 wrap-anywhere text-danger-bright">{error}</p>
      {:else if hint}
        <p id={numberHelpId} class="min-w-0 text-muted">{hint}</p>
      {:else}
        <span></span>
      {/if}
      <div class="flex shrink-0 items-center gap-3">
        {#if question.optional}
          <Button
            variant="text"
            size="xs"
            aria-pressed={draft.skipped}
            onclick={toggleSkip}
            class="px-0 aria-pressed:text-accent">Skip</Button
          >
        {/if}
        {#if canAddNote}
          <Button variant="text" size="xs" disabled={!canNote} onclick={() => void toggleNote()} class="px-0"
            >Add a note</Button
          >
        {/if}
      </div>
    </div>
  {/if}

  {#if question.type !== 'text' && draft.noteOpen}
    <div class="flex min-w-0 flex-col gap-1.5 border-t border-edge pt-2.5">
      <label for={`${uid}-note`} class="text-sm text-fg">Note for axo</label>
      <Textarea
        id={`${uid}-note`}
        bind:ref={noteInput}
        value={draft.note}
        disabled={!canNote}
        rows={2}
        aria-describedby={`${noteHelpId}${error ? ` ${errorId}` : ''}`}
        aria-invalid={!!error}
        oninput={(event) => {
          draft.note = event.currentTarget.value
          changed()
        }}
        class="bg-ground"
      />
      <div class="flex min-w-0 items-center justify-between gap-3 text-sm">
        <span id={noteHelpId} class="text-muted">Sent along with your answer</span>
        <Button variant="text" size="xs" onclick={() => void toggleNote()} class="px-0">Remove note</Button>
      </div>
    </div>
  {/if}
</fieldset>
