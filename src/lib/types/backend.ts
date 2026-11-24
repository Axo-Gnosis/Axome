// Typed interfaces for the Rust backend IPC responses.
// Keep in sync with types in src-crates/axome-protocol/src/.

import type { TextRevealTarget } from '$lib/features/workbench/surfaces/text/service.svelte'

export interface PtyEvent {
  type: 'started' | 'exit' | 'error' | 'sessionIdBound' | 'synced'
  run_id: string
  pid?: number | null
  code?: number | null
  message?: string
  session_id?: string
  sequence?: number
}

export interface TerminalTransferState {
  runId: string | null
  harnessId?: string
  serializedBuffer: string
  cols: number
  rows: number
  viewportPosition: number
  lastSequence: number
  status?: string
}

export interface TextModelTransferState {
  tabId: string
  folderPath: string | null
  filePath: string | null
  value: string
  savedValue: string
  language: string
  viewState: unknown | null
}
export interface TabTransferInitiateParams {
  sourceWindow: string
  targetWindow: string
  tabId: string
  targetIndex: number
}

export interface TabTransferExportPayload {
  transferId: string
  tab: unknown
  terminalState: TerminalTransferState | null
  modelState: TextModelTransferState | null
}

export interface TabTransferAbortedPayload {
  transferId: string
  reason: string
  ptyLost: boolean
}

export interface FocusTabPayload {
  tabId: string
  reveal: TextRevealTarget | null
}

export interface FilePathChangedPayload {
  oldPath: string
  newPath: string
  folderPath: string
}

export type OpenTarget =
  | { type: 'folder'; folder_path: string }
  | { type: 'file'; folder_path: string; file_path: string }
  | { type: 'workbench'; server: string; workbench_id: string }

export interface FolderInfo {
  path: string
  name: string
}

export interface PathRoot {
  kind: 'home' | 'volume'
  label: string
  path: string
}

export interface FolderEntry {
  name: string
  path: string
  is_dir: boolean
}

export interface AppRuntimeInfo {
  name: string
  version: string
  memory_bytes: number | null
  app_cpu_time_ticks: number | null
  system_cpu_time_ticks: number | null
  thread_count: number | null
  file_descriptor_count: number | null
}

/** Server-owned durable workbench; `connected`/`running` are live observations. */
export interface WorkbenchInfo {
  id: string
  created_at: string
  last_seen_at: string
  connected: boolean
  client: string | null
  yours: boolean
  /** Tab folders from the saved layout, active tab's folder first. */
  folders: string[]
  running: WorkbenchRun[]
}

export type AttachMode = { kind: 'open' } | { kind: 'resume'; controller_token: string } | { kind: 'takeover' }

export type WorkbenchAttached =
  | { kind: 'ready'; attachment_id: string; controller_token: string; snapshot: string }
  | { kind: 'busy' | 'revoked'; client: string | null; snapshot: string }

/** A live process a workbench left running. */
export type WorkbenchRun =
  | { kind: 'session'; folder: string; harness_id: string; preset: string | null }
  | { kind: 'shell'; folder: string; preset: string | null }
  | { kind: 'task'; folder: string; task_id: string }
  | { kind: 'chat'; folder: string }

/** A folder opened in Axome, most recent first. */
export interface RecentFolder {
  path: string
  opened_at: string
}

export interface SessionStartInfo {
  resumed: boolean
  /** Session id the process is known to own at spawn time; null until discovery binds one. */
  sessionId: string | null
}

export type SessionSource = { kind: 'harness'; harnessId: string; presets: (string | null)[] } | { kind: 'axo' }

export type SessionSourceKey = { kind: 'harness'; harnessId: string; preset: string | null } | { kind: 'axo' }

export interface IndexedChildSession {
  sessionId: string
  agent: string | null
  teamMember: string | null
  title: string | null
  model: string | null
  updatedAt: string | null
}

export interface IndexedSession {
  source: SessionSource
  sessionId: string
  title: string | null
  lastMessage: { role: 'user' | 'agent'; text: string } | null
  messageCount: number | null
  updatedAt: string | null
  model: string | null
  gitBranch: string | null
  children: IndexedChildSession[]
}

export type SessionIndexStatus = 'pending' | 'ready' | 'missing' | 'error'

export interface SessionIndexSource {
  source: SessionSourceKey
  status: SessionIndexStatus
  refreshing: boolean
  refreshedAt: string | null
  message: string | null
}

export interface SessionIndexSnapshot {
  sessions: IndexedSession[]
  sources: SessionIndexSource[]
}

export type ChatThinking = 'off' | 'minimal' | 'low' | 'medium' | 'high' | 'xhigh' | 'max'
export type ChatServiceTier = 'standard' | 'flex' | 'fast' | 'ultrafast'

export interface ChatModel {
  selector: string
  provider: string
  origin: string | null
  name: string
  thinking: ChatThinking[]
  serviceTiers: Partial<Record<ChatServiceTier, number>>
  contextWindow: number
  /** Accepts image input. */
  images: boolean
}

export type AxoCredentialSource =
  { kind: 'config' } | { kind: 'apiKey' } | { kind: 'oauth' } | { kind: 'env'; name: string } | { kind: 'none' }

export interface AxoProvider {
  id: string
  origin: string | null
  originProject: boolean
  hides: AxoHiddenProvider | null
  env: string[]
  requiresKey: boolean
  oauthOnly: boolean
  login: boolean
  status: { kind: 'ready' } | { kind: 'needsCredentials' } | { kind: 'error'; message: string }
  source: AxoCredentialSource
  models: number
}

export interface AxoModelsFile {
  path: string
  error: string | null
}

export interface AxoProvidersPayload {
  providers: AxoProvider[]
  modelsFile: AxoModelsFile
}

export interface AxoLoginStarted {
  loginId: string
}

export type AxoLoginQuestion = { id: number; message: string } & (
  | { kind: 'prompt'; placeholder: string | null; secret: boolean }
  | { kind: 'select'; options: { id: string; label: string }[] }
)

export interface AxoLoginView {
  url: { url: string; instructions: string | null } | null
  deviceCode: { userCode: string; verificationUrl: string; expiresInSeconds: number | null } | null
  progress: string | null
  question: AxoLoginQuestion | null
}

export type AxoLoginStatus =
  { state: 'pending'; view: AxoLoginView } | { state: 'done' } | { state: 'failed'; message: string }

