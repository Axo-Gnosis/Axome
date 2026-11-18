<script lang="ts">
  import { Button } from '$lib/components/ui/button'
  import * as Dialog from '$lib/components/ui/dialog'
  import { Input } from '$lib/components/ui/input'

  let {
    open,
    title,
    description,
    key = $bindable(),
    busy,
    error,
    onSave,
    onClose
  }: {
    open: boolean
    title: string
    description?: string
    key: string
    busy: boolean
    error?: string | null
    onSave: () => void
    onClose: () => void
  } = $props()
</script>

<Dialog.Root
  {open}
  onOpenChange={(open) => {
    if (!open) onClose()
  }}
>
  <Dialog.Content>
    <Dialog.Header>
      <Dialog.Title>{title}</Dialog.Title>
      {#if description}<Dialog.Description>{description}</Dialog.Description>{/if}
    </Dialog.Header>
    <form
      class="flex flex-col gap-3"
      onsubmit={(event) => {
        event.preventDefault()
        onSave()
      }}
    >
      <label class="flex flex-col gap-2 text-sm text-muted"
        >API key<Input type="password" bind:value={key} autocomplete="off" disabled={busy} /></label
      >
      {#if error}<p role="alert" class="text-sm text-danger">{error}</p>{/if}
      <Dialog.Footer>
        <Button type="button" disabled={busy} onclick={onClose}>Cancel</Button>
        <Button type="submit" variant="accent" disabled={busy || !key.trim()}>Save Key</Button>
      </Dialog.Footer>
    </form>
  </Dialog.Content>
</Dialog.Root>
