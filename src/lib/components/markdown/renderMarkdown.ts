import type { Element, ElementContent, Root } from 'hast'
import { unified } from 'unified'
import remarkParse from 'remark-parse'
import remarkGfm from 'remark-gfm'
import { remarkAlert } from 'remark-github-blockquote-alert'
import remarkRehype from 'remark-rehype'
import rehypeRaw from 'rehype-raw'
import rehypeSlug from 'rehype-slug'
import rehypeSanitize, { defaultSchema, type Options } from 'rehype-sanitize'
import rehypeStringify from 'rehype-stringify'
import { SKIP, visit } from 'unist-util-visit'
import { platform } from '$lib/platform'
import { markdownImageSrc } from '$lib/utils/mediaAssets'
import { normalizeNestedMarkdownFences } from './markdownBlocks'

export interface MarkdownCodeBlock {
  lang: string
  code: string
}

function collectText(nodes: ElementContent[]): string {
  let out = ''
  for (const node of nodes) {
    if (node.type === 'text') out += node.value
    else if (node.type === 'element') out += collectText(node.children)
  }
  return out
}

function codeLanguage(pre: Element): { lang: string; fromCodeElement: boolean } {
  const first = pre.children[0]
  if (first?.type !== 'element' || first.tagName !== 'code') {
    return { lang: '', fromCodeElement: false }
  }
  const classes = Array.isArray(first.properties.className) ? first.properties.className : []
  for (const name of classes) {
    const value = String(name)
    if (value.startsWith('language-')) return { lang: value.slice('language-'.length), fromCodeElement: true }
  }
  return { lang: '', fromCodeElement: true }
}

const markdownSchema: Options = {
  ...defaultSchema,
  tagNames: [...(defaultSchema.tagNames ?? []), 'svg', 'path'],
  attributes: {
    ...defaultSchema.attributes,
    div: [...(defaultSchema.attributes?.div ?? []), ['className', /^markdown-alert(?:-\w+)?$/]],
    p: [['className', 'markdown-alert-title']],
    svg: [['className', 'octicon'], 'viewBox', 'width', 'height', 'ariaHidden'],
    path: ['d']
  },
  protocols: {
    ...defaultSchema.protocols,
    // App links are dispatched through openLink, never browser navigation.
    href: [
      'http',
      'https',
      'mailto',
      'file',
      'issue',
      'pr',
      'omp',
      'skill',
      'rule',
      'local',
      'artifact',
      'history',
      'agent',
      'memory'
    ],
    // Embedded images are MIME-checked after sanitization; data links stay blocked.
    src: [...(defaultSchema.protocols?.src ?? []), 'data']
  }
}

const webMarkdownSchema: Options = {
  ...markdownSchema,
  protocols: {
    ...markdownSchema.protocols,
    src: [...(markdownSchema.protocols?.src ?? []), 'file']
  }
}

export async function renderMarkdown(
  source: string,
  folderPath?: string,
  filePath?: string | null,
  loadLocalImage?: (filePath: string) => Promise<string | null>
): Promise<{ html: string; images: Promise<string | null>[]; codeBlocks: MarkdownCodeBlock[] }> {
  // Web admits file: image URLs only to replace them with text; desktop retains its original sanitizer.
  const schema = 'url' in platform.assets ? markdownSchema : webMarkdownSchema
  const images: Promise<string | null>[] = []
  const codeBlocks: MarkdownCodeBlock[] = []
  const processor = unified()
    .use(remarkParse)
    .use(remarkGfm)
    .use(remarkAlert)
    // Sanitize owns the single user-content- prefix, including footnote IDs.
    .use(remarkRehype, { allowDangerousHtml: true, clobberPrefix: '' })
    .use(rehypeRaw)
    .use(rehypeSlug)
    // Sanitize raw HTML and generated IDs before trusted asset/code-slot transforms.
    .use(rehypeSanitize, schema)
    .use(() => (tree: Root) => {
      visit(tree, 'element', (node, index, parent) => {
        if (node.tagName === 'img' && typeof node.properties.src === 'string') {
          const src = node.properties.src.trim()
          if (/^data:/i.test(src.replace(/[\t\n\r]/g, '')) && !/^data:image\/[a-z0-9.+-]+(?:;[^,]*)?,/i.test(src)) {
            delete node.properties.src
          } else {
            // Trusted URLs are applied after publication; image I/O must not block the document.
            node.properties.dataAxomeImage = String(images.length)
            node.properties.ariaBusy = 'true'
            delete node.properties.src
            images.push(markdownImageSrc(src, folderPath, filePath, loadLocalImage).catch(() => null))
          }
          return
        }
        if (node.tagName === 'pre' && parent && index !== undefined) {
          const { lang, fromCodeElement } = codeLanguage(node)
          const raw = collectText(node.children)
          const code = fromCodeElement ? raw.replace(/\n$/, '') : raw
          const slotIndex = codeBlocks.length
          codeBlocks.push({ lang, code })
          parent.children[index] = {
            type: 'element',
            tagName: 'div',
            properties: {
              className: ['axome-code-slot'],
              dataAxomeCode: String(slotIndex)
            },
            children: []
          }
          return [SKIP, index + 1]
        }
      })
    })
    .use(rehypeStringify)

  return { html: String(await processor.process(normalizeNestedMarkdownFences(source))), images, codeBlocks }
}
