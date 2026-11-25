<script lang="ts">
  import { onMount, tick } from 'svelte'
  import FolderHero from '$lib/components/layout/FolderHero.svelte'
  import * as Alert from '$lib/components/ui/alert'
  import { BlurFade } from '$lib/components/ui/blur-fade'
  import { Button, IconButton } from '$lib/components/ui/button'
  import { TidalStipple, type StippleParams } from '$lib/components/ui/tidal-stipple'
  import * as chats from '$lib/features/chat/registry'
  import { getChatModels, MODELS_HINT } from '$lib/features/chat/models.svelte'
  import { chatShortcut } from '$lib/features/chat/keymap/bindings.svelte'
  import { ChatKeys, setChatKeys, type KeyScope } from '$lib/features/chat/keymap/router.svelte'
  import { TranscriptNav } from '$lib/features/chat/keymap/nav.svelte'
  import { ASK_ENTRY } from '$lib/features/chat/elements/ask/askDocument'
  import { modelName } from '$lib/features/chat/format'
  import { pendingSetup, transcriptTurns } from '$lib/features/chat/turns'
  import { isActiveJob, transcriptJobs } from '$lib/features/chat/jobs'
  import type { BlockView } from '$lib/features/chat/view'
  import { Conversation } from '$lib/features/chat/elements/conversation'
  import { UserBlock, AgentBlock } from '$lib/features/chat/elements/transcript'
  import { groupSteps, segmentSteps, turnFoldId } from '$lib/features/chat/elements/transcript/groupSteps'
  import BackgroundBand from '$lib/features/chat/elements/transcript/BackgroundBand.svelte'
  import RetryRow from '$lib/features/chat/elements/transcript/RetryRow.svelte'
  import {
    isShown,
    loadCaptures,
    reveal,
    revealsIn,
    sessionCaptures,
    type CaptureRun
  } from '$lib/features/memory/captures.svelte'
  import ExtensionDiagnostics from '$lib/features/chat/elements/transcript/ExtensionDiagnostics.svelte'
  import { PromptInput } from '$lib/features/chat/elements/prompt-input'
  import { QueuedMessages } from '$lib/features/chat/elements/queued-messages'
  import { askCountdown } from '$lib/features/chat/elements/ask/askDocument'
  import { fileReference } from '$lib/features/chat/elements/prompt-input/attachments'
  import { ModelPicker, EffortPicker, THINKING_LEVELS } from '$lib/features/chat/elements/model-picker'
  import { ComposerNote } from '$lib/features/chat/elements/composer-note'
  import { PersonaCards } from '$lib/features/chat/elements/persona-cards'
  import { backend } from '$lib/api/backend'
  import { hasContent } from '$lib/features/chat/parts/parts'
  import { isImagePart } from '$lib/features/chat/parts/image'
  import { selectFile } from '$lib/features/browser/state.svelte'
  import { notify } from '$lib/features/notifications/state.svelte'
  import type { ChatTab } from '$lib/features/workbench/model'
  import { PaperclipIcon } from '@hugeicons/core-free-icons'
  import Icon from '$lib/icons/Icon.svelte'
  import { getErrorMessage } from '$lib/utils/client-error'
  import SessionSidebar from '$lib/features/chat/sidebar/SessionSidebar.svelte'
  import { SessionSidebarState } from '$lib/features/chat/sidebar/state.svelte'

  let { tab }: { tab: ChatTab } = $props()
  const controller = $derived(chats.getOrCreate(tab.id))
  const chat = $derived(controller.state)
  $effect(() => controller.holdExtensions())
  const models = $derived(getChatModels(tab.folderPath))
  const catalog = $derived(models.catalog)
  const available = $derived(catalog?.models ?? [])
  const turns = $derived(chat ? transcriptTurns(chat.entries, chat.streaming, chat.live) : [])
  const jobIndex = $derived(transcriptJobs(turns))
  const captures = $derived(sessionCaptures(tab.folderPath, tab.sessionId).filter(isShown))
  // Compaction captures join their compaction's band; the rest sit after the last turn that started before them.
  const placedCaptures = $derived.by(() => {
    const compactions = new Map<string, string>()
    for (const turn of turns)
      for (const step of turn.steps) if (step.kind === 'compaction') compactions.set(step.id, turn.id)
    const anchored = new Map<string, CaptureRun[]>()
    const after = new Map<string, CaptureRun[]>()
    for (const run of captures) {
      const anchor = run.trigger === 'compaction' && run.anchorEntryId ? compactions.get(run.anchorEntryId) : undefined
      if (anchor) {
        anchored.set(anchor, [...(anchored.get(anchor) ?? []), run])
        continue
      }
      const started = Date.parse(run.startedAt)
      const turn = turns.findLast((turn) => turn.startedAt && Date.parse(turn.startedAt) <= started) ?? turns[0]
      if (turn) after.set(turn.id, [...(after.get(turn.id) ?? []), run])
    }
    return { anchored, after }
  })
  const running = $derived(chat?.live.running ?? false)
  const pending = $derived(chat && !chat.isChild ? pendingSetup(turns, chat.live) : [])
  const model = $derived(chat?.live.model ?? null)
  const current = $derived(available.find((item) => item.selector === model))
  const modelUnavailable = $derived(!!model && !!catalog && !current)
  const thinking = $derived(chat?.live.thinking ?? 'off')
  const levels = $derived(current?.thinking ?? THINKING_LEVELS)
  const disabled = $derived(controller.connection !== 'live' || !model || !!chat?.live.maintenance)
  const modelDisabled = $derived(disabled || !available.length)
  const canSend = $derived(!disabled && !modelUnavailable && hasContent(controller.draft))
  const imageWarning = $derived(current?.images === false && controller.draft.some(isImagePart))
  const owner = $derived({ folderPath: tab.folderPath, sessionId: tab.sessionId })
  const canBranch = $derived(controller.connection === 'live' && !controller.busy)
  // Personas and keywords belong to the root session; a child transcript offers neither.
  const root = $derived(!!chat && !chat.isChild)
  const personas = $derived(
    root ? (chat?.toolbox.agents ?? []).filter((a) => !a.team && (a.runAs === 'root' || a.runAs === 'any')) : []
  )
  const keywordNames = $derived(root ? (chat?.toolbox.keywords ?? []).map((k) => k.name) : [])
  const keywordMatches = $derived.by(() => {
    const { folderPath, sessionId } = tab
    if (!root || !sessionId) return undefined
    return (texts: string[]) => backend.chat.keywordMatches(folderPath, sessionId, texts)
  })
  const catalogMessage = $derived(
    models.error
      ? `${models.error} ${MODELS_HINT}`
      : catalog && !available.length
        ? `No axo models are available on this host. ${MODELS_HINT}`
        : null
  )
  let expansion = $state<BlockView>('collapsed')
  let overrides = $state<Record<string, BlockView>>({})
  let conversation = $state<Conversation>()
  let composer = $state<PromptInput>()
  const keys = setChatKeys(new ChatKeys())
  const nav = new TranscriptNav(() => composer?.focus())
  let visibleAskIds = $state<string[]>([])
  const pendingAsks = $derived(chat?.live.questions ?? [])
  const orderedAsks = $derived.by(() => {
    if (!pendingAsks.length) return []
    const pending = new Map(pendingAsks.map((ask) => [ask.callId, ask]))
    const ordered = []
    for (const turn of turns) {
      for (const step of turn.steps) {
        if (step.kind !== 'tool' || step.name !== 'ask') continue
        const ask = pending.get(step.callId)
        if (ask) {
          ordered.push(ask)
          pending.delete(step.callId)
        }
      }
    }
    return [...ordered, ...pending.values()]
  })
  const hiddenAsk = $derived(orderedAsks.find((ask) => !visibleAskIds.includes(ask.callId)))
  const questionCount = $derived(
    pendingAsks.reduce((count, ask) => count + ask.blocks.filter((block) => block.type !== 'markdown').length, 0)
  )
  const onAskVisibility = (ids: string[]) => (visibleAskIds = ids)
  let modelOpen = $state(false)
  let now = $state(Date.now())
  const sidebar = new SessionSidebarState()
  // Calmer field once a conversation has turns. Module-stable: TidalStipple transitions when this identity appears or goes.
  const ACTIVE_STIPPLE: Partial<StippleParams> = {
    intensity: 30,
    density: 42,
    tide: 4,
    topDensity: 5,
    dotBottom: 100,
    curveStart: 85,
    waveLength: 300,
    crest: 20
  }

  onMount(() => {
    if (controller.connection === 'detached') void reconnect()
  })

  $effect(() => {
    if (tab.sessionId) void loadCaptures(tab.folderPath, { sessionId: tab.sessionId, bank: null, limit: 500 })
  })

  $effect(() => {
    if (
      !running &&
      !(chat?.live.jobs ?? []).some((j) => isActiveJob(j.status)) &&
      !pendingAsks.some((ask) => ask.deadlineMs !== null)
    )
      return
    now = Date.now()
    const timer = setInterval(() => (now = Date.now()), 1000)
    return () => clearInterval(timer)
  })
  async function reconnect() {
    // attach stores the failure for the closed-connection status line.
    await controller.attach().catch(() => {})
  }

  function onSubmit() {
    if (canSend) return controller.send().then(() => conversation?.scrollToBottom())
  }

  // Operations resolve to an error message; show it like any other axo failure.
  function report(operation: Promise<string | null>) {
    void operation.then((error) => {
      if (error) notify.error('axo', error)
    })
  }

  function configure(name: string, args: Record<string, unknown>) {
    report(controller.runCommand(name, args))
  }

  async function stop() {
    try {
      await controller.stop()
    } catch (error) {
      notify.error('Stop chat failed', getErrorMessage(error))
    }
  }

  // `@` covers project files; the paperclip browses the whole machine the chat runs on.
  async function attachFile() {
    const picked = await selectFile({ path: tab.folderPath })
    if (picked) composer?.insertPart({ type: 'file', path: fileReference(picked.path, tab.folderPath, picked.folder) })
  }

  function onOpenMemory() {
    sidebar.view = 'memory'
  }

  // Review of one of this chat's captures: open its band and scroll to it. A request that can't resolve to a band
  // yet (turns or captures still loading) stays pending until it can.
  $effect(() => {
    const captureId = revealsIn(tab.folderPath, tab.sessionId)
    const run = captureId === null ? undefined : captures.find((run) => run.id === captureId)
    const id = run && bandOf(run)
    if (!id) return
    reveal.current = null
    overrides[id] = 'preview'
    void tick().then(() => conversation?.jumpToView(id))
  })

  /** The id of the band showing a capture run, mirroring where `placedCaptures` puts it. */
  function bandOf(run: CaptureRun): string | null {
    for (const [turnId, runs] of placedCaptures.after) if (runs.includes(run)) return `band:after:${turnId}`
    for (const turn of turns)
      for (const step of groupSteps(turn.steps))
        if (step.kind === 'band' && step.steps.some((s) => s.id === run.anchorEntryId)) return step.id
    return null
  }

  /** Open the turn's work, the step and the tool group holding a step, then scroll to it. */
  async function revealStep(stepId: string) {
    for (const turn of turns) {
      for (const segment of segmentSteps(groupSteps(turn.steps), jobIndex)) {
        if (segment.kind !== 'work') continue
        for (const step of segment.steps) {
          const group = step.kind === 'toolGroup' && step.calls.some((call) => call.id === stepId) ? step : null
          if (step.id !== stepId && !group) continue
          overrides[turnFoldId(turn.id)] = 'preview'
          overrides[segment.id] = 'preview'
          if (group) overrides[`tool-group:${group.id}`] = 'preview'
          overrides[stepId] = 'preview'
          await tick()
          conversation?.jumpToView(stepId)
          return
        }
      }
    }
  }

  function onView(stepId: string, view: BlockView) {
    const change = () => {
      overrides[stepId] = view
    }
    if (conversation) void conversation.preserveScroll(change, stepId)
    else change()
  }

  /** Moves the keyboard to an ask's first open question. */
  function enterAsk(callId: string) {
    nav.enter(`[data-ask-call-id="${CSS.escape(callId)}"]`, ASK_ENTRY)
  }

  // Commands that work anywhere in the chat view; inner scopes (composer, transcript) go first.
  const surface: KeyScope = {
    commands: {
      cycleThinking: () => {
        if (disabled || !levels.length) return false
        configure('thinking', { level: levels[(levels.indexOf(thinking) + 1) % levels.length] })
      },
      pickModel: () => {
        if (modelDisabled) return false
        modelOpen = true
      },
      cycleDetails: (event) => {
        if (event.repeat) return
        const change = () => {
          expansion = expansion === 'collapsed' ? 'preview' : expansion === 'preview' ? 'full' : 'collapsed'
          overrides = {}
        }
        if (conversation) void conversation.preserveScroll(change)
        else change()
      },
      jumpToQuestion: (event) => {
        const ask = hiddenAsk ?? orderedAsks[0]
        if (!ask) return false
        if (!event.repeat) enterAsk(ask.callId)
      },
      enterTranscript: () => nav.enter(),
      pageUp: () => conversation?.scrollPage(-1),
      pageDown: () => conversation?.scrollPage(1)
    },
    type: (key) => {
      if (!composer) return false
      composer.focus(key)
    }
  }
