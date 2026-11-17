<script lang="ts">
  import { backend } from '$lib/api/backend'
  import { filePathToLanguage } from '$lib/features/editor/languageMap'
  import type { DiffContentFetcher } from '$lib/features/editor/renderers/monaco/diff/diffModels.svelte'
  import type { TurnDiffSource } from '$lib/features/workbench/model'
  import DiffStack from '$lib/features/workbench/surfaces/diff/DiffStack.svelte'
  import type { FileDiff } from '$lib/types/backend'

  let {
    source,
    folderPath,
    initialFile = null
  }: {
    source: TurnDiffSource
    folderPath: string
    initialFile?: string | null
  } = $props()

  const files: FileDiff[] = $derived(
    source.files.map((file) => ({
      path: file.path,
      oldPath: null,
      status: file.created ? 'added' : 'modified',
      lang: filePathToLanguage(folderPath, file.path),
      additions: file.additions,
      deletions: file.deletions
    }))
  )

  const contentFetcher: DiffContentFetcher = async (entry) => {
    const file = source.files.find((candidate) => candidate.path === entry.path)
    if (!file || file.binary || file.omitted) {
      return { oldContent: null, newContent: null, binary: true }
    }
    return backend.chat.turnDiffFile(folderPath, source.sessionId, file.beforeSha, file.afterSha)
  }
</script>

<DiffStack
  {files}
  {initialFile}
  scrollNonce={source.revealNonce}
  label={source.label}
  idPrefix="turn-file"
  {folderPath}
  sessionId={source.sessionId}
  {contentFetcher}
/>
