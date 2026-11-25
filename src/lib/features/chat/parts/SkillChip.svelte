<!--
  @component
  SkillChip — an invoked skill's `/name` in a sent message; opens the skill file.

  @param name - the skill's name
  @param info - the skill's catalog entry; null when the session no longer has it
  @param owner - folder the skill path resolves against
-->
<script lang="ts">
  import { BookOpen01Icon } from '@hugeicons/core-free-icons'
  import Icon from '$lib/icons/Icon.svelte'
  import { Button } from '$lib/components/ui/button'
  import { chipVariants } from '$lib/components/ui/chip'
  import * as Tooltip from '$lib/components/ui/tooltip'
  import { openFileLocation } from '$lib/features/workbench/links/openLink'
  import type { ChatSkillInfo } from '$lib/types/backend'
  import { cn } from '$lib/utils/cn'

  let {
    name,
    info,
    owner
  }: { name: string; info: ChatSkillInfo | null; owner: { folderPath: string; sessionId: string | null } } = $props()
</script>

<!-- No whitespace between trigger and content: the chip sits inside message text. -->
<Tooltip.Root>
  <Tooltip.Trigger>
    {#snippet child({ props })}
      <Button
        {...props}
        data-slot="skill-chip"
        variant="ghost"
        disabled={!info}
        class={cn(chipVariants(), 'h-auto gap-0 py-0 font-normal hover:bg-overlay hover:text-bright')}
        onclick={() => info && void openFileLocation(info.path, null, owner.folderPath)}
        ><Icon icon={BookOpen01Icon} size={12} class="me-1 inline-block align-middle" />{'/' + name}</Button
      >
    {/snippet}
  </Tooltip.Trigger><Tooltip.Content side="top" class="max-w-80">
    <div><span class="font-medium text-bright">{name}</span> · skill</div>
    {#if info?.description}<div>{info.description}</div>{/if}
    {#if info}<div class="font-mono text-xs text-muted">{info.path}</div>{/if}
  </Tooltip.Content>
</Tooltip.Root>
