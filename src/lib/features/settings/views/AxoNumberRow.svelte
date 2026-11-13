<script lang="ts">
  import { Input } from '$lib/components/ui/input'
  import SettingRow from '../dialog/SettingRow.svelte'
  import type { AxoFields } from './axoFields.svelte'
  let { fields, path, label, value }: { fields: AxoFields; path: string[]; label: string; value: number | null } =
    $props()
</script>

<SettingRow {label}>
  <Input
    class="w-28"
    type="number"
    min="0"
    step="1"
    value={fields.value(path, value)}
    disabled={!fields.settings}
    oninput={(event) => fields.number(path, event.currentTarget)}
    placeholder="Default"
  />
</SettingRow>
