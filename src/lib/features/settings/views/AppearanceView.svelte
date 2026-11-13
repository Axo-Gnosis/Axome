<!--
  @component
  AppearanceView — theme readout, typography, and webview zoom.

  Only "Dark" is supported today; zoom is available when the platform
  provides native window scaling. Font names are freeform: any installed
  family works, and the bundled fonts cover names that are not installed.

-->

<script lang="ts">
  import { platform } from '$lib/platform'
  import { Badge } from '$lib/components/ui/badge'
  import { Button } from '$lib/components/ui/button'
  import { Input } from '$lib/components/ui/input'
  import { getZoomLevel, zoomIn, zoomOut, zoomReset } from '$lib/features/app-shell/zoom/state.svelte'
  import { notify } from '$lib/features/notifications/state.svelte'
  import { getDesktopSettings, saveTypographySettings } from '$lib/features/settings/state/settings.svelte'
  import { MAX_FONT_SIZE, MIN_FONT_SIZE } from '$lib/features/settings/state/typography.svelte'
  import { getErrorMessage } from '$lib/utils/client-error'
  import { createAutoSaver } from './autoSaver.svelte'
  import SettingRow from '../dialog/SettingRow.svelte'
  import SettingsSection from '../dialog/SettingsSection.svelte'

  let zoom = $derived(getZoomLevel())
  let desktop = $derived(getDesktopSettings())

  type Draft = {
    uiFont: string
    codeFont: string
    codeSize: number | string
    terminalFont: string
    terminalSize: number | string
  }
  let draft = $state<Draft | null>(null)
  let values = $derived(
    draft ?? {
      uiFont: desktop?.typography.uiFontFamily ?? '',
      codeFont: desktop?.typography.codeFontFamily ?? '',
      codeSize: desktop?.typography.codeFontSize ?? '',
      terminalFont: desktop?.typography.terminalFontFamily ?? '',
      terminalSize: desktop?.typography.terminalFontSize ?? ''
    }
  )

  const saver = createAutoSaver()

  /** Empty or non-numeric input reads as unset; anything else clamps into the schema's range. */
  function fontSize(value: number | string | null): number | null {
    const size = value === '' || value == null ? NaN : Number(value)
    if (!Number.isFinite(size)) return null
    return Math.min(MAX_FONT_SIZE, Math.max(MIN_FONT_SIZE, Math.round(size)))
  }

  function schedule() {
    const current = getDesktopSettings()?.typography
    if (!current) return
    const next = {
      uiFontFamily: values.uiFont.trim(),
      codeFontFamily: values.codeFont.trim(),
      codeFontSize: fontSize(values.codeSize) ?? current.codeFontSize,
      terminalFontFamily: values.terminalFont.trim() || null,
      terminalFontSize: fontSize(values.terminalSize)
    }
    saver.schedule('typography', async () => {
      try {
        await saveTypographySettings(next)
      } catch (error) {
        notify.error('Failed to save typography settings', getErrorMessage(error))
      }
    })
  }
</script>

<SettingsSection title="Theme">
  <SettingRow label="Color scheme" description="Custom themes are in the works." as="div">
    <span class="rounded-md border border-edge bg-surface px-3 py-1.5 text-sm text-fg">Dark</span>
    <Badge>WIP</Badge>
  </SettingRow>
</SettingsSection>

<SettingsSection
  title="Typography"
  description="Any installed font name. Bundled fonts fill in for names that are not installed."
>
  <SettingRow label="Interface font">
    <Input
      class="w-64"
      value={values.uiFont}
      oninput={(event) => {
        draft = { ...values, uiFont: event.currentTarget.value }
        schedule()
      }}
      placeholder="Paper Mono UI"
      disabled={!desktop}
    />
  </SettingRow>

  <SettingRow label="Code font" description="Also covers diffs and monospace interface text.">
    <Input
      class="w-64"
      value={values.codeFont}
      oninput={(event) => {
        draft = { ...values, codeFont: event.currentTarget.value }
        schedule()
      }}
      placeholder="Paper Mono"
      disabled={!desktop}
    />
  </SettingRow>

  <SettingRow label="Code font size">
    <Input
      class="w-24"
      type="number"
      min={MIN_FONT_SIZE}
      max={MAX_FONT_SIZE}
      value={values.codeSize}
      oninput={(event) => {
        draft = { ...values, codeSize: event.currentTarget.value }
        schedule()
      }}
      disabled={!desktop}
    />
    <span class="text-sm text-subtle">px</span>
  </SettingRow>

  <SettingRow label="Terminal font" description="Leave empty to follow the code font.">
    <Input
      class="w-64"
      value={values.terminalFont}
      oninput={(event) => {
        draft = { ...values, terminalFont: event.currentTarget.value }
        schedule()
      }}
      placeholder={values.codeFont.trim() || 'Paper Mono'}
      disabled={!desktop}
    />
  </SettingRow>

  <SettingRow label="Terminal font size" description="Leave empty to follow the code font size.">
    <Input
      class="w-24"
      type="number"
      min={MIN_FONT_SIZE}
      max={MAX_FONT_SIZE}
      value={values.terminalSize}
      oninput={(event) => {
        draft = { ...values, terminalSize: event.currentTarget.value }
        schedule()
      }}
      placeholder={String(fontSize(values.codeSize) ?? 13)}
      disabled={!desktop}
    />
    <span class="text-sm text-subtle">px</span>
  </SettingRow>
</SettingsSection>

{#if platform.native}
  <SettingsSection title="Zoom">
    <SettingRow label="Webview scale" description="Useful on HiDPI displays where native px sizes feel small." as="div">
      <Button size="sm" variant="outline" onclick={zoomOut} disabled={zoom <= 0.5}>−</Button>
      <span class="w-14 text-center font-mono text-sm text-fg">{Math.round(zoom * 100)}%</span>
      <Button size="sm" variant="outline" onclick={zoomIn} disabled={zoom >= 2.0}>+</Button>
      <Button size="sm" variant="ghost" onclick={zoomReset} disabled={zoom === 1.0}>Reset</Button>
    </SettingRow>
  </SettingsSection>
{/if}
