<script lang="ts">
  import * as Alert from '$lib/components/ui/alert'
  import { Button, IconButton } from '$lib/components/ui/button'
  import type { ChatController } from '$lib/features/chat/controller.svelte'
  import { chatExtensionTarget, commandClash } from '$lib/features/chat/extensions'
  import { openExtensionSettings } from '$lib/features/settings/dialog/state.svelte'
  import Icon from '$lib/icons/Icon.svelte'
  import { XIcon } from '@hugeicons/core-free-icons'

  let { controller, folderPath }: { controller: ChatController; folderPath: string } = $props()
</script>

{#if controller.extensionDiagnostics.length}
  <div data-slot="extension-diagnostics" class="flex max-h-40 shrink-0 flex-col gap-1.5 overflow-y-auto px-3 py-2">
    {#each controller.extensionDiagnostics as diagnostic (diagnostic)}
      {@const clash = commandClash(diagnostic)}
      <Alert.Root variant="warning">
        <div class="flex items-start gap-2">
          <div class="min-w-0 flex-1 wrap-anywhere">
            {#if clash}
              <Alert.Title
                ><span class="font-mono">/{clash.name}</span> from {clash.origin.id} isn't available in this chat:</Alert.Title
              >
            {/if}
            <Alert.Description class="whitespace-pre-wrap">{diagnostic}</Alert.Description>
          </div>
          <IconButton size="sm" tooltip="Dismiss" onclick={() => controller.dismissExtensionDiagnostic(diagnostic)}>
            <Icon icon={XIcon} size={14} />
          </IconButton>
        </div>
        <Button
          variant="text"
          size="xs"
          onclick={() => openExtensionSettings(chatExtensionTarget(folderPath, clash?.origin ?? null))}
        >
          Open Extensions
        </Button>
      </Alert.Root>
    {/each}
  </div>
{/if}
