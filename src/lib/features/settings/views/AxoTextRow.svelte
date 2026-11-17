<script lang="ts">
  import { Input } from '$lib/components/ui/input'
  import SettingRow from '../dialog/SettingRow.svelte'
  import type { AxoFields } from './axoFields.svelte'

  let {
    fields,
    path,
    label,
    value,
    placeholder,
    description
  }: {
    fields: AxoFields
    path: string[]
    label: string
    value: string | null
    placeholder: string
    description?: string
  } = $props()
</script>

<SettingRow {label} {description} stacked>
  <Input
    value={fields.value(path, value)}
    {placeholder}
    disabled={!fields.settings}
    oninput={(event) => fields.save(path, event.currentTarget.value.trim() || null, event.currentTarget.value)}
  />
</SettingRow>
