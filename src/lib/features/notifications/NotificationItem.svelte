<script lang="ts" module>
  import { SvelteSet } from 'svelte/reactivity'

  // The toast can time out and remount in the notification center while its action runs.
  const runningActions = new SvelteSet<string>()
</script>

<script lang="ts">
  import * as Alert from '$lib/components/ui/alert'
  import { Button, IconButton } from '$lib/components/ui/button'
  import TabBeam from '$lib/components/ui/TabBeam.svelte'
  import { XIcon } from '@hugeicons/core-free-icons'
  import Icon from '$lib/icons/Icon.svelte'
  import { notify, type Notification, type NotificationAction, type NotificationTone } from './state.svelte'
  import { cn } from '$lib/utils/cn'
  import { getErrorMessage } from '$lib/utils/client-error'

  let {
    notification,
    onDismiss,
    showTimestamp = true
  }: {
    notification: Notification
    onDismiss: (id: string) => void
    showTimestamp?: boolean
  } = $props()

  async function runAction(action: NotificationAction) {
    const id = notification.id
    if (runningActions.has(id)) return
    runningActions.add(id)
    try {
      await action.run()
    } catch (error) {
      notify.error(`${action.label} failed`, getErrorMessage(error))
    } finally {
      runningActions.delete(id)
    }
  }

  function getNotificationAlertVariant(tone: NotificationTone) {
    return tone === 'neutral' ? 'info' : tone === 'error' ? 'danger' : tone
  }

  function formatNotificationTimestamp(timestamp: number): string {
    const diff = Date.now() - timestamp
    if (diff < 60_000) return 'just now'
    if (diff < 3_600_000) return `${Math.floor(diff / 60_000)}m ago`
    if (diff < 86_400_000) return `${Math.floor(diff / 3_600_000)}h ago`
    return new Date(timestamp).toLocaleDateString()
  }
</script>

<Alert.Root variant={getNotificationAlertVariant(notification.tone)} class="group min-h-[4.25rem] pr-10">
  <div class="min-w-0 flex-1 space-y-1">
    <Alert.Title class="pr-1">{notification.title}</Alert.Title>
    {#if notification.description}
      <Alert.Description class="pr-1">{notification.description}</Alert.Description>
    {/if}
    {#if notification.actions?.length}
      <div class="flex flex-wrap gap-1.5">
        {#each notification.actions as action}
          <Button
            variant="text"
            size="xs"
            disabled={runningActions.has(notification.id)}
            onclick={() => runAction(action)}>{action.label}</Button
          >
        {/each}
      </div>
    {/if}

    {#if notification.loading}
      <div class="relative mt-1 h-1.5 overflow-hidden rounded-full bg-ground/70" aria-hidden="true">
        <TabBeam class="h-full rounded-full" />
      </div>
    {/if}

    <div class={cn('text-2xs leading-none text-subtle', !showTimestamp && 'invisible')} aria-hidden={!showTimestamp}>
      {formatNotificationTimestamp(notification.timestamp)}
    </div>
  </div>

  <IconButton tooltip="Dismiss" class="absolute top-2 right-2" onclick={() => onDismiss(notification.id)}>
    <Icon icon={XIcon} size={10} />
  </IconButton>
</Alert.Root>
