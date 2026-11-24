import type { ChatSquashedChange } from '$lib/types/backend'
import { displayFilePath, fileRecords } from './elements/tool/payload'
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
  unfinished: boolean
}

export interface TurnChanges {
  raw: Record<string, unknown>[]
  files: TurnChangedFile[]
  needsBlobSquash: boolean
}

/** Collects and squashes net per-file changes from every recorded mutation in a turn. */
export function turnFileChanges(steps: Step[], folderPath: string): TurnChanges {
  const raw: Record<string, unknown>[] = []
  const files: TurnChangedFile[] = []
  const byKey = new Map<string, TurnChangedFile>()

  for (const step of steps) {
    if (step.kind !== 'tool' || !step.result) continue
    const unfinished = step.result.isError
    for (const { change } of fileRecords(step.result.details)) {
      if (!change) continue
      raw.push(change.raw)

      const existing = byKey.get(change.key)
      const beforeSha = change.before?.sha256 ?? null
      const binary = (change.before?.binary ?? false) || change.after.binary
      const omitted = Boolean(change.before?.omitted || change.after.omitted)
      const displayPath = change.url ?? displayFilePath(change.path, folderPath)
      if (!existing) {
        const entry: TurnChangedFile = {
          path: change.path,
          key: change.key,
          url: change.url ?? null,
          displayPath,
          created: change.created,
          beforeSha,
          afterSha: change.after.sha256,
          binary,
          omitted,
          mutations: 1,
          discontinuous: false,
          additions: change.diff?.additions ?? null,
          deletions: change.diff?.deletions ?? null,
          unfinished
        }
        byKey.set(change.key, entry)
        files.push(entry)
      } else {
        existing.discontinuous ||= beforeSha !== existing.afterSha
        existing.path = change.path
        existing.url = change.url ?? null
        existing.displayPath = displayPath
        existing.afterSha = change.after.sha256
        existing.binary ||= binary
        existing.omitted ||= omitted
        existing.mutations += 1
        existing.additions = null
        existing.deletions = null
        existing.unfinished ||= unfinished
      }
    }
  }

  const retained = files.filter((file) => file.created || file.discontinuous || file.beforeSha !== file.afterSha)
  const needsBlobSquash = retained.some((file) => file.mutations >= 2 && !file.binary && !file.omitted)
  return { raw, files: retained, needsBlobSquash }
}

export function applySquashedChanges(
  squashed: ChatSquashedChange[],
  folderPath: string,
  original: TurnChangedFile[]
): TurnChangedFile[] {
  const unfinished = new Set<string>()
  for (const file of original) if (file.unfinished) unfinished.add(file.key)
  return squashed.map((file) => ({
    path: file.path,
    key: file.key,
    url: file.url,
    displayPath: file.url ?? displayFilePath(file.path, folderPath),
    created: file.created,
    beforeSha: file.beforeSha,
    afterSha: file.afterSha,
    binary: file.binary,
    omitted: file.omitted,
    mutations: file.mutations,
    discontinuous: file.discontinuous,
    additions: file.additions,
    deletions: file.deletions,
    unfinished: unfinished.has(file.key)
  }))
}

export function summarizeTurnChanges(files: TurnChangedFile[]): {
  additions: number
  deletions: number
  hasUnknownCounts: boolean
  unfinished: number
} {
  let additions = 0
  let deletions = 0
  let hasUnknownCounts = false
  let unfinished = 0
  for (const file of files) {
    if (file.unfinished) unfinished += 1
    if (file.additions === null || file.deletions === null) {
      hasUnknownCounts = true
    } else {
      additions += file.additions
      deletions += file.deletions
    }
  }
  return { additions, deletions, hasUnknownCounts, unfinished }
}
