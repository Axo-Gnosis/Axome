<script lang="ts">
  import { Switch } from '$lib/components/ui/switch'
  import SettingRow from '../dialog/SettingRow.svelte'
  import type { AxoFields } from './axoFields.svelte'
  let { fields, path, label, checked }: { fields: AxoFields; path: string[]; label: string; checked: boolean } =
    $props()
</script>

<SettingRow {label} as="div">
  <Switch
    aria-label={label}
    {checked}
    disabled={!fields.settings}
    onCheckedChange={(value) => fields.save(path, value)}
  />
</SettingRow>
