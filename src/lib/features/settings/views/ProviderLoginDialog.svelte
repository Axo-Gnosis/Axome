<script lang="ts">
  import { backend } from '$lib/api/backend'
  import { Button } from '$lib/components/ui/button'
  import * as Dialog from '$lib/components/ui/dialog'
  import { Input } from '$lib/components/ui/input'
  import { Spinner } from '$lib/components/ui/progress'
  import { notify } from '$lib/features/notifications/state.svelte'
  import { platform } from '$lib/platform'
  import type { AxoLoginView } from '$lib/types/backend'
  import { getErrorMessage } from '$lib/utils/client-error'
  import { copyToClipboard } from '$lib/utils/clipboard'

  let {
    provider = $bindable(null),
    host,
    scopeFolder,
    disabled = false,
    onDone
  }: {
    provider?: string | null
    host: string | null
    scopeFolder: string | null
    disabled?: boolean
    onDone: () => void
  } = $props()

  let generation = 0
  let loginId: string | null = null
  let view = $state<AxoLoginView | null>(null)
  let error = $state<string | null>(null)
  let value = $state('')
  let submitted = $state<number | null>(null)

  async function openUrl(url: string) {
    try {
      if (!['https:', 'http:'].includes(new URL(url).protocol)) throw new Error('Expected an HTTP or HTTPS login URL')
      await platform.links.openExternal(url)
    } catch (cause) {
      notify.error('Could not open login URL', getErrorMessage(cause))
    }
  }

  function cancel(id: string, owner: string | null) {
    void backend.axo
      .loginCancel(id, owner)
      .catch((cause) => notify.error('Could not cancel login', getErrorMessage(cause)))
  }

  $effect(() => {
    const name = provider
    if (name === null) return
    const owner = host
    const scope = scopeFolder
    const current = ++generation
    let timer: ReturnType<typeof setTimeout>
    let lastUrl: string | undefined
    view = null
    error = null
    submitted = null
    value = ''

    async function poll(id: string) {
      const status = await backend.axo.loginStatus(id, owner)
      if (current !== generation) return
      if (status.state !== 'pending') {
        loginId = null // The host retires the login on its terminal status poll.
        view = null
        if (status.state === 'failed') error = status.message
        else {
          provider = null
          onDone()
        }
        return
      }
      if (view?.question?.id !== status.view.question?.id) {
        value = ''
        submitted = null
        error = null
      }
      view = status.view
      const url = view.url?.url
      if (url && url !== lastUrl) {
        lastUrl = url
        void openUrl(url)
      }
      timer = setTimeout(() => void poll(id).catch(fail), 1000)
    }

    function fail(cause: unknown) {
      if (current !== generation) return
      view = null
      error = getErrorMessage(cause)
    }

    void backend.axo
      .loginStart(name, scope, owner)
      .then(({ loginId: id }) => {
        if (current !== generation) return cancel(id, owner)
        loginId = id
        return poll(id)
      })
      .catch(fail)

    return () => {
      generation++
      clearTimeout(timer)
      if (loginId !== null) cancel(loginId, owner)
      loginId = null
      value = ''
    }
  })

  async function submit(id: number, answer: string) {
    if (disabled || !loginId || submitted === id) return
    const current = generation
    submitted = id
    error = null
    try {
      await backend.axo.loginSubmit(loginId, id, answer, host)
      if (generation === current && view?.question?.id === id) value = ''
    } catch (cause) {
      if (generation !== current || view?.question?.id !== id) return
      submitted = null
      error = getErrorMessage(cause)
    }
  }
</script>

<Dialog.Root
  open={provider !== null}
  onOpenChange={(open) => {
    if (!open) provider = null
  }}
>
  <Dialog.Content onModalClose={() => (provider = null)}>
    <Dialog.Header><Dialog.Title>Log In to {provider}</Dialog.Title></Dialog.Header>
    <div class="flex flex-col gap-3">
      {#if view}
        {#if view.url}
          {@const link = view.url}
          <label class="flex flex-col gap-2 text-sm text-muted">Login URL<Input value={link.url} readonly /></label>
          {#if link.instructions}<p class="text-xs text-muted">{link.instructions}</p>{/if}
          <div class="flex gap-2">
            <Button size="sm" onclick={() => openUrl(link.url)}>Open Login</Button>
            <Button size="sm" onclick={() => copyToClipboard(link.url)}>Copy Link</Button>
          </div>
        {/if}
        {#if view.deviceCode}
          {@const device = view.deviceCode}
          <div class="flex items-center gap-2">
            <span class="font-mono text-lg text-bright">{device.userCode}</span>
            <Button size="sm" onclick={() => copyToClipboard(device.userCode)}>Copy Code</Button>
          </div>
          <div class="flex items-center gap-2">
            <span class="font-mono text-sm break-all">{device.verificationUrl}</span>
            <Button size="sm" onclick={() => openUrl(device.verificationUrl)}>Open</Button>
          </div>
          {#if device.expiresInSeconds !== null}<p class="text-xs text-muted">
              Expires in {Math.ceil(device.expiresInSeconds / 60)} min
            </p>{/if}
        {/if}
        {#if view.question}
          {@const question = view.question}
          {#if view.url && (host !== null || !platform.native)}
            <p class="text-xs text-muted">
              Complete login in your browser, then follow the prompt below. Credentials stay on the selected host.
            </p>
          {/if}
          {#if question.kind === 'prompt'}
            <form
              class="flex flex-col gap-2"
              onsubmit={(event) => {
                event.preventDefault()
                if (value) void submit(question.id, value)
              }}
            >
              <label class="flex flex-col gap-2 text-sm text-muted">
                {question.message}
                <Input
                  type={question.secret ? 'password' : 'text'}
                  placeholder={question.placeholder ?? undefined}
                  bind:value
                  disabled={disabled || submitted === question.id}
                />
              </label>
              <Button type="submit" disabled={disabled || !value || submitted === question.id}>Submit</Button>
            </form>
          {:else}
            <p class="text-sm text-muted">{question.message}</p>
            <div class="flex flex-wrap gap-2">
              {#each question.options as option (option.id)}
                <Button
                  size="sm"
                  disabled={disabled || submitted === question.id}
                  onclick={() => submit(question.id, option.id)}>{option.label}</Button
                >
              {/each}
            </div>
          {/if}
        {/if}
        <p class="flex items-center gap-2 text-xs text-muted"><Spinner />{view.progress ?? 'Waiting for login…'}</p>
      {:else if error === null}
        <Spinner label="Starting login" />
      {/if}
      {#if error !== null}<p role="alert" class="text-sm text-danger">{error}</p>{/if}
    </div>
    <Dialog.Footer><Button onclick={() => (provider = null)}>Cancel</Button></Dialog.Footer>
  </Dialog.Content>
</Dialog.Root>