export interface AxoKeywordInfo {
  name: string
  description: string
}

export interface AxoSearchProviderStatus {
  id: string
  label: string
  requiresKey: boolean
  env: string[]
  configured: boolean
  source: AxoCredentialSource
}

export interface AxoDocsSourceInfo {
  key: string
  kind: string
  docPath: string
  sections: number
  stale: boolean
}

export interface AxoWebToolsStatusPayload {
  searchProviders: AxoSearchProviderStatus[]
  docsSources: AxoDocsSourceInfo[]
  cacheBytes: number
}

export interface ChatModelRole {
  name: string
  model: string | null
}

export interface ChatModelCatalog {
  models: ChatModel[]
  roles: ChatModelRole[]
  defaultModel: string | null
  defaultThinking: ChatThinking
  defaultServiceTier: ChatServiceTier
}

export type ChatContent = { type: 'text'; text: string } | { type: 'image'; mimeType: string; data: string }

export type ChatBlock =
  | { type: 'text'; text: string }
  | { type: 'thinking'; text: string; redacted: boolean }
  | { type: 'toolCall'; id: string; name: string; arguments: unknown }

export interface ChatUsage {
  input: number
  output: number
  cacheRead: number
  cacheWrite: number
  totalTokens: number
  cost: number | null
}

export type ChatStopReason = 'stop' | 'length' | 'toolUse' | 'error' | 'aborted'
export type ChatRunOutcome = 'done' | 'aborted' | 'failed' | 'interrupted'

export interface ChatEntry {
  id: string
  timestamp: string
  body: ChatEntryBody
}

/** One part of a user message, mirroring `axo::PromptPart`. `content` is input only; axo stores it as `saved`. */
export type ChatPart =
  | { type: 'text'; text: string }
  | { type: 'paste'; text: string }
  | { type: 'file'; path: string }
  | { type: 'content'; name: string; /** Base64. */ bytes: string }
  | { type: 'saved'; url: string; image: boolean }

export type ChatKiller = 'model' | 'host'

export type ChatCommandExit =
  | { type: 'exited'; code: number }
  | { type: 'signaled'; signal: number }
  | { type: 'timedOut'; afterSecs: number }
  | { type: 'killed'; by: ChatKiller }
  | { type: 'aborted' }
  | { type: 'interrupted' }

export interface ChatTeamRef {
  id: string
  member: string
  name: string | null
}

export type ChatDeliverable =
  | {
      kind: 'command'
      jobId: string
      command: string
      exit: ChatCommandExit | null
      isError: boolean
    }
  | {
      kind: 'child'
      jobId: string
      agent: string | null
      team: ChatTeamRef | null
      note: boolean
      partial: boolean
      isError: boolean
      text: string
      value: unknown | null
    }

export type ChatNotice =
  | { kind: 'jobWake'; jobs: string[]; deliverables: ChatDeliverable[] }
  | { kind: 'mail'; sender: string; text: string }
  | { kind: 'turnBudget'; text: string }
  | { kind: 'yieldNudge'; text: string }
  | { kind: 'host'; text: string }
  | { kind: 'extension'; id: string; text: string }

/** Who submitted a user entry when it did not come directly from the user. */
export type ChatSender = { kind: 'agent'; name: string | null } | { kind: 'extension'; id: string }

export type ChatEntryBody =
  | {
      kind: 'user'
      parts: ChatPart[]
      anchor: string | null
      siblings: string[]
      from: ChatSender | null
      /** Names of the keywords that fired for this submission, first occurrence first. */
      keywords: string[]
    }
  | {
      kind: 'assistant'
      blocks: ChatBlock[]
      model: string
      serviceTier: ChatServiceTier
      thinking: ChatThinking | null
      usage: ChatUsage
      stopReason: ChatStopReason
      error: string | null
      providerUnavailable: boolean
      durationMs: number | null
    }
  | {
      kind: 'toolResult'
      callId: string
      toolName: string
      content: ChatContent[]
      details: unknown | null
      isError: boolean
      deliverables: ChatDeliverable[]
      durationMs: number | null
    }
  | { kind: 'notice'; notice: ChatNotice }
  | { kind: 'modelChange'; model: string }
  | { kind: 'thinkingChange'; thinking: ChatThinking; automatic: boolean; serviceTier: ChatServiceTier | null }
  | { kind: 'serviceTierChange'; serviceTier: ChatServiceTier }
  | { kind: 'agentChange'; agent: string | null }
  | { kind: 'modeChange'; mode: string | null }
  | { kind: 'compaction'; summary: string; tokensBefore: number }
  | { kind: 'branchSummary'; summary: string }
  | { kind: 'contextMoved'; blocks: number }
  | { kind: 'runEnd'; outcome: ChatRunOutcome; reason: string | null }

export type ChatMaintenance = 'compact' | 'shake' | 'branchSummary'

export interface ChatToolRun {
  callId: string
  name: string
  arguments: unknown
  output: ChatContent[]
  details: unknown | null
}

export type ChatQueueKind = 'steer' | 'followUp'

export interface ChatQueued {
  submissionId: string
  kind: ChatQueueKind
  parts: ChatPart[]
  from: ChatSender | null
}

export interface ChatAskOption {
  value: string
  label: string
  description: string | null
  recommended: boolean
}

export type ChatAskBlock =
  | { type: 'markdown'; text: string }
  | {
      type: 'choice'
      id: string
      question: string
      optional: boolean
      options: ChatAskOption[]
      multi: boolean
    }
  | {
      type: 'text'
      id: string
      question: string
      optional: boolean
      multiline: boolean
      placeholder: string | null
      prefill: string | null
    }
  | {
      type: 'number'
      id: string
      question: string
      optional: boolean
      integer: boolean
      min: number | null
      max: number | null
      default: number | null
    }

export type ChatAnswerValue = string | number | string[]

export interface ChatAnswer {
  id: string
  value?: ChatAnswerValue
  other?: string
  note?: string
}

export interface ChatQuestions {
  callId: string
  blocks: ChatAskBlock[]
  deadlineMs: number | null
}

export interface ChatRetry {
  attempt: number
  maxRetries: number
  delayMs: number
  error: string | null
}

export interface ChatStats {
  contextTokens: number
  contextWindow: number
  contextTrigger: number | null
  contextReserve: number | null
  usage: ChatUsage
  tree: ChatUsage | null
}

