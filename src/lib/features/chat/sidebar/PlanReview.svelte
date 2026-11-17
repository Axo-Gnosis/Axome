<script lang="ts">
  import { Button } from '$lib/components/ui/button'
  import { Input, Textarea } from '$lib/components/ui/input'
  import SidebarDivision from '$lib/features/app-shell/sidebar/SidebarDivision.svelte'
  import type { ChatController } from '$lib/features/chat/controller.svelte'
  import { record } from '$lib/features/chat/elements/tool/payload'
  import { newChat } from '$lib/features/sessions/service.svelte'
  import type { ChatPlanThen, ChatReview } from '$lib/types/backend'

  let {
    review,
    controller,
    owner
  }: {
    review: ChatReview
    controller: ChatController
    owner: { folderPath: string; sessionId: string | null }
  } = $props()

  const id = $props.id()
  const result = $derived(record(review.result))
  const title = $derived(typeof result.title === 'string' ? result.title : '')
  const canReview = $derived(controller.connection === 'live' && !controller.readOnly)
  let saveAsDraft = $state<string | null>(null)
  const saveAs = $derived(saveAsDraft ?? (typeof result.saveAs === 'string' ? result.saveAs : ''))
  const freeze = () => {
    saveAsDraft ??= saveAs
  }
  let feedback = $state('')
  let error = $state<string | null>(null)
  let busy = $state(false)
  const locked = $derived(!canReview || busy)
  const canAccept = $derived(!locked && !!saveAs.trim())
  const canRevise = $derived(!locked && !!feedback.trim())

  async function run(action: () => Promise<string | null>) {
    if (busy) return
    freeze()
    busy = true
    error = null
    try {
      error = await action()
    } finally {
      busy = false
    }
  }

  function accept(then: ChatPlanThen, newSession = false) {
    const path = saveAs.trim()
    if (!canAccept || !path) return
    void run(async () => {
      const outcome = await controller.acceptPlan(review.callId, path, then)
      if ('error' in outcome) return outcome.error
      if (newSession) {
        newChat(owner.folderPath, [
          { type: 'text', text: 'Implement the plan in ' },
          { type: 'file', path: outcome.path }
        ])
      }
      return null
    })
  }

  function revise() {
    const message = feedback.trim()
    if (!canRevise || !message) return
    void run(() => controller.reviseResult(review.callId, message))
  }
</script>

<SidebarDivision title="Review">
  <div class="flex flex-col gap-2.5 px-3 pt-1 pb-3">
    {#if title}
      <p class="text-md text-bright">{title}</p>
    {/if}
    <label class="flex flex-col gap-1 text-sm text-muted">
      Save as
      <Input
        class="font-mono text-sm"
        bind:value={() => saveAs, (value) => (saveAsDraft = String(value ?? ''))}
        disabled={locked}
        spellcheck={false}
      />
    </label>
    <label for={`${id}-feedback`} class="sr-only">Requested changes</label>
    <Textarea
      id={`${id}-feedback`}
      rows={3}
      placeholder="What should change?"
      bind:value={
        () => feedback,
        (value) => {
          freeze()
          feedback = value ?? ''
        }
      }
      disabled={locked}
    />
    <div class="flex flex-wrap items-center gap-2">
      <Button type="button" variant="accent" size="xs" disabled={!canAccept} onclick={() => accept('work')}>
        Work
      </Button>
      <Button type="button" size="xs" disabled={!canAccept} onclick={() => accept('compactWork')}>
        Compact & Work
      </Button>
      <Button type="button" size="xs" disabled={!canAccept} onclick={() => accept('save', true)}>
        Work in New Session
      </Button>
      <Button type="button" size="xs" disabled={!canAccept} onclick={() => accept('save')}>Save Only</Button>
      <Button type="button" size="xs" disabled={!canRevise} onclick={revise}>Request Changes</Button>
    </div>
    {#if error}
      <p role="alert" class="text-sm whitespace-pre-wrap text-danger">{error}</p>
    {/if}
  </div>
</SidebarDivision>
