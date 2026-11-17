import type { ChatSquashedChange } from '$lib/types/backend'
import { isPathWithin, splitRemotePath } from '$lib/utils/paths'
import { fileChange } from './elements/tool/payload'
import type { Step } from './turns'

export interface TurnChangedFile {
  path: string
  key: string
  url: string | null
  displayPath: string
  created: boolean
  beforeSha: string | null
  afterSha: string
  binary: boolean
  omitted: boolean
  mutations: number
  discontinuous: boolean
  additions: number | null
  deletions: number | null
}

export interface TurnChanges {
  raw: Record<string, unknown>[]
  files: TurnChangedFile[]
  needsBlobSquash: boolean
}

function displayChangePath(change: { path: string; url?: string | null }, folderPath: string): string {
  if (change.url) return change.url
  const root = splitRemotePath(folderPath)?.path ?? folderPath
  if (isPathWithin(root, change.path)) {
    return change.path.slice(root.replace(/\/+$/, '').length + 1) || '.'
  }
  return change.path
}

/** Collects and squashes net per-file changes across a turn's successful `edit` and `write` steps. */
export function turnFileChanges(steps: Step[], folderPath: string): TurnChanges {
  const raw: Record<string, unknown>[] = []
  const files: TurnChangedFile[] = []
  const byKey = new Map<string, TurnChangedFile>()

  for (const step of steps) {
    if (step.kind !== 'tool' || step.status !== 'done') continue
    if (step.name !== 'edit' && step.name !== 'write') continue
    const change = fileChange(step.result?.details)
    if (!change) continue
    raw.push(change.raw)

    const existing = byKey.get(change.key)
    const beforeSha = change.before?.sha256 ?? null
    const binary = (change.before?.binary ?? false) || change.after.binary
    const omitted = Boolean(change.before?.omitted || change.after.omitted)
    if (!existing) {
      const entry: TurnChangedFile = {
        path: change.path,
        key: change.key,
        url: change.url ?? null,
        displayPath: displayChangePath(change, folderPath),
        created: change.created,
        beforeSha,
        afterSha: change.after.sha256,
        binary,
        omitted,
        mutations: 1,
        discontinuous: false,
        additions: change.diff?.additions ?? null,
        deletions: change.diff?.deletions ?? null
      }
      byKey.set(change.key, entry)
      files.push(entry)
    } else {
      existing.discontinuous ||= beforeSha !== existing.afterSha
      existing.path = change.path
      existing.url = change.url ?? null
      existing.displayPath = displayChangePath(change, folderPath)
      existing.afterSha = change.after.sha256
      existing.binary ||= binary
      existing.omitted ||= omitted
      existing.mutations += 1
      existing.additions = null
      existing.deletions = null
    }
  }

  const retained = files.filter((file) => file.created || file.discontinuous || file.beforeSha !== file.afterSha)
  const needsBlobSquash = retained.some((file) => file.mutations >= 2 && !file.binary && !file.omitted)
  return { raw, files: retained, needsBlobSquash }
}

export function applySquashedChanges(squashed: ChatSquashedChange[], folderPath: string): TurnChangedFile[] {
  return squashed.map((file) => ({
    path: file.path,
    key: file.key,
    url: file.url,
    displayPath: displayChangePath(file, folderPath),
    created: file.created,
    beforeSha: file.beforeSha,
    afterSha: file.afterSha,
    binary: file.binary,
    omitted: file.omitted,
    mutations: file.mutations,
    discontinuous: file.discontinuous,
    additions: file.additions,
    deletions: file.deletions
  }))
}

export function summarizeTurnChanges(files: TurnChangedFile[]): {
  additions: number
  deletions: number
  hasUnknownCounts: boolean
} {
  let additions = 0
  let deletions = 0
  let hasUnknownCounts = false
  for (const file of files) {
    if (file.additions === null || file.deletions === null) {
      hasUnknownCounts = true
    } else {
      additions += file.additions
      deletions += file.deletions
    }
  }
  return { additions, deletions, hasUnknownCounts }
}