export type ChatJobStatus = 'queued' | 'running' | 'helpersRunning' | 'finished' | 'failed' | 'interrupted'
export type ChatPromotion = 'async' | 'timer' | 'steer' | 'host'

export interface ChatChildProgress {
  turns: number
  maxTurns: number
  tools: string[]
}

export type ChatJobKind =
  | {
      kind: 'command'
      command: string
      artifact: number
      promotion: ChatPromotion | null
      endedAt: number | null
      exit: ChatCommandExit | null
    }
  | {
      kind: 'child'
      sessionId: string
      agent: string | null
      model: string
      team: ChatTeamRef | null
      progress: ChatChildProgress | null
      unreadMail: number
    }

export interface ChatJob {
  id: string
  status: ChatJobStatus
  callId: string | null
  startedAt: number
  kind: ChatJobKind
}

export type ChatToolExposure = 'direct' | 'deferred'

export interface ChatSessionTool {
  name: string
  summary: string
  description: string
  parameters: Record<string, unknown>
  exposure: ChatToolExposure
  promoted: boolean
  allowedInMode: boolean
}

export type ChatOutputStream = 'stdout' | 'stderr' | 'merged'

export interface ChatOutputChunk {
  stream: ChatOutputStream
  text: string
}

export interface ChatJobOutput {
  chunks: ChatOutputChunk[]
  captureFailed: boolean
  truncated: boolean
  done: boolean
}

export interface ChatLiveSummary {
  sessionId: string
  running: boolean
  waiting: boolean
  autoPaused: boolean
  activeCommands: number
  activeChildren: number
}

export interface ChatSummariesChangedEvent {
  folderPath: string
  summaries: ChatLiveSummary[]
}

export interface ChatLive {
  title: string | null
  running: boolean
  maintenance: ChatMaintenance | null
  model: string | null
  thinking: ChatThinking
  serviceTier: ChatServiceTier
  effectiveServiceTier: ChatServiceTier
  tools: ChatToolRun[]
  queue: ChatQueued[]
  questions: ChatQuestions[]
  retry: ChatRetry | null
  autoPaused: boolean
  jobs: ChatJob[]
  stats: ChatStats
  /** The selected persona; `null` = Default. */
  agent: string | null
  /** The selected mode. */
  mode: string | null
  /** The mode the current model request runs under; differs from `mode` until the next request. */
  effectiveMode: string | null
  /** Mode results awaiting review. */
  reviews: ChatReview[]
}

/** A mode result awaiting review, mirroring `axo::PendingResult`. */
export interface ChatReview {
  callId: string
  mode: string
  result: unknown
  /** The mode is marked `axome: { plan: true }`. */
  plan: boolean
}

export type ChatCommandPolicy = 'alongside' | 'when_idle' | 'queued'

export type ChatCommandKind =
  | { kind: 'builtin'; /** Inlined JSON Schema object. */ parameters: Record<string, unknown> }
  | { kind: 'skill'; argumentHint: string | null }
  | { kind: 'mode'; argumentHint: string | null }
  | { kind: 'registered'; argumentHint: string | null }

/** One catalog entry, mirroring `axo::CommandInfo`. */
export type ChatCommand = {
  name: string
  description: string
  policy: ChatCommandPolicy
  origin: string | null
} & ChatCommandKind

export interface ChatSkillInfo {
  name: string
  description: string
  path: string
  modelInvocable: boolean
  argumentHint: string | null
}

export type ChatAgentRunAs = 'root' | 'subagent' | 'any'

export interface ChatAgentInfo {
  name: string
  description: string
  team: boolean
  runAs: ChatAgentRunAs | null
  model: string | null
  tools: string[] | null
  bashReadOnly: boolean
  maxTurns: number | null
  structuredOutput: boolean
  members: string[]
  path: string | null
}

export interface ChatModeInfo {
  name: string
  description: string
  argumentHint: string | null
  modelInvocable: boolean
  model: string | null
  toolsAllow: string[] | null
  toolsDeny: string[]
  bashReadOnly: boolean
  /** Where `write` and `edit` may write; `null` = unrestricted. */
  writes: string[] | null
  resultReviewed: boolean | null
  path: string | null
  /** The mode is marked `axome: { plan: true }`. */
  plan: boolean
}

/** What follows accepting a reviewed plan. */
export type ChatPlanThen = 'save' | 'work' | 'compactWork'

export interface ChatKeywordInfo {
  name: string
  description: string
  requiresTools: string[]
  model: string | null
  path: string | null
}

export interface ChatToolbox {
  skills: ChatSkillInfo[]
  agents: ChatAgentInfo[]
  modes: ChatModeInfo[]
  keywords: ChatKeywordInfo[]
  diagnostics: string[]
}

export interface ChatPromptSection {
  name: string
  body: string
}

export interface ChatContextFile {
  path: string
  content: string
  nested: boolean
}

export interface ChatContextDetails {
  prompt: ChatPromptSection[]
  contextFiles: ChatContextFile[]
}

/** The session's `local://` files and the canonical directory they live in. */
export interface ChatLocalFiles {
  /** Canonical absolute path of the `local://` root, for matching ledger paths under it. */
  root: string
  files: ChatLocalFile[]
}

/** A file under the session's `local://` root. */
export interface ChatLocalFile {
  /** Relative to the `local://` root, `/`-separated. */
  path: string
  size: number
}

/** A keyword occurrence in a composer text; offsets are UTF-16 code units, end exclusive. */
export interface ChatKeywordMatch {
  name: string
  start: number
  end: number
}

export interface ChatState {
  sessionId: string
  readOnly: boolean
  isChild: boolean
  entries: ChatEntry[]
  streaming: ChatBlock[] | null
  live: ChatLive
  commands: ChatCommand[]
  toolOrigins: Record<string, string>
  extensionDiagnostics: string[]
  sessionTools: ChatSessionTool[]
  toolbox: ChatToolbox
}

export type ChatDelta =
  | { type: 'start' }
  | { type: 'text'; index: number; text: string }
  | { type: 'thinking'; index: number; text: string }
  | { type: 'toolCall'; index: number; id: string; name: string; arguments: unknown | null }
  | { type: 'toolOutput'; callId: string; content: ChatContent[]; details: unknown | null }

