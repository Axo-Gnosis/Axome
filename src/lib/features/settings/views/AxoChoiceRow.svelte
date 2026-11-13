<script lang="ts">
  import { Select } from '$lib/components/ui/select'
  import SettingRow from '../dialog/SettingRow.svelte'
  import type { AxoFields } from './axoFields.svelte'
  let {
    fields,
    path,
    label,
    value,
    choices
  }: { fields: AxoFields; path: string[]; label: string; value: string; choices: readonly string[] } = $props()
</script>

<SettingRow {label}>
  <Select {value} disabled={!fields.settings} onValueChange={(next) => fields.save(path, next)} items={choices} />
</SettingRow>
