<!--
  @component
  CodeBlock — framed fenced code block with language header, word-wrap toggle,
  copy action, and Preview/Raw switcher for markdown blocks.
-->

<script lang="ts">
  import { onDestroy } from 'svelte'
  import { IconButton } from '$lib/components/ui/button'
  import * as Tabs from '$lib/components/ui/tabs'
  import Icon from '$lib/icons/Icon.svelte'
  import { copyToClipboard } from '$lib/utils/clipboard'
  import { cn } from '$lib/utils/cn'
  import { highlightSpans, type HighlightSpan } from '$lib/utils/syntaxHighlight'
  import { CopyIcon, ChevronsDownUpIcon, TextWrapIcon, Tick02Icon, UnfoldMoreIcon } from '@hugeicons/core-free-icons'
  import { MARKDOWN_FENCE_LANGS } from './markdownBlocks'
  import MarkdownRenderer from './MarkdownRenderer.svelte'

  const PLAIN_LANGS = new Set(['', 'text', 'plaintext', 'txt'])
  const MAX_PREVIEW_DEPTH = 3
  const MAX_SCROLL_PX = 288

  let {
    code,
    lang = '',
    label,
    expanded = $bindable(false),
    folderPath,
    sessionId,
    filePath,
    depth = 0
  }: {
    code: string
    lang?: string
    label?: string
    expanded?: boolean
    folderPath?: string
    sessionId?: string | null
    filePath?: string | null
    depth?: number
  } = $props()

  const trimmedLang = $derived(lang.trim().split(/\s+/, 1)[0] ?? '')
  const normalizedLang = $derived((trimmedLang.split(',', 1)[0] ?? '').toLowerCase())
  const displayLang = $derived(label ?? (trimmedLang || 'text'))
  const isPreviewable = $derived(MARKDOWN_FENCE_LANGS.has(normalizedLang) && depth < MAX_PREVIEW_DEPTH)

  let raw = $state(false)
  const previewing = $derived(isPreviewable && !raw)

  let previewOverflows = $state(false)
  let rawOverflows = $state(false)
  const overflows = $derived(previewing ? previewOverflows : rawOverflows)

  let wrap = $state(false)
  let copied = $state(false)
  let copyTimer: ReturnType<typeof setTimeout> | undefined

  let highlighted = $state.raw<{ lang: string; code: string; spans: HighlightSpan[] } | null>(null)

  const renderedSpans = $derived.by((): HighlightSpan[] | null => {
    if (!highlighted || highlighted.lang !== normalizedLang) return null
    if (highlighted.code === code) return highlighted.spans
    if (code.startsWith(highlighted.code)) {
      return [...highlighted.spans, { text: code.slice(highlighted.code.length) }]
    }
    return null
  })

  $effect(() => {
    if (previewing) return
    const source = code
    const language = normalizedLang
    if (PLAIN_LANGS.has(language) || !source) {
      highlighted = null
      return
    }
    let cancelled = false
    void highlightSpans(source, language)
      .then((spans) => {
        if (cancelled) return
        highlighted = spans ? { lang: language, code: source, spans } : null
      })
      .catch((cause) => console.warn('Code block syntax highlighting failed', cause))
    return () => {
      cancelled = true
    }
  })

  async function copy() {
    await copyToClipboard(code, 'Copy code failed')
    copied = true
    clearTimeout(copyTimer)
    copyTimer = setTimeout(() => {
      copied = false
      copyTimer = undefined
    }, 1500)
  }

  function measureBody(forPreview: boolean) {
    return (element: HTMLElement) => {
      void code
      void previewing
      void wrap
      void expanded
      const check = () => {
        if (previewing !== forPreview || element.getClientRects().length === 0) return
        const next = element.scrollHeight > MAX_SCROLL_PX + 4
        if (forPreview) previewOverflows = next
        else rawOverflows = next
      }
      check()
      const observer = new ResizeObserver(check)
      observer.observe(element)
      return () => observer.disconnect()
    }
  }

  onDestroy(() => clearTimeout(copyTimer))
</script>

<div
  data-slot="code-block"
  class={cn('min-w-0 overflow-hidden rounded-md border border-edge not-italic', depth > 0 ? 'bg-surface' : 'bg-ground')}
>
  <div
    class={cn(
      'flex min-h-8 items-center justify-between gap-2 border-b border-edge px-3 py-1',
      depth > 0 ? 'bg-raised' : 'bg-surface'
    )}
  >
    <span class={cn('truncate font-mono text-xs', label || trimmedLang ? 'text-muted' : 'text-subtle')}
      >{displayLang}</span
    >
    <div class="flex shrink-0 items-center gap-1">
      {#if isPreviewable}
        <Tabs.Root
          value={previewing ? 'preview' : 'raw'}
          onValueChange={(value) => {
            raw = value === 'raw'
          }}
        >
          <Tabs.List>
            <Tabs.Trigger value="preview" class="px-2 py-0.5 text-xs">Preview</Tabs.Trigger>
            <Tabs.Trigger value="raw" class="px-2 py-0.5 text-xs">Raw</Tabs.Trigger>
          </Tabs.List>
        </Tabs.Root>
      {/if}
      {#if !previewing}
        <IconButton
          tooltip={wrap ? 'Disable word wrap' : 'Enable word wrap'}
          active={wrap}
          onclick={() => (wrap = !wrap)}
        >
          <Icon icon={TextWrapIcon} size={14} />
        </IconButton>
      {/if}
      {#if overflows || expanded}
        <IconButton
          tooltip={expanded ? 'Collapse height' : 'Expand full'}
          active={expanded}
          onclick={() => (expanded = !expanded)}
        >
          <Icon icon={expanded ? ChevronsDownUpIcon : UnfoldMoreIcon} size={14} />
        </IconButton>
      {/if}
      <IconButton tooltip={copied ? 'Copied' : 'Copy code'} onclick={() => void copy()}>
        <Icon icon={copied ? Tick02Icon : CopyIcon} size={14} class={copied ? 'text-success' : undefined} />
      </IconButton>
    </div>
  </div>
  {#if isPreviewable}
    <div
      {@attach measureBody(true)}
      class={cn('px-4 py-3', !expanded && 'max-h-72 overflow-y-auto', !previewing && 'hidden')}
    >
      <MarkdownRenderer source={code} {folderPath} {sessionId} {filePath} depth={depth + 1} class="p-0" />
    </div>
  {/if}
  {#if !previewing}
    <div
      data-slot="code-block-raw"
      {@attach measureBody(false)}
      class={cn(
        'px-3.5 py-3 font-mono text-sm leading-relaxed text-fg',
        !expanded && 'max-h-72 overflow-y-auto',
        wrap ? 'overflow-x-hidden' : 'overflow-x-auto'
      )}
    >
      <pre class={cn('m-0 font-mono', wrap ? 'wrap-anywhere whitespace-pre-wrap' : 'whitespace-pre')}><code
          >{#if renderedSpans}{#each renderedSpans as span}<span style:color={span.color}>{span.text}</span
              >{/each}{:else}{code}{/if}</code
        ></pre>
    </div>
  {/if}
</div>
