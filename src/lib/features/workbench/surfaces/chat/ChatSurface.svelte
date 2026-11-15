<script lang="ts">
  import { onMount } from 'svelte'
  import FolderHero from '$lib/components/layout/FolderHero.svelte'
  import * as Alert from '$lib/components/ui/alert'
  import { BlurFade } from '$lib/components/ui/blur-fade'
  import { Button, IconButton } from '$lib/components/ui/button'
  import { TidalStipple, type StippleParams } from '$lib/components/ui/tidal-stipple'
  import * as chats from '$lib/features/chat/registry'
  import { getChatModels, MODELS_HINT } from '$lib/features/chat/models.svelte'
  import { resolveChatKey } from '$lib/features/chat/chatKeymap'
  import { transcriptTurns } from '$lib/features/chat/turns'
  import { isActiveJob } from '$lib/features/chat/jobs'
  import type { BlockView } from '$lib/features/chat/view'
  import { Conversation } from '$lib/features/chat/elements/conversation'
  import { UserBlock, AgentBlock } from '$lib/features/chat/elements/transcript'
  import { PromptInput } from '$lib/features/chat/elements/prompt-input'
  import { fileReference } from '$lib/features/chat/elements/prompt-input/attachments'
  import { ModelPicker, EffortPicker } from '$lib/features/chat/elements/model-picker'
  import PartChip from '$lib/features/chat/parts/PartChip.svelte'
  import { hasContent, isImagePart, partsText } from '$lib/features/chat/parts/parts'
  import { selectFile } from '$lib/features/browser/state.svelte'
  import { notify } from '$lib/features/notifications/state.svelte'
  import type { ChatTab } from '$lib/features/workbench/model'
  import { PaperclipIcon } from '@hugeicons/core-free-icons'
  import Icon from '$lib/icons/Icon.svelte'
  import { getErrorMessage } from '$lib/utils/client-error'
  import { logicalKey } from '$lib/utils/keyboardEvent'
  import SessionSidebar from '$lib/features/chat/sidebar/SessionSidebar.svelte'
  import { SessionSidebarState } from '$lib/features/chat/sidebar/state.svelte'

  let { tab }: { tab: ChatTab } = $props()
  const controller = $derived(chats.getOrCreate(tab))
  const chat = $derived(controller.state)
  const models = $derived(getChatModels(tab.folderPath))
  const catalog = $derived(models.catalog)
  const available = $derived(catalog?.models ?? [])
  const turns = $derived(chat ? transcriptTurns(chat.entries, chat.streaming, chat.live) : [])
  const running = $derived(chat?.live.running ?? false)
  const model = $derived(chat?.live.model ?? null)
  const current = $derived(available.find((item) => item.selector === model))
  const thinking = $derived(chat?.live.thinking ?? 'off')
  const levels = $derived(current?.thinking ?? [thinking])
  const disabled = $derived(controller.connection !== 'live' || !model || !!chat?.live.maintenance)
  const modelDisabled = $derived(disabled || !available.length)
  const canSend = $derived(!disabled && hasContent(controller.draft))
  const imageWarning = $derived(current?.images === false && controller.draft.some(isImagePart))
  const owner = $derived({ folderPath: tab.folderPath, sessionId: tab.sessionId })
  const canBranch = $derived(controller.connection === 'live' && !controller.busy)
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
    if (!running && !(chat?.live.jobs ?? []).some((j) => isActiveJob(j.status))) return
    now = Date.now()
    const timer = setInterval(() => (now = Date.now()), 1000)
    return () => clearInterval(timer)
  })
  async function reconnect() {
    // attach stores the failure for the closed-connection status line.
    await controller.attach().catch(() => {})
  }

  function onSubmit() {
    if (canSend) void controller.send().then(() => conversation?.scrollToBottom())
  }

  function onAltSubmit() {
    if (!canSend) return
    if (running) void controller.followUp()
    else onSubmit()
  }

  function configure(name: string, args: Record<string, unknown>) {
    void controller.runCommand(name, args).then((error) => {
      if (error) notify.error('axo', error)
    })
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

  function onView(stepId: string, view: BlockView) {
    const change = () => {
      overrides[stepId] = view
    }
    if (conversation) void conversation.preserveScroll(change, stepId)
    else change()
  }

  function onKeyAction(event: KeyboardEvent) {
    const target = event.target instanceof HTMLElement ? event.target : null
    const inComposer = !!target?.closest('[data-slot="prompt-input-root"] [contenteditable="true"]')
    const inEditable = !!target?.closest(
      'input, textarea, select, button, a, [contenteditable="true"], [role="textbox"]'
    )
    const action = resolveChatKey(event, { inComposer, inEditable, running })
    if (!action) return
    event.preventDefault()
    event.stopPropagation()
    switch (action) {
      case 'stop':
        void stop()
        break
      case 'cycleThinking':
        if (!disabled && levels.length) {
          configure('thinking', { level: levels[(levels.indexOf(thinking) + 1) % levels.length] })
        }
        break
      case 'openModel':
        if (!modelDisabled) modelOpen = true
        break
      case 'cycleExpansion': {
        const change = () => {
          expansion = expansion === 'collapsed' ? 'preview' : expansion === 'preview' ? 'full' : 'collapsed'
          overrides = {}
        }
        if (conversation) void conversation.preserveScroll(change)
        else change()
        break
      }
      case 'pageUp':
        conversation?.scrollPage(-1)
        break
      case 'pageDown':
        conversation?.scrollPage(1)
        break
      case 'focusComposer':
        composer?.focus(logicalKey(event))
        break
    }
  }
</script>

<!-- Surface keymap (chatKeymap.ts) delegates from here; every control inside stays native. -->
<!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
<div
  data-slot="chat-surface"
  role="region"
  aria-label="Axo agent"
  tabindex="-1"
  onkeydown={onKeyAction}
  class="relative flex min-h-0 flex-1 flex-col bg-ground"
>
  <div class="relative flex min-h-0 flex-1 flex-row overflow-hidden">
    <!-- isolate scopes the background's negative z-index so it paints above this bg but under the transcript. -->
    <div class="relative isolate flex min-h-0 min-w-0 flex-1 flex-col">
      <TidalStipple class="-z-10" params={turns.length ? ACTIVE_STIPPLE : undefined} transitionMs={900} />
      <Conversation bind:this={conversation}>
        {#if !turns.length}
          <div class="flex flex-1 flex-col items-center justify-center gap-6 py-10">
            <FolderHero folderPath={tab.folderPath}>
              <p class="mt-3 text-sm text-muted">
                Ask anything, or type <span class="font-mono">/</span> for commands.
              </p>
            </FolderHero>
            {#if catalogMessage}
              <BlurFade delay={0.15} class="w-full max-w-xl">
                <Alert.Root variant={models.error ? 'danger' : 'info'}>
                  <Alert.Description>{catalogMessage}</Alert.Description>
                </Alert.Root>
              </BlurFade>
            {/if}
          </div>
        {/if}
        {#each turns as turn (turn.id)}
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
              {controller}
              folderPath={tab.folderPath}
              sessionId={tab.sessionId}
              {canBranch}
              now={turn.running ? now : 0}
              {expansion}
              {overrides}
              {onView}
              onOpenJob={(ref) => sidebar.openJob(ref)}
            />
          {/if}
        {/each}
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
            {#if chat?.live.autoPaused}
              <Alert.Root variant="warning" class="mb-2 flex items-center justify-between gap-3">
                <Alert.Description>Background wakes paused after an error or stop.</Alert.Description>
                <Button variant="outline" size="xs" onclick={() => void controller.resumeWakes()}>Resume</Button>
              </Alert.Root>
            {/if}
            {#if chat?.live.queue.length}
              <ul aria-label="Queued messages" class="flex flex-col gap-1 pb-2">
                {#each chat.live.queue as item}
                  <li class="flex gap-2 text-xs text-subtle">
                    <span class="shrink-0 font-mono">{item.kind === 'steer' ? 'steer:' : 'follow-up:'}</span>
                    <span
                      class="flex min-w-0 flex-wrap items-center gap-1 break-words whitespace-pre-wrap"
                      aria-label={partsText(item.parts)}
                    >
                      {#each item.parts as part}
                        {#if part.type === 'text'}{part.text}{:else}<PartChip {part} {owner} />{/if}
                      {/each}
                    </span>
                  </li>
                {/each}
              </ul>
            {/if}
            <PromptInput
              bind:this={composer}
              bind:parts={controller.draft}
              {owner}
              models={available}
              {running}
              onCommand={(name, args) => controller.runCommand(name, args)}
              onCancel={running ? () => void stop() : undefined}
              {disabled}
              placeholder={running ? 'Steer axo…' : 'Ask axo…'}
              commands={chat?.commands ?? []}
              {onSubmit}
              {onAltSubmit}
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
                  label="Model (Alt+M)"
                  value={model ?? ''}
                  models={available}
                  roles={catalog?.roles ?? []}
                  side="top"
                  align="end"
                  bind:open={modelOpen}
                  disabled={modelDisabled}
                  onSelect={(value) => configure('model', { model: value })}
                />
                <EffortPicker
                  variant="ghost"
                  label="Reasoning effort (Shift+Tab)"
                  value={thinking}
                  {levels}
                  side="top"
                  align="end"
                  {disabled}
                  onSelect={(level) => configure('thinking', { level })}
                />
              {/snippet}
            </PromptInput>
            {#if imageWarning}
              <p role="status" class="mt-1 px-2 text-sm text-warning">The selected model does not accept images.</p>
            {/if}
          {/if}
        {/snippet}
      </Conversation>
    </div>
    <SessionSidebar {controller} {sidebar} {owner} {now} />
  </div>
</div>
