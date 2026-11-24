<script lang="ts">
  import { tick } from 'svelte'
  import { CheckIcon, MinusSignIcon, PlusSignIcon } from '@hugeicons/core-free-icons'
  import { Badge } from '$lib/components/ui/badge'
  import { Button } from '$lib/components/ui/button'
  import { DisclosureChevron } from '$lib/components/ui/disclosure-chevron'
  import { Input, Textarea } from '$lib/components/ui/input'
  import { chatShortcut } from '$lib/features/chat/keymap/bindings.svelte'
  import { getTranscriptNav, NAV_ACTIVE, NAV_FIELD_ACTIVE } from '$lib/features/chat/keymap/nav.svelte'
  import Icon from '$lib/icons/Icon.svelte'
  import { cn } from '$lib/utils/cn'
  import AskAnswerRow from './AskAnswerRow.svelte'
  import {
    askAnswerText,
    askDecimalPlaces,
    askNumberBounds,
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
    base,
    onChange,
    onSettle
  }: {
    question: AskQuestion
    draft: AskQuestionDraft
    error: string | null
    disabled: boolean
    /** Prefix of this question's navigation stop ids. */
    base: string
    onChange: () => void
    /** Collapse the question if its answer is complete; `advance` moves on to the next open one. */
    onSettle: (advance: boolean) => void
  } = $props()

  const nav = getTranscriptNav()
  const uid = $props.id()
  const labelId = `${uid}-question`
  const errorId = `${uid}-error`
  const numberHelpId = `${uid}-number`
  const noteHelpId = `${uid}-note-help`
  let root = $state<HTMLElement | null>(null)
  let otherInput = $state<HTMLInputElement | null>(null)
  let noteInput = $state<HTMLTextAreaElement | null>(null)
  let lineInput = $state<HTMLInputElement | null>(null)
  let areaInput = $state<HTMLTextAreaElement | null>(null)
  const numericValue = $derived(question.type === 'number' ? askNumberValue(draft.text) : null)
  const step = $derived(question.type === 'number' ? askNumberStep(question) : 1)
  const bounds = $derived(
    question.type === 'number' ? askNumberBounds(question) : { lower: -Infinity, upper: Infinity }
  )
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
  const stepKeys = $derived([chatShortcut('decrease'), chatShortcut('increase')].filter(Boolean).join(' '))
  const hint = $derived(
    question.type === 'number'
      ? [
          question.default !== null ? `Suggested: ${question.default}` : null,
          !segments && !question.integer ? 'decimals allowed' : null,
          stepKeys ? `${stepKeys} to ${segments ? 'pick' : `step ${step}`}` : null
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
  const settled = $derived.by(() => {
    if (!draft.collapsed) return null
    const { answer, hasAnswer } = buildQuestionAnswer(question, draft)
    return {
      answered: hasAnswer,
      text: hasAnswer ? askAnswerText(question, answer) : draft.skipped ? 'skipped' : 'no answer',
      note: answer.note ?? null
    }
  })

  /** Every action is inert while the document cannot be answered. */
  const live =
    <A extends unknown[]>(action: (...args: A) => void) =>
    (...args: A) => {
      if (!disabled) action(...args)
    }

  async function focusStop(id: string, edit = false) {
    await tick()
    const stop = root?.querySelector<HTMLElement>(`[data-nav-stop="${CSS.escape(id)}"]`)
    if (stop) nav.focus(stop, { edit })
  }

  function changed() {
    draft.skipped = false
    onChange()
  }

  function choose(value: string) {
    if (question.type !== 'choice') return
    if (question.multi) {
      draft.selected = draft.selected.includes(value)
        ? draft.selected.filter((item) => item !== value)
        : [...draft.selected, value]
    } else {
      draft.selected = [value]
      draft.otherOpen = false
      draft.other = ''
    }
    changed()
  }

  function setOther(open: boolean) {
    draft.otherOpen = open
    if (open && question.type === 'choice' && !question.multi) draft.selected = []
    if (!open) draft.other = ''
    changed()
    if (open) void focusStop(`${base}:other-text`, true)
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
    if (draft.skipped) onSettle(false)
  }

  function toggleNote() {
    draft.noteOpen = !draft.noteOpen
    if (!draft.noteOpen) draft.note = ''
    onChange()
    if (draft.noteOpen) void focusStop(`${base}:note`, true)
  }

  function reopen() {
    draft.collapsed = false
    void focusStop(entryStop())
  }

  /** The stop a reopened question lands on: its picked option, else its first stop. */
  function entryStop(): string {
    if (question.type === 'choice') {
      const picked = question.options.findIndex((option) => draft.selected.includes(option.value))
      return draft.otherOpen && picked < 0 ? `${base}:other` : `${base}:option:${Math.max(0, picked)}`
    }
    if (question.type === 'text') return `${base}:text`
    return segments ? `${base}:segments` : `${base}:number`
  }

  function setNumber(value: number) {
    draft.text = String(value)
    changed()
  }

  function stepBy(direction: 1 | -1) {
    if (question.type !== 'number' || (numericValue === null && draft.text.trim())) return
    const { lower, upper } = bounds
    const start = numericValue ?? question.default ?? Math.max(lower, Math.min(upper, 0))
    const base = question.integer ? Math.round(start) : start
    const next = Math.max(lower, Math.min(upper, base + direction * step))
    if (!Number.isFinite(next)) return
    const digits = Math.max(askDecimalPlaces(base), askDecimalPlaces(step))
    const rounded = Number(next.toFixed(Math.min(100, digits)))
    const value = !rounded && next && digits > 100 ? next : rounded
    setNumber(Math.max(lower, Math.min(upper, value)))
  }

  /** `detail` is 0 for clicks the keyboard synthesizes; only pointer clicks pick and collapse. */
  const pointer = (event: MouseEvent) => event.detail > 0
</script>

<!--
  Options, rows and segments are navigation stops, not tab stops: the transcript holds focus and points
  at the highlighted one (aria-activedescendant). Keys reach them through ChatKeys; clicks act directly.
-->

{#snippet indicator(checked: boolean, multi: boolean)}
  <span
    aria-hidden="true"
    class={cn(
      'mt-0.5 flex size-3.5 shrink-0 items-center justify-center border transition-colors',
      multi ? 'rounded-sm' : 'rounded-full',
      checked ? (multi ? 'border-accent bg-accent text-ground' : 'border-accent') : 'border-edge bg-surface'
    )}
  >
    {#if checked && multi}<Icon icon={CheckIcon} size={10} strokeWidth={3} />{/if}
    {#if checked && !multi}<span class="size-1.5 rounded-full bg-accent"></span>{/if}
  </span>
{/snippet}

{#snippet optionText(id: string, label: string, description: string | null, selected: boolean)}
  <span class="flex min-w-0 flex-1 flex-col">
    <span id={`${id}-label`} class={cn('wrap-anywhere', selected ? 'text-bright' : 'text-fg')}>{label}</span>
    {#if description}<span class="text-sm wrap-anywhere text-muted">{description}</span>{/if}
  </span>
{/snippet}

{#if settled}
  <!-- svelte-ignore a11y_click_events_have_key_events, a11y_interactive_supports_focus -->
  <div bind:this={root} data-slot="ask-question" data-ask-question-id={question.id} data-collapsed="">
    <div
      id={`${uid}-row`}
      role="button"
      aria-label={`${question.question}: ${settled.text}. Change answer`}
      {@attach nav.stop(`${base}:row`, () => ({ activate: live(reopen), toggle: live(reopen) }))}
      onclick={live(reopen)}
      class="flex w-full min-w-0 cursor-pointer items-start gap-2.5 px-3 py-2 hover:bg-raised data-[nav-active]:bg-raised"
    >
      <AskAnswerRow question={question.question} answered={settled.answered} text={settled.text} note={settled.note} />
      <DisclosureChevron open={false} size={14} class="mt-1 shrink-0 text-muted" />
    </div>
  </div>
{:else}
  <fieldset
    bind:this={root}
    data-slot="ask-question"
    data-ask-question-id={question.id}
    aria-labelledby={labelId}
    class={cn(
      'flex min-w-0 flex-col gap-2 px-3 pt-2.5 pb-2',
      error && 'shadow-[inset_2px_0_0_var(--color-danger-border)]'
    )}
    {disabled}
  >
    <div class="flex min-w-0 items-baseline justify-between gap-3">
      <span id={labelId} class="text-md font-semibold wrap-anywhere text-bright">{question.question}</span>
      {#if meta}<span class="shrink-0 font-mono text-xs text-muted">{meta}</span>{/if}
    </div>

    {#if question.type === 'choice'}
      {@const multi = question.multi}
      <div
        role={multi ? 'group' : 'radiogroup'}
        aria-labelledby={labelId}
        aria-describedby={error ? errorId : undefined}
        class="-mx-1.5 flex min-w-0 flex-col gap-0.5"
      >
        {#each question.options as option, index (option.value)}
          {@const id = `${uid}-option-${index}`}
          {@const selected = draft.selected.includes(option.value)}
          <!-- svelte-ignore a11y_click_events_have_key_events, a11y_interactive_supports_focus -->
          <div
            {id}
            role={multi ? 'checkbox' : 'radio'}
            aria-checked={selected}
            aria-labelledby={`${id}-label`}
            {@attach nav.stop(`${base}:option:${index}`, () => ({
              activate: live(() => {
                if (!multi) choose(option.value)
                onSettle(true)
              }),
              toggle: live(() => choose(option.value))
            }))}
            onclick={live((event: MouseEvent) => {
              if (!pointer(event)) return
              if (multi || !selected) choose(option.value)
              if (!multi) onSettle(false)
            })}
            class={cn(
              'flex min-w-0 cursor-pointer items-start gap-2.5 rounded-md px-1.5 py-1.5',
              selected ? 'bg-accent-bg' : 'hover:bg-raised',
              NAV_ACTIVE
            )}
          >
            {@render indicator(selected, multi)}
            {@render optionText(id, option.label, option.description, selected)}
            {#if option.recommended}<Badge variant="accent" class="shrink-0">Recommended</Badge>{/if}
          </div>
        {/each}
        <div class={cn('min-w-0 rounded-md', draft.otherOpen ? 'bg-accent-bg' : 'hover:bg-raised')}>
          <!-- svelte-ignore a11y_click_events_have_key_events, a11y_interactive_supports_focus -->
          <div
            id={`${uid}-other`}
            role={multi ? 'checkbox' : 'radio'}
            aria-checked={draft.otherOpen}
            aria-labelledby={`${uid}-other-label`}
            {@attach nav.stop(`${base}:other`, () => ({
              activate: live(() => {
                if (!draft.otherOpen) setOther(true)
                else onSettle(true)
              }),
              toggle: live(() => setOther(!draft.otherOpen))
            }))}
            onclick={live((event: MouseEvent) => pointer(event) && setOther(!draft.otherOpen))}
            class={cn('flex min-w-0 cursor-pointer items-start gap-2.5 rounded-md px-1.5 py-1.5', NAV_ACTIVE)}
          >
            {@render indicator(draft.otherOpen, multi)}
            {@render optionText(
              `${uid}-other`,
              'Other',
              draft.otherOpen ? null : 'Type your own answer',
              draft.otherOpen
            )}
          </div>
          {#if draft.otherOpen}
            <div
              class={cn('ps-7.5 pe-1.5 pb-2', NAV_FIELD_ACTIVE)}
              {@attach nav.stop(`${base}:other-text`, () => ({
                activate: live(() => onSettle(true)),
                field: () => otherInput
              }))}
            >
              <Input
                bind:ref={otherInput}
                value={draft.other}
                placeholder="Type your own answer"
                aria-label="Other answer"
                aria-describedby={error ? errorId : undefined}
                aria-invalid={!!error}
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
      <div
        class={NAV_FIELD_ACTIVE}
        {@attach nav.stop(`${base}:text`, () => ({
          activate: live(() => onSettle(true)),
          field: () => areaInput ?? lineInput
        }))}
      >
        {#if question.multiline}
          <Textarea
            bind:ref={areaInput}
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
            bind:ref={lineInput}
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
      </div>
    {:else if segments}
      {@const values = segments}
      <div
        role="radiogroup"
        aria-labelledby={labelId}
        aria-describedby={error ? errorId : hint ? numberHelpId : undefined}
        class={cn('w-fit max-w-full', NAV_FIELD_ACTIVE)}
        {@attach nav.stop(`${base}:segments`, () => ({
          activate: live(() => onSettle(true)),
          step: live((direction: -1 | 1) => {
            const at = numericValue === null ? -1 : values.indexOf(numericValue)
            const next =
              at < 0
                ? question.type === 'number'
                  ? (question.default ?? values[0])
                  : values[0]
                : values[at + direction]
            if (next !== undefined) setNumber(next)
          })
        }))}
      >
        <div class="flex flex-wrap overflow-hidden rounded border border-edge bg-ground">
          {#each values as value (value)}
            {@const checked = numericValue === value}
            <!-- svelte-ignore a11y_click_events_have_key_events, a11y_interactive_supports_focus -->
            <div
              role="radio"
              aria-checked={checked}
              onclick={live((event: MouseEvent) => {
                if (!pointer(event)) return
                setNumber(value)
                onSettle(false)
              })}
              class={cn(
                'min-w-10 cursor-pointer border-e border-edge px-3 py-1 text-center font-mono text-sm tabular-nums last:border-e-0',
                checked ? 'bg-accent-bg text-accent' : 'text-muted hover:bg-raised hover:text-bright'
              )}
            >
              {value}
            </div>
          {/each}
        </div>
      </div>
    {:else}
      <div
        class={cn('w-fit', NAV_FIELD_ACTIVE)}
        {@attach nav.stop(`${base}:number`, () => ({
          activate: live(() => onSettle(true)),
          step: live(stepBy),
          field: () => lineInput
        }))}
      >
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
            onclick={() => stepBy(-1)}
            class="h-auto w-7 border-e border-edge px-0"
          >
            <Icon icon={MinusSignIcon} size={12} />
          </Button>
          <Input
            bind:ref={lineInput}
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
            onclick={() => stepBy(1)}
            class="h-auto w-7 border-s border-edge px-0"
          >
            <Icon icon={PlusSignIcon} size={12} />
          </Button>
        </div>
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
            <Button variant="text" size="xs" disabled={!canNote} onclick={toggleNote} class="px-0">Add a note</Button>
          {/if}
        </div>
      </div>
    {/if}

    {#if question.type !== 'text' && draft.noteOpen}
      <div class="flex min-w-0 flex-col gap-1.5 border-t border-edge pt-2.5">
        <label for={`${uid}-note`} class="text-sm text-fg">Note for axo</label>
        <div
          class={NAV_FIELD_ACTIVE}
          {@attach nav.stop(`${base}:note`, () => ({
            activate: live(() => onSettle(true)),
            field: () => noteInput
          }))}
        >
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
        </div>
        <div class="flex min-w-0 items-center justify-between gap-3 text-sm">
          <span id={noteHelpId} class="text-muted">Sent along with your answer</span>
          <Button variant="text" size="xs" onclick={toggleNote} class="px-0">Remove note</Button>
        </div>
      </div>
    {/if}
  </fieldset>
{/if}