</script>

<!-- The chat view's one keyboard listener (ChatKeys); scopes inside register their own commands. -->
<div
  data-slot="chat-surface"
  role="region"
  aria-label="Axo agent"
  tabindex="-1"
  {@attach keys.listen}
  {@attach keys.scope(() => surface)}
  class="relative flex min-h-0 flex-1 flex-col bg-ground"
>
  <div class="relative flex min-h-0 flex-1 flex-row overflow-clip">
    <!-- isolate scopes the background's negative z-index so it paints above this bg but under the transcript. -->
    <div class="relative isolate flex min-h-0 min-w-0 flex-1 flex-col">
      <TidalStipple class="-z-10" params={turns.length ? ACTIVE_STIPPLE : undefined} transitionMs={900} />
      <ExtensionDiagnostics {controller} />
      <Conversation bind:this={conversation} {nav} {turns} {onAskVisibility}>
        {#if !turns.length}
          <div class="flex flex-1 flex-col items-center justify-center gap-6 py-10">
            <FolderHero folderPath={tab.folderPath}>
              <p class="mt-3 text-sm text-muted">
                Ask anything, or type <span class="font-mono">/</span> for commands.
              </p>
            </FolderHero>
            {#if personas.length}
              <PersonaCards
                agents={personas}
                value={chat?.live.agent ?? null}
                disabled={controller.connection !== 'live' || controller.readOnly}
                onSelect={(agent) => report(controller.setAgent(agent))}
              />
            {/if}
            {#if catalogMessage}
              <BlurFade delay={0.15} class="w-full max-w-xl">
                <Alert.Root variant={models.error ? 'danger' : 'info'}>
                  <Alert.Description>{catalogMessage}</Alert.Description>
                </Alert.Root>
              </BlurFade>
            {/if}
          </div>
        {/if}
        {#each turns as turn, turnIndex (turn.id)}
          <div data-turn-id={turn.id} {@attach nav.stop(`turn:${turn.id}`)} class="flex min-w-0 flex-col gap-4">
            {#if turn.user}<UserBlock
                entry={turn.user}
                {controller}
                {canBranch}
                {owner}
                commands={chat?.commands ?? []}
                models={available}
              />{/if}
            {#if turn.steps.length || turn.running || turn.end}
              <AgentBlock
                {turn}
                {turns}
                jobs={jobIndex}
                {turnIndex}
                {controller}
                folderPath={tab.folderPath}
                sessionId={tab.sessionId}
                {canBranch}
                {now}
                {expansion}
                {overrides}
                {onView}
                onOpenJob={(ref) => sidebar.openJob(ref)}
                onOpenPlan={() => (sidebar.view = 'plan')}
                captures={placedCaptures.anchored.get(turn.id) ?? []}
                {onOpenMemory}
              />
            {/if}
            {#if placedCaptures.after.has(turn.id)}
              {@const id = `band:after:${turn.id}`}
              <BackgroundBand
                {id}
                captures={placedCaptures.after.get(turn.id)}
                view={overrides[id] ?? (expansion === 'full' ? 'preview' : 'collapsed')}
                {onView}
                folderPath={tab.folderPath}
                sessionId={tab.sessionId}
                {onOpenMemory}
              />
            {/if}
          </div>
        {/each}
        {#if chat && !turns.length && chat.live.maintenance}
          <BackgroundBand
            id="band:live"
            maintenance={chat.live.maintenance}
            contextTokens={chat.live.stats.contextTokens}
            view="collapsed"
            {onView}
            folderPath={tab.folderPath}
            sessionId={tab.sessionId}
          />
        {/if}
        {#if chat && chat.live.maintenance !== null && chat.live.retry && !(turns.at(-1)?.running ?? false)}
          <RetryRow retry={chat.live.retry} class="py-2" />
        {/if}
        {#snippet footer()}
          {#if controller.readOnly}
            <div
              class="flex items-center justify-center gap-2 rounded-lg border border-edge bg-surface px-3 py-2 text-xs text-muted"
            >
              <span>Saved subagent transcript</span>
              {#if controller.connection === 'live'}
                <Button variant="outline" size="xs" onclick={() => void controller.resume()}>Resume</Button>
              {/if}
            </div>
          {:else if controller.connection === 'closed'}
            <Alert.Root variant="danger" class="flex items-center justify-between gap-3">
              <Alert.Description>{controller.error ?? 'The axo connection closed.'}</Alert.Description>
              <Button size="xs" onclick={() => void reconnect()}>Reconnect</Button>
            </Alert.Root>
          {:else}
            <QueuedMessages {controller} {owner} onEdited={() => composer?.focus()} />
            {#if hiddenAsk}
              {@const callId = hiddenAsk.callId}
              <ComposerNote tone="waiting">
                <span class="font-medium text-fg">axo is waiting on {questionCount} questions</span>
                {#if hiddenAsk.deadlineMs !== null}
                  <span class="min-w-0 truncate"
                    >suggested answers apply in <span class="tabular-nums"
                      >{askCountdown(hiddenAsk.deadlineMs, now)}</span
                    ></span
                  >
                {/if}
                {#snippet action()}
                  <Button variant="accent" size="xs" onclick={() => enterAsk(callId)}>Jump to questions</Button>
                {/snippet}
              </ComposerNote>
            {/if}
            {#if pending.length}
              {#snippet revert()}
                <Button
                  variant="ghost"
                  size="xs"
                  disabled={controller.connection !== 'live'}
                  onclick={() => report(controller.revertSetup(pending))}>Cancel</Button
                >
              {/snippet}
              <!-- No Cancel mid-turn: the diff may list what the running request already uses. -->
              <ComposerNote action={running ? undefined : revert}>
                <span>{running ? 'Next step' : 'Next message'}:</span>
                <span class="min-w-0 truncate text-fg"
                  >{pending
                    .map(({ key, to }) =>
                      key === 'persona'
                        ? (to ?? 'Default')
                        : key === 'mode'
                          ? (to ?? 'no mode')
                          : key === 'model'
                            ? modelName(to ?? '')
                            : `${to} effort`
                    )
                    .join(' · ')}</span
                >
              </ComposerNote>
            {/if}
            <PromptInput
              bind:this={composer}
              bind:parts={controller.draft}
              {owner}
              models={available}
              extensions={controller.extensions}
              {running}
              onCommand={(name, args) => controller.runCommand(name, args)}
              onMode={(name, parts) => controller.setMode(name, parts)}
              keywords={keywordNames}
              {keywordMatches}
              onStop={running ? () => void stop() : undefined}
              onLeaveUp={() => nav.enter()}
              {disabled}
              blocked={modelUnavailable ? `${model} is no longer available. Pick another model to continue.` : null}
              placeholder={pendingAsks.length ? 'Or reply here to steer instead…' : running ? 'Steer axo…' : 'Ask axo…'}
              commands={chat?.commands ?? []}
              {onSubmit}
              onFollowUp={() => {
                if (canSend) return controller.followUp()
              }}
            >
              {#snippet start()}
                <IconButton
                  tooltip="Attach File"
                  tooltipSide="top"
                  size="md"
                  class="hover:bg-raised"
                  {disabled}
                  onclick={() => void attachFile()}
                >
                  <Icon icon={PaperclipIcon} size={14} />
                </IconButton>
              {/snippet}
              {#snippet end()}
                <ModelPicker
                  variant="ghost"
                  label={chatShortcut('pickModel') ? `Model (${chatShortcut('pickModel')})` : 'Model'}
                  value={model ?? ''}
                  models={available}
                  loaded={!!catalog}
                  roles={catalog?.roles ?? []}
                  side="top"
                  align="end"
                  bind:open={modelOpen}
                  disabled={modelDisabled}
                  onSelect={(value) => configure('model', { model: value })}
                />
                <EffortPicker
                  variant="ghost"
                  label="Model options"
                  shortcutHint={chatShortcut('cycleThinking')}
                  tier={chat?.live.serviceTier}
                  effectiveTier={chat?.live.effectiveServiceTier}
                  serviceTiers={current?.serviceTiers}
                  modelName={current?.name ?? model ?? undefined}
                  tierDisabled={!current}
                  value={thinking}
                  {levels}
                  side="top"
                  align="end"
                  {disabled}
                  onSelect={(level) => configure('thinking', { level })}
                  onTierSelect={(tier) => configure('tier', { tier: tier ?? 'standard' })}
                >
                  {#snippet tierNote()}
                    <p class="text-xs text-muted">
                      Applies from the next request. Also <span class="font-mono">/tier fast</span>.
                    </p>
                  {/snippet}
                </EffortPicker>
              {/snippet}
            </PromptInput>
            {#if imageWarning}
              <p role="status" class="mt-1 px-2 text-sm text-warning">The selected model does not accept images.</p>
            {/if}
          {/if}
        {/snippet}
      </Conversation>
    </div>
    <SessionSidebar
      {controller}
      {sidebar}
      {owner}
      {now}
      {turns}
      {personas}
      onJump={(stepId) => void revealStep(stepId)}
    />
  </div>
</div>
