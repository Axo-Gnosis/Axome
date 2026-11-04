<script lang="ts">
  import { onMount } from 'svelte'
  import FolderHero from '$lib/components/layout/FolderHero.svelte'
  import * as Alert from '$lib/components/ui/alert'
  import { BlurFade } from '$lib/components/ui/blur-fade'
  import { IconButton } from '$lib/components/ui/button'
  import { Particles } from '$lib/components/ui/particles'
  import * as chats from '$lib/features/chat/registry'
  import { getChatModels } from '$lib/features/chat/models.svelte'
  import { resolveChatKey } from '$lib/features/chat/chatKeymap'
  import { transcriptTurns, turnDurationMs } from '$lib/features/chat/turns'
  import type { BlockView } from '$lib/features/chat/view'
  import { Conversation } from '$lib/features/chat/elements/conversation'
  import { UserBlock, AgentBlock } from '$lib/features/chat/elements/transcript'
  import { SessionBar } from '$lib/features/chat/elements/session-bar'
  import { PromptInput } from '$lib/features/chat/elements/prompt-input'
  import { fileReference } from '$lib/features/chat/elements/prompt-input/attachments'
  import { ConfigPicker } from '$lib/features/chat/elements/model-picker'
  import PartChip from '$lib/features/chat/parts/PartChip.svelte'
  import { hasContent, isImagePart, partsText } from '$lib/features/chat/parts/parts'
  import { selectFile } from '$lib/features/browser/state.svelte'
  import { notify } from '$lib/features/notifications/state.svelte'
  import type { ChatTab } from '$lib/features/workbench/model'
  import { openFileLocation, openLink } from '$lib/features/workbench/links/openLink'
  import { isSchemeUrl } from '$lib/features/workbench/links/parseLink'
  import { PaperclipIcon } from '@hugeicons/core-free-icons'
  import Icon from '$lib/icons/Icon.svelte'
  import { getErrorMessage } from '$lib/utils/client-error'
  import { logicalKey } from '$lib/utils/keyboardEvent'

  let { tab }: { tab: ChatTab } = $props()
  const controller = $derived(chats.getOrCreate(tab))
  const chat = $derived(controller.state)
  const models = $derived(getChatModels(tab.folderPath))
  const catalog = $derived(models.catalog)
  const turns = $derived(chat ? transcriptTurns(chat.entries, chat.streaming, chat.live) : [])
  const running = $derived(chat?.live.running ?? false)
  const model = $derived(chat?.live.model ?? null)
  const thinking = $derived(chat?.live.thinking ?? 'off')
  const choices = $derived.by(() => {
    const choices = (catalog?.models ?? []).map((item) => ({
      value: item.selector,
      name: item.name,
      group: item.provider
    }))
    if (model && !choices.some((choice) => choice.value === model)) {
      choices.push({ value: model, name: model, group: model.split('/')[0] })
    }
    return choices
  })
  const levels = $derived(catalog?.models.find((item) => item.selector === model)?.thinking ?? [thinking])
  const disabled = $derived(controller.connection !== 'live' || !model || !!chat?.live.maintenance)
  const modelDisabled = $derived(disabled || !choices.length)
  const canSend = $derived(!disabled && hasContent(controller.draft))
  const imageWarning = $derived(
    catalog?.models.find((item) => item.selector === model)?.images === false && controller.draft.some(isImagePart)
  )
  const owner = $derived({ folderPath: tab.folderPath, sessionId: tab.sessionId })
  const canBranch = $derived(controller.connection === 'live' && !controller.busy)
  const catalogMessage = $derived(
    models.error
      ? `${models.error} Check ~/.axo/settings.jsonc and credentials in ~/.axo/auth.json or the environment.`
      : catalog && !catalog.models.length
        ? 'No axo models are available on this host. Set defaultModel in ~/.axo/settings.jsonc and credentials in ~/.axo/auth.json or the environment.'
        : null
  )
  let expansion = $state<BlockView>('collapsed')
  let overrides = $state<Record<string, BlockView>>({})
  let conversation = $state<Conversation>()
  let composer = $state<PromptInput>()
  let modelOpen = $state(false)
  let now = $state(Date.now())
  const workingTurn = $derived(turns.find((turn) => turn.running))
  const elapsed = $derived(workingTurn ? turnDurationMs(workingTurn, now) : null)

  onMount(() => {
    if (controller.connection === 'detached') void reconnect()
  })

  $effect(() => {
    if (!running) return
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

  function openLocation(path: string, line?: number) {
    if (isSchemeUrl(path)) {
      void openLink(path, owner)
      return
    }
    void openFileLocation(
      path,
      line === undefined ? undefined : { kind: 'position', lineNumber: line, column: 1 },
      tab.folderPath
    )
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
  <SessionBar
    {controller}
    title={chat?.live.title ?? tab.title}
    sessionId={tab.sessionId}
    {elapsed}
    onReconnect={() => void reconnect()}
    onStop={() => void stop()}
  />
  <!-- isolate scopes the particles' negative z-index so they paint above this bg but under the transcript. -->
  <div class="relative isolate flex min-h-0 flex-1 flex-col">
    {#if !turns.length}<Particles class="-z-10 opacity-60" />{/if}
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
            models={catalog?.models ?? []}
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
            onOpenLocation={openLocation}
          />
        {/if}
      {/each}
      {#snippet footer()}
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
          models={catalog?.models ?? []}
          {running}
          onCommand={(name, args) => controller.runCommand(name, args)}
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
            <ConfigPicker
              label="Model"
              value={model ?? ''}
              {choices}
              bind:open={modelOpen}
              disabled={modelDisabled}
              onSelect={(value) => configure('model', { model: value })}
            />
            {#if levels.some((level) => level !== 'off')}
              <ConfigPicker
                label="Effort"
                value={thinking}
                choices={levels.map((level) => ({ value: level, name: level }))}
                {disabled}
                onSelect={(value) => configure('thinking', { level: value })}
              />
            {/if}
          {/snippet}
        </PromptInput>
        {#if imageWarning}
          <p role="status" class="mt-1 px-2 text-sm text-warning">The selected model does not accept images.</p>
        {/if}
      {/snippet}
    </Conversation>
  </div>
</div>