export type ChatEvent =
  | { type: 'snapshot'; state: ChatState }
  | { type: 'entry'; entry: ChatEntry }
  | { type: 'live'; live: ChatLive }
  | { type: 'sessionTools'; tools: ChatSessionTool[] }
  | {
      type: 'extensionCatalog'
      commands: ChatCommand[]
      toolOrigins: Record<string, string>
      diagnostics: string[]
      toolbox: ChatToolbox
    }
  | { type: 'delta'; delta: ChatDelta }
  | { type: 'jobOutput'; jobId: string; reset: boolean; output: ChatJobOutput }
  | { type: 'error'; message: string }
  | { type: 'closed'; message: string; missing: boolean }
  | { type: 'reconnecting' }

export interface ChatForkInfo {
  sessionId: string
  editable: ChatPart[] | null
}

export type ChatLinkTarget =
  { kind: 'file'; path: string; isDir: boolean } | { kind: 'virtual'; text: string } | { kind: 'unhandled' }

export interface ChatSquashedChange {
  path: string
  key: string
  url: string | null
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

/** Everything the backend needs to spawn a session tab's process under an ephemeral run id. */
export interface SessionSpec {
  runId: string
  folderPath: string
  harnessId: string
  preset: string | null
  sessionId: string | null
}

export type HarnessConnectionStatus = 'connected' | 'missing' | 'error'

export interface HarnessStatus {
  id: string
  label: string
  status: HarnessConnectionStatus
  enabled: boolean
  presets: string[]
  version: string | null
  resolved_path: string | null
  message: string | null
  install_hint: string
}

/** One host's harnesses and the scope's configured default preset. */
export interface HarnessListing {
  harnesses: HarnessStatus[]
  default_preset: string | null
}

export type ExternalFolderOpenMode = 'newWindow' | 'focusedWindow'
export type ExternalFileOpenMode = 'preferFolder' | 'focusedWindow' | 'newWindow'
export type TabBeamPosition = 'top' | 'bottom'

export interface WindowSettings {
  theme: string
  externalFolderOpenMode?: ExternalFolderOpenMode
  externalFileOpenMode?: ExternalFileOpenMode
  tabBeamPosition?: TabBeamPosition
  homeFolder?: string | null
}

export interface TypographySettings {
  uiFontFamily: string
  codeFontFamily: string
  codeFontSize: number
  terminalFontFamily: string | null
  terminalFontSize: number | null
}

export interface NixSettings {
  evalTimeoutSecs: number
}

export interface ExplorerSettings {
  exclude: Record<string, boolean>
  excludeGitignore: boolean
  compactFolders: boolean
}

/** One row of a directory listing. A compacted chain's `name` carries the run. */
export interface ExplorerDirEntry {
  name: string
  path: string
  is_dir: boolean
  ignored: boolean
  excluded: boolean
  /** Directories a compacted chain swallowed, excluding `path` itself. */
  hops: string[]
}

export interface ExplorerPathList {
  paths: string[]
  truncated: boolean
}

/** A file's text plus the version of the bytes it was read from. */
export interface FileContent {
  content: string
  version: string
}

/** Project-relative directories whose contents changed; '' is the root. */
export interface FilesChangedEvent {
  folder_path: string
  dirs: string[]
}

/** Git freshness notification for one open folder. */
export interface GitChangedEvent {
  folder_path: string
  scope: 'summary' | 'all'
  /** Watcher failure; null for a normal filesystem change. */
  error: string | null
}

export interface FormattingLanguageSettings {
  formatter: string
}

export type FormattingSettings = Record<string, FormattingLanguageSettings>

export interface HarnessConfig {
  harnessId: string
  enabled: boolean
  binaryPathOverride: string | null
  extraArgs: string[]
}

export interface HarnessSettings {
  enabled: boolean
  binaryPathOverride: string | null
  extraArgs: string[]
  presets: Record<string, PresetSettings>
}

export interface PresetSettings {
  args: string[]
  env: Record<string, string>
}

export interface ShellPresetSettings {
  command?: string | null
  args: string[]
  env: Record<string, string>
  startup?: string | null
}

export interface ShellSettings {
  binaryPathOverride?: string | null
  extraArgs: string[]
  defaultPreset?: string | null
  presets: Record<string, ShellPresetSettings>
}

export interface ShellConfig {
  binaryPathOverride: string | null
  extraArgs: string[]
}

export interface ShellPresetEntry {
  name: string
  shell: string
}

export interface ShellListing {
  shell: string
  presets: ShellPresetEntry[]
  default_preset: string | null
}

export interface SessionsSettings {
  /** `<harness_id>` or `<harness_id>/<preset>`; launched by New Session and listed first. */
  defaultPreset: string | null
}

export interface RemoteSettings {
  address: string
  fingerprint: string
}

export interface RemoteStatus {
  connected: boolean
  last_error: string | null
  state: 'disconnected' | 'reconnecting' | 'connected' | 'error'
}

export interface RemoteStatusEvent extends RemoteStatus {
  server: string
}

export interface RemoteRunStatusEvent {
  runId: string
  state: 'reconnecting' | 'connected' | 'disconnected'
}

export interface FileStat {
  size: number
  version: string
  regular: boolean
}

export interface FileReadProgress {
  requestId: string
  folderPath: string
  filePath: string
  bytes: number
  total: number
}

export interface EffectiveLspServerSettings {
  enabled: boolean
  binaryPathOverride: string | null
  extraArgs: string[]
  trace: LspTraceLevel
  settings: unknown | null
}

export interface GitSettings {
  worktreeCopy: Record<string, boolean>
}

export type AxoQueueMode = 'all' | 'one-at-a-time'
export type AxoDevShellMode = 'off' | 'direnv' | 'nix'
export type AxoProjectAccess = 'ignore' | 'read' | 'trust'
export type AxoSummaryRequest = 'live' | 'separate'
export type AxoContextFiles = boolean | Record<string, boolean>

export interface AxoBashSettings {
  devShell: AxoDevShellMode
  devShellRef: string
  devShellWatch: string[]
  devShellTimeoutMs: number
  ci: boolean
  promoteAfterMs: number
}

export interface AxoAskSettings {
  timeoutMs: number | null
}

export interface AxoProjectFiles {
  skills: boolean
  config: boolean
}

export interface AxoProjectSettings {
  files: AxoProjectFiles
  access: AxoProjectAccess
  paths: Record<string, AxoProjectAccess>
}

export interface AxoCompactionSettings {
  enabled: boolean
  maxContext: number | null
  reserveTokens: number | null
  keepRecentTokens: number | null
  summaryRequest: AxoSummaryRequest
}

export interface AxoRetrySettings {
  maxRetries: number
  baseDelayMs: number
  maxDelayMs: number
}

export type AxoTierOnModelChange = 'reset' | 'keep'
export type AxoSubagentTier = 'standard' | 'inherit'

export interface AxoServiceTierSettings {
  onModelChange: AxoTierOnModelChange
}

export interface AxoSubagentSettings {
  maxDepth: number
  maxConcurrent: number
  maxTurns: number
  defaultModel: string | null
  models: Record<string, string>
  serviceTier: AxoSubagentTier
  contextFiles: AxoContextFiles
}

export interface AxoKeywordSettings {
  enabled: boolean
  disabled: string[]
}

export interface AxoWebSearchSettings {
  enabled: boolean
  providers: string[]
  searxngUrl: string | null
  groundedModel: string | null
}

export interface AxoDocsSettings {
  enabled: boolean
  ttlHours: number
  retentionDays: number
  sources: string[]
}

export interface AxoBrowserSettings {
  enabled: boolean
  headless: boolean
  cdpUrl: string | null
  executablePath: string | null
  idleCloseSecs: number
}

export interface AxoSettings {
  extensions: AxoEnabledExtension[]
  projectExtensions: Record<string, AxoEnabledExtension[]>
  defaultModel: string | null
  defaultAgent: string | null
  /** Where accepted plans are saved, relative to the folder or absolute. */
  plansDir: string
  modelRoles: Record<string, string>
  defaultThinkingLevel: ChatThinking
  serviceTier: AxoServiceTierSettings
  homeAgentsDir: boolean
  steeringMode: AxoQueueMode
  followUpMode: AxoQueueMode
  contextFiles: Record<string, boolean>
  nestedContextFiles: boolean
  extendedContext: boolean
  bash: AxoBashSettings
  ask: AxoAskSettings
  project: AxoProjectSettings
  compaction: AxoCompactionSettings
  retry: AxoRetrySettings
  subagents: AxoSubagentSettings
  keywords: AxoKeywordSettings
  webSearch: AxoWebSearchSettings
  docs: AxoDocsSettings
  browser: AxoBrowserSettings
}

export interface MemoryBank {
  path: string | null
}

export interface MemoryCapture {
  compaction: boolean
  close: boolean
  notify: boolean
}

export interface MemorySettings {
  enabled: boolean
  semantic: boolean
  bank: string
  exclude: boolean
  banks: Record<string, MemoryBank>
  capture: MemoryCapture
}

export interface MemoryStatus {
  enabled: boolean
  embed: {
    state: 'disabled' | 'missing' | 'downloading' | 'ready' | 'failed'
    received: number
    total: number
    error: string | null
  }
  banks: { name: string; path: string; exists: boolean; notes: number; default: boolean }[]
}

export type MemoryCaptureTrigger = 'compaction' | 'close'
export type MemoryFactOutcome = 'new' | 'updated' | 'known'

export interface MemoryCaptureQuery {
  sessionId: string | null
  bank: string | null
  limit: number
}

export interface MemoryCapturedFact {
  id: number
  outcome: MemoryFactOutcome
  category: string
  text: string
  /** The observation an `updated` fact replaced. */
  previous: string | null
  /** Bank-relative note path without `.md`. */
  note: string
  /** Absolute note file on the host. */
  path: string
  reverted: boolean
}

export interface MemoryCaptureRun {
  id: number
  sessionId: string
  /** Host-local folder path. */
  folderPath: string
  title: string | null
  bank: string
  trigger: MemoryCaptureTrigger
  anchorEntryId: string | null
  startedAt: string
  /** `null` while the capture runs. */
  endedAt: string | null
  error: string | null
  noModel: boolean
  facts: MemoryCapturedFact[]
}

export interface EffectiveSettings {
  axo: AxoSettings
  memory: MemorySettings
  window: WindowSettings
  typography: TypographySettings
  nix: NixSettings
  explorer: ExplorerSettings
  git: GitSettings
  formatting: FormattingSettings
  sessions: SessionsSettings
  shell: ShellSettings
  harnesses: Record<string, HarnessSettings>
  lsp: { servers: Record<string, EffectiveLspServerSettings> }
  remotes: Record<string, RemoteSettings>
}

export type SettingsLayerKind = 'global' | 'folder'
export type SettingsDiagnosticCode = 'parse_error' | 'invalid_value' | 'unknown_key'
export type SettingsDiagnosticSeverity = 'warning' | 'error'
/** Machine that resolved the layer: this desktop, or the daemon hosting a remote folder. */
export type SettingsOrigin = 'desktop' | 'host'

export interface SettingsDiagnostic {
  layer: SettingsLayerKind
  origin: SettingsOrigin
  path: string
  pointer: string
  code: SettingsDiagnosticCode
  severity: SettingsDiagnosticSeverity
  message: string
}

export interface EffectiveSettingsPayload {
  settings: EffectiveSettings
  diagnostics: SettingsDiagnostic[]
}

export interface SettingsFileResult {
  path: string
}

export interface ShortcutsFilePayload {
  path: string
  loaded: boolean
  value: unknown
}

export interface ShortcutsFileResult {
  path: string
}

export interface SettingsChangedEvent {
  layer: SettingsLayerKind
  folder_path?: string | null
  generation: number
  diagnostics: SettingsDiagnostic[]
}

export interface HarnessSettingsEntry {
  harness: HarnessStatus
  config: HarnessConfig
}

export interface SettingsPayload {
  axo: AxoSettings
  memory: MemorySettings
  window: WindowSettings
  typography: TypographySettings
  nix: NixSettings
  formatting: FormattingSettings
  lsp: EffectiveSettings['lsp']
  harnesses: HarnessSettingsEntry[]
  shell: ShellSettings
}

export type LspTraceLevel = 'off' | 'messages' | 'verbose'
export type LspServerConnectionStatus = 'connected' | 'missing' | 'error' | 'disabled'
export type LspTransportTraceDirection = 'incoming' | 'outgoing' | 'stderr'
export type SettingsPageKind = 'language' | 'nix'

export interface LanguageContribution {
  id: string
  label: string
  aliases: string[]
  extensions: string[]
  filenames: string[]
}

export interface LspDocumentSelector {
  language: string | null
  extensions: string[]
  filenames: string[]
}

// Optional per-server settings descriptor sourced from the integration
// manifest. `schema` is an inlined JSON Schema object usable by Monaco
// for autocomplete/validation; `section` hints which LSP workspace
// configuration section the JSON is expected to slot into; `defaults`
// seeds the JSON editor when the user has no settings yet.
export interface LspServerSettingsDescriptor {
  section: string | null
  defaults: unknown
  schema: unknown
}

export interface LanguageFormatterPolicy {
  language_id: string
  language_label: string
  options: FormatterOption[]
  default: string
}

export interface FormatterOption {
  id: string
  label: string
}

export interface SettingsPage {
  id: string
  kind: SettingsPageKind
  label: string
  icon_filename: string
  language_ids: string[]
  server_definition_ids: string[]
}

export type ShiftEncoding = 'legacy' | 'kitty'

export interface HarnessDescriptor {
  id: string
  label: string
  integration_id: string
  /** SVG markup. */
  icon: string
  wordmark: string | null
  wordmark_aspect: number | null
  wordmark_font: string | null
  gradient: [string, string]
  shift_encoding: ShiftEncoding
  title_prefixes: string[]
}

export interface IntegrationRuntimeCatalog {
  languages: LanguageContribution[]
  formatter_policies: LanguageFormatterPolicy[]
  harnesses: HarnessDescriptor[]
}

export interface IntegrationSettingsCatalog {
  pages: SettingsPage[]
}

export interface IntegrationDiagnostic {
  integration_id: string | null
  path: string
  message: string
}

export interface IntegrationCatalog {
  runtime: IntegrationRuntimeCatalog
  settings: IntegrationSettingsCatalog
  diagnostics: IntegrationDiagnostic[]
}

export interface IntegrationGrammar {
  integration_id: string
  language: string
  embedded_languages: string[]
  grammar: Record<string, unknown>
}

export type FormatterAvailability = 'available' | 'missing'

export interface FormatterStatus {
  formatter_id: string
  label: string
  integration_id: string
  integration_label: string
  languages: string[]
  status: FormatterAvailability
  resolved_path: string | null
  message: string | null
  install_hint: string
}

export interface LspServerConfig {
  serverDefinitionId: string
  enabled: boolean
  binaryPathOverride: string | null
  extraArgs: string[]
  trace: LspTraceLevel
  settings: unknown | null
}

export interface LspServerStatus {
  server_definition_id: string
  integration_id: string
  integration_label: string
  label: string
  enabled: boolean
  status: LspServerConnectionStatus
  resolved_path: string | null
  message: string | null
  install_hint: string
  document_selectors: LspDocumentSelector[]
  initialization_options: unknown
  settings: LspServerSettingsDescriptor | null
}

export interface LspServerSettingsEntry {
  server: LspServerStatus
  config: LspServerConfig
}

export type LspEvent =
  | {
      type: 'started'
      session_id: string
      pid: number | null
      resolved_path: string | null
    }
  | {
      type: 'message'
      session_id: string
      payload_json: string
    }
  | {
      type: 'trace'
      session_id: string
      direction: LspTransportTraceDirection
      payload: string
    }
  | {
      type: 'exit'
      session_id: string
      code: number | null
    }
  | {
      type: 'error'
      session_id: string
      message: string
    }

export interface GitChange {
  path: string
  status: string
  staged: boolean
  additions: number | null
  deletions: number | null
}

export interface GitSummary {
  is_repo: boolean
  branch: string | null
  base_ref: string | null
  ahead: number | null
  behind: number | null
  changes: GitChange[]
  staged_count: number
  unstaged_count: number
  untracked_count: number
  upstream: string | null
  detached: boolean
  unborn: boolean
  /** HEAD commit hash; null when unborn. */
  head: string | null
  operation: GitOperation | null
  conflicts: GitConflict[]
  message_template: string | null
}

export type GitOperationKind = 'merge' | 'rebase' | 'cherry_pick' | 'revert'

export interface GitOperation {
  kind: GitOperationKind
  target: string | null
  headName: string | null
  step: number | null
  total: number | null
  commit: BranchCommitRef | null
}

export type GitConflictKind =
  | 'both_modified'
  | 'both_added'
  | 'both_deleted'
  | 'added_by_us'
  | 'added_by_them'
  | 'deleted_by_us'
  | 'deleted_by_them'

export interface GitConflict {
  path: string
  kind: GitConflictKind
}

export type GitPathsOp =
  | { kind: 'stage'; files: string[] | null; allowConflictMarkers: boolean }
  | { kind: 'unstage'; files: string[] | null }
  | { kind: 'discard'; files: string[] | null; permanent: boolean }
  | { kind: 'ignore'; files: string[] }

export interface GitCommitOptions {
  amend: boolean
  all: boolean
  signOff: boolean
  noVerify: boolean
}

export interface GitRemote {
  name: string
  fetchUrl: string | null
  pushUrl: string | null
  pushDefault: boolean
}

export interface GitTag {
  name: string
  hash: string
  shortHash: string
  annotated: boolean
  subject: string
  date: string
}

export type GitTagOp =
  | { kind: 'create'; name: string; rev: string; message: string | null }
  | { kind: 'delete'; name: string }
  | { kind: 'push'; remote: string; name: string | null }
  | { kind: 'delete_remote'; remote: string; name: string }

export type GitRemoteConfigOp =
  | { kind: 'add'; name: string; url: string }
  | { kind: 'remove'; name: string }
  | { kind: 'rename'; oldName: string; newName: string }
  | { kind: 'set_url'; name: string; url: string }

export type GitResetMode = 'soft' | 'mixed' | 'hard'

export type GitHistoryOp =
  | { kind: 'cherry_pick'; rev: string }
  | { kind: 'revert'; rev: string }
  | { kind: 'reset'; rev: string; mode: GitResetMode }

export type GitRemoteOp =
  | { kind: 'fetch'; remote: string | null }
  | { kind: 'pull'; rebase: boolean }
  | { kind: 'push'; branch: string | null; force: boolean }
  | { kind: 'publish'; branch: string | null; remote: string | null }
  | { kind: 'sync'; rebase: boolean }

export type GitStashScope =
  { kind: 'all'; includeUntracked: boolean } | { kind: 'staged' } | { kind: 'paths'; paths: string[] }

export type GitStashOp =
  | { kind: 'push'; message: string | null; scope: GitStashScope }
  | { kind: 'apply'; index: number }
  | { kind: 'pop'; index: number }
  | { kind: 'drop'; index: number }
  | { kind: 'clear' }
  | { kind: 'branch'; index: number; name: string }

export interface GitWorktree {
  path: string
  head: string | null
  branch: string | null
  isMain: boolean
  isCurrent: boolean
  bare: boolean
  locked: string | null
  prunable: string | null
}

export interface GitWorktreeList {
  commonDir: string
  mainPath: string | null
  worktrees: GitWorktree[]
}

export type GitWorktreeCheckout =
  | { kind: 'existing_branch'; name: string }
  | { kind: 'new_branch'; name: string; base: string }
  | { kind: 'detached'; rev: string }

export type GitWorktreeOp =
  | { kind: 'add'; destination: string; checkout: GitWorktreeCheckout }
  | { kind: 'remove'; worktreePath: string; force: boolean }
  | { kind: 'prune' }
  | { kind: 'migrate'; worktreePath: string }

export type GitWorktreeResult =
  | { kind: 'added'; path: string; copyComplete: boolean; warnings: string[] }
  | { kind: 'removed' }
  | { kind: 'pruned' }
  | { kind: 'migrated'; mainPath: string; recoveryStash: string | null }

export type GitBranchOp =
  | { kind: 'checkout'; name: string }
  | { kind: 'checkout_detached'; rev: string }
  | { kind: 'checkout_remote'; remoteName: string; localName: string }
  | { kind: 'create'; name: string; base: string; checkout: boolean }
  | { kind: 'rename'; oldName: string; newName: string }
  | { kind: 'delete'; name: string; force: boolean }
  | { kind: 'delete_remote'; remote: string; name: string }
  | { kind: 'set_upstream'; branch: string; upstream: string }
  | { kind: 'unset_upstream'; branch: string }
  | { kind: 'fast_forward'; name: string }
  | { kind: 'merge'; source: string; noFf: boolean }
  | { kind: 'rebase'; target: string }

export type GitSequencerAction = { kind: 'continue'; message: string | null } | { kind: 'skip' } | { kind: 'abort' }

export type GitConflictSide = 'ours' | 'theirs'

export type GitConflictsOp =
  { kind: 'take_side'; files: string[]; side: GitConflictSide } | { kind: 'delete'; files: string[] }

/** Cheap git state for Home project cards; `changed` counts distinct paths. */
export interface GitBrief {
  is_repo: boolean
  branch: string | null
  changed: number
  ahead: number | null
  behind: number | null
}

export type GitRefKind = 'branch' | 'remote_branch' | 'tag'

export interface GitRef {
  kind: GitRefKind
  name: string
  current: boolean
}

export interface GraphCommit {
  hash: string
  short_hash: string
  parents: string[]
  author: string
  date: string
  message: string
  refs: GitRef[]
}

export interface CommitDetail {
  hash: string
  short_hash: string
  parents: string[]
  author: string
  date: string
  message: string
  body: string
  files: FileDiff[]
}

export interface GitQuickDiffData {
  indexContent: string | null
  headContent: string | null
  hasIndexChanges: boolean
}

// DIFF METADATA //
//
// Mirrors `src-crates/axome-protocol/src/file_diff.rs`: metadata for
// one changed file; contents via `DiffFileContent`.

export type GitStatusKind =
  'added' | 'modified' | 'deleted' | 'renamed' | 'copied' | 'untracked' | 'unmerged' | 'unknown'

export type DiffSource =
  | { kind: 'working'; staged: boolean }
  | { kind: 'commit'; hash: string }
  | { kind: 'stash'; index: number }
  | { kind: 'range'; base: string; target: string | null }

export interface FileDiff {
  path: string
  oldPath: string | null
  status: GitStatusKind
  lang: string
  additions: number | null
  deletions: number | null
}

// Per-file content payload for working-tree, commit and stash diffs.
// Mirrors `DiffFileContent` in `src-crates/axome-protocol/src/git.rs`.
export interface DiffFileContent {
  oldContent: string | null
  newContent: string | null
  binary: boolean
}

export type ClaimFileResult = { status: 'claimed' } | { status: 'redirect'; owner_label: string; tab_id: string }

export interface FilePasteMapping {
  source: string
  destination: string
}

export interface FilePasteCollision {
  source: string
  destination: string
}

// JSON Schema for a project-scoped config file (e.g. .axome/tasks.jsonc).
// Schemas are derived from Rust types via schemars so they can't drift
// from the deserialization contract.
export interface ConfigSchemaEntry {
  id: string
  fileMatch: string[]
  schema: unknown
}

// Mirror of Rust `TaskDefinition` (src-crates/axome-protocol/src/task.rs).
// camelCase here matches the `#[serde(rename_all = "camelCase")]` on
// the source type.
export interface TaskDefinition {
  id: string
  label: string
  command: string
  shell?: string
  cwd?: string
  env?: Record<string, string>
  icon?: string
  group?: string
  singleton?: boolean
  clearOnRerun?: boolean
  confirm?: boolean
  runOn?: 'worktreeCreated'
}

export type IssueStatus = 'todo' | 'in_progress' | 'blocked' | 'completed' | 'wont_fix' | 'archived'
export type IssueEpicStatus = 'todo' | 'in_progress' | 'completed' | 'archived'
export type IssueAssigneeKind = 'human' | 'agent' | 'session' | 'unassigned'

export interface IssueEpic {
  id: string
  title: string
  description: string | null
  status: IssueEpicStatus
  priority: number
  createdBy: string
  updatedBy: string
  createdAt: string
  updatedAt: string
}

export interface Issue {
  id: string
  epicId: string | null
  parentIssueId: string | null
  title: string
  description: string | null
  status: IssueStatus
  priority: number
  assigneeKind: IssueAssigneeKind
  assigneeId: string | null
  createdBy: string
  updatedBy: string
  tags: string[]
  contextJson: string | null
  createdAt: string
  updatedAt: string
}

export interface IssueComment {
  id: string
  issueId: string
  author: string
  body: string
  createdBy: string
  updatedBy: string
  createdAt: string
  updatedAt: string
}

export interface IssueDependency {
  id: string
  issueId: string
  dependsOnIssueId: string
  createdBy: string
  createdAt: string
}

export interface IssueEvent {
  id: number
  actor: string
  action: string
  entityType: string
  entityId: string
  snapshotJson: string | null
  changesJson: string | null
  createdAt: string
}

export interface IssueDetail {
  issue: Issue
  comments: IssueComment[]
  dependsOn: IssueDependency[]
  blockedBy: IssueDependency[]
  subIssues: Issue[]
  events: IssueEvent[]
}

export interface IssueListFilters {
  status?: IssueStatus
  epicId?: string
  includeArchived?: boolean
  limit?: number
}

export interface IssueCreateInput {
  title: string
  description?: string | null
  status?: IssueStatus
  priority?: number
  epicId?: string | null
  parentIssueId?: string | null
  assigneeKind?: IssueAssigneeKind
  assigneeId?: string | null
  tags?: string[]
  contextJson?: string | null
  actor?: string
}

export interface IssueUpdateInput {
  title?: string
  description?: string | null
  status?: IssueStatus
  priority?: number
  epicId?: string | null
  assigneeKind?: IssueAssigneeKind
  assigneeId?: string | null
  tags?: string[]
  contextJson?: string | null
  actor?: string
}

export interface IssueEpicCreateInput {
  title: string
  description?: string | null
  status?: IssueEpicStatus
  priority?: number
  actor?: string
}

export interface IssueEpicUpdateInput {
  title?: string
  description?: string | null
  status?: IssueEpicStatus
  priority?: number
  actor?: string
}

export interface IssueCommentCreateInput {
  issueId: string
  author: string
  body: string
  actor?: string
}

export interface StashEntry {
  index: number
  message: string
  date: string
  files: FileDiff[]
}

// BRANCHES //
//
// Mirror of `src-crates/axome-protocol/src/branch.rs`. The Rust types use
// `#[serde(rename_all = "camelCase")]` so JSON keys match the TS
// shape directly.

export type BranchKind = 'local' | 'remote'

export interface BranchCommitRef {
  hash: string
  shortHash: string
  subject: string
  author: string
  /** ISO-8601 author date. */
  date: string
}

export interface BranchSummary {
  name: string
  kind: BranchKind
  /** True for the currently checked-out HEAD (one local row at most). */
  isCurrent: boolean
  upstream: string | null
  worktreePath: string | null
  ahead: number
  behind: number
  tip: BranchCommitRef
}

// ACTIVITY MAP //

export interface DiscoveredHarnessActivity {
  harness_id: string
  last_active: string
  daily_counts: [number, number, number, number, number, number, number]
}

export interface DiscoveredProject {
  path: string
  name: string
  path_exists: boolean
  last_active: string
  harnesses: DiscoveredHarnessActivity[]
}

// NIX ENVIRONMENT //

export type NixEnvStatus = 'pending' | 'evaluating' | 'ready' | 'error' | 'timeout'

export interface NixDiagnostic {
  message: string
  line: number
  column: number
}

export interface NixEnvRecord {
  folder_path: string
  nix_file: string
  status: NixEnvStatus
  error_message: string | null
  evaluated_at: string | null
  created_at: string
  updated_at: string
}

export interface NixDetection {
  folder_path: string
  detected_files: string[]
  selected: NixEnvRecord | null
}

export interface AxoEnabledExtension {
  id: string
  group: string | null
}

export interface AxoExtensions {
  bun: AxoBun
  host: AxoExtension[]
  project: AxoProjectExtensions | null
  collisions: AxoProviderCollision[]
  diagnostics: AxoExtensionDiagnostic[]
}

export interface AxoBun {
  path: string | null
  searched: string[]
}

export interface AxoProjectExtensions {
  folder: string
  access: AxoProjectAccess
  rootExists: boolean
  items: AxoExtension[]
  collisions: AxoProviderCollision[]
  diagnostics: AxoExtensionDiagnostic[]
}

export type AxoExtensionOrigin = 'installed' | 'userFolder' | 'projectFolder' | 'missing'

export type AxoExtensionPhase = 'cold' | 'installing' | 'starting' | 'ready' | 'restarting' | 'disabled' | 'unavailable'

export interface AxoExtensionProblem {
  kind: AxoExtensionProblemKind
  message: string
}

export type AxoExtensionProblemKind =
  'bun' | 'notFound' | 'invalid' | 'dependenciesMissing' | 'dependencyInstall' | 'load' | 'crash' | 'setup'

export interface AxoExtension {
  id: string
  origin: AxoExtensionOrigin
  enabled: boolean
  position: number | null
  group: string | null
  phase: AxoExtensionPhase | null
  problem: AxoExtensionProblem | null
  adds: AxoExtensionAdds
  install: AxoExtensionInstall | null
  dataDir: string
  failure: AxoExtensionFailure | null
}

export interface AxoExtensionAdds {
  providers: AxoExtensionProvider[]
  tools: string[]
  commands: string[]
  toolPolicy: string[]
  spawnHook: boolean
}

export interface AxoExtensionProvider {
  id: string
  models: number
  login: boolean
}

export interface AxoExtensionInstall {
  origin: string
  revision: string | null
  gitRef: string | null
  url: string | null
  installedAt: number | null
  previous: AxoExtensionCopy | null
}

export interface AxoExtensionCopy {
  revision: string | null
  installedAt: number | null
}

export interface AxoExtensionFailure {
  title: string
  output: string | null
}

export interface AxoProviderCollision {
  provider: string
  winner: string
  hidden: AxoHiddenProvider
}

export type AxoHiddenProvider =
  { type: 'extension'; id: string } | { type: 'loaded' } | { type: 'registered' } | { type: 'layer' }

export interface AxoExtensionDiagnostic {
  id: string
  message: string
}

export interface AxoRunnerMemory {
  members: string[]
  group: string | null
  pssBytes: number
  project: boolean
}

export type AxoExtensionSource = { type: 'folder'; path: string } | { type: 'git'; url: string; gitRef: string | null }

export interface AxoExtensionStage {
  stageId: string
  id: string
  adds: AxoExtensionAdds
  origin: string
  revision: string | null
  subject: string | null
}

export type AxoInstallOutcome =
  | { type: 'installed'; id: string }
  | { type: 'originChanged'; installed: string; offered: string }
  | { type: 'failed'; title: string; output: string | null }
