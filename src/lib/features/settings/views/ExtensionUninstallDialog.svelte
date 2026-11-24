<script lang="ts">
  import { Button } from '$lib/components/ui/button'
  import { Checkbox } from '$lib/components/ui/checkbox'
  import * as Dialog from '$lib/components/ui/dialog'
  import type { AxoExtension } from '$lib/types/backend'

  let {
    item,
    hostLabel,
    busy,
    error,
    onUninstall,
    onClose
  }: {
    item: AxoExtension | null
    hostLabel: string
    busy: boolean
    error: string | null
    onUninstall: (deleteData: boolean) => void
    onClose: () => void
  } = $props()
  let deleteData = $state(false)
  const checkbox = $props.id()

  function close() {
    if (busy) return
    deleteData = false
    onClose()
  }
</script>

<Dialog.Root
  open={item !== null}
  onOpenChange={(open) => {
    if (!open) close()
  }}
>
  {#if item}
    <Dialog.Content
      onModalClose={close}
      aria-busy={busy}
      escapeKeydownBehavior={busy ? 'ignore' : 'close'}
      interactOutsideBehavior={busy ? 'ignore' : 'close'}
    >
      <Dialog.Header>
        <Dialog.Title>Uninstall {item.id}?</Dialog.Title>
        <Dialog.Description>
          {item.enabled ? "It's on, so it's turned off first. Chats lose what it adds. " : ''}Both copies are removed
          from {hostLabel}.
        </Dialog.Description>
      </Dialog.Header>
      <div class="flex flex-col gap-2 text-sm text-muted">
        <label for={checkbox} class="flex items-center gap-2 text-fg">
          <Checkbox id={checkbox} bind:checked={deleteData} disabled={busy} /> Also delete its data
        </label>
        <p class="font-mono break-all">{item.dataDir}</p>
        <p>Can't be undone. Files it created elsewhere aren't touched.</p>
        {#if error}<p role="alert" class="text-danger-bright">{error}</p>{/if}
      </div>
      <Dialog.Footer>
        <Button size="sm" disabled={busy} onclick={close}>Cancel</Button>
        <Button size="sm" variant="destructive" disabled={busy} onclick={() => onUninstall(deleteData)}
          >Uninstall</Button
        >
      </Dialog.Footer>
    </Dialog.Content>
  {/if}
</Dialog.Root>
