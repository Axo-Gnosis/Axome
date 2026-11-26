// Typed host API. Desktop and future web transports provide the same calls.

import {
  getHostTransport,
  type ChatSinks,
  type ChatStreamRequest,
  type PtySinks,
  type StreamHandle,
  type Unsubscribe
} from './transport'
import { splitRemotePath } from '$lib/utils/paths'
import type {
  AxoSettings,
  AxoExtensions,
  AxoEnabledExtension,
  AxoExtensionProblem,
  AxoExtensionSource,
  AxoExtensionStage,
  AxoInstallOutcome,
  AxoRunnerMemory,
  AxoProvidersPayload,
  AxoWebToolsStatusPayload,
  AxoLoginStarted,
  AxoLoginStatus,
  AxoKeywordInfo,
  AppRuntimeInfo,
  BranchSummary,
  ChatAnswer,
  ChatContextDetails,
  ChatForkInfo,
  ChatJobOutput,
  ChatLiveSummary,
  ChatLinkTarget,
  ChatKeywordMatch,
  ChatLocalFiles,
  ChatModelCatalog,
  ChatModel,
  ChatPart,
  ChatPlanThen,
  ChatSquashedChange,
  ChatSummariesChangedEvent,
  ChatServiceTier,
  ChatThinking,
  IntegrationCatalog,
  IntegrationGrammar,
  FormatterStatus,
  CommitDetail,
  ConfigSchemaEntry,
  DiffFileContent,
  DiffSource,
  DiscoveredProject,
  EffectiveSettingsPayload,
  ExplorerDirEntry,
  ExplorerPathList,
  FileContent,
  FileStat,
  FileReadProgress,
  FileDiff,
  FilePasteMapping,
  FilePasteCollision,
  FilePathChangedPayload,
  FilesChangedEvent,
  FormattingSettings,
  NixSettings,
  MemoryCaptureQuery,
  MemoryCaptureRun,
  MemorySettings,
  MemoryStatus,
  PredictSettings,
  PredictStatus,
  PredictSuggestion,
  TypographySettings,
  WindowSettings,
  GitBrief,
  GitBranchOp,
  GitCommitOptions,
  GitConflictsOp,
  GitHistoryOp,
  GitOperationKind,
  GitPathsOp,
  GitChangedEvent,
  GitQuickDiffData,
  GitRemote,
  GitRemoteOp,
  GitRemoteConfigOp,
  GitSequencerAction,
  GitStashOp,
  GitSummary,
  GitTag,
  GitTagOp,
  GitWorktreeCheckout,
  GitWorktreeList,
  GitWorktreeOp,
  GitWorktreeResult,
  GraphCommit,
  Issue,
  IssueComment,
  IssueCommentCreateInput,
  IssueCreateInput,
  IssueDetail,
  IssueEpic,
  IssueEpicCreateInput,
  IssueEpicUpdateInput,
  IssueListFilters,
  IssueUpdateInput,
  LspEvent,
  LspServerConfig,
  LspServerSettingsEntry,
  NixDetection,
  NixDiagnostic,
  NixEnvRecord,
  FolderEntry,
  FolderInfo,
  PathRoot,
  HarnessConfig,
  HarnessListing,
  ShellConfig,
  ShellListing,
  SessionSpec,
  SessionStartInfo,
  SessionIndexSnapshot,
  SettingsChangedEvent,
  SettingsFileResult,
  SettingsPayload,
  ShortcutsFilePayload,
  StashEntry,
  TaskDefinition,
  RecentFolder,
  AttachMode,
  WorkbenchAttached,
  WorkbenchInfo
} from '$lib/types/backend'

// Keep the command names and argument shapes at the host facade boundary.
function invoke<T>(method: string, params: object = {}): Promise<T> {
  return getHostTransport().call<T>(method, params)
}

export const backend = {
  activityMap: {
    get(): Promise<DiscoveredProject[]> {
      return invoke<DiscoveredProject[]>('activity_map_get')
    },
    refresh(): Promise<DiscoveredProject[]> {
      return invoke<DiscoveredProject[]>('activity_map_refresh')
    }
  },

  app: {
    runtimeInfo(): Promise<AppRuntimeInfo> {
      return invoke<AppRuntimeInfo>('app_runtime_info')
    },
    stateGet(key: string): Promise<string | null> {
      return invoke<string | null>('app_state_get', { key })
    },
    statePut(key: string, valueJson: string): Promise<void> {
      return invoke<void>('app_state_put', { key, valueJson })
    },
    stateDelete(key: string): Promise<void> {
      return invoke<void>('app_state_delete', { key })
    }
  },

  /** Server-owned workbench registry; attaching to an unknown id creates it. */
  workbenches: {
    list(server?: string): Promise<WorkbenchInfo[]> {
      return invoke<WorkbenchInfo[]>('workbench_list', { server })
    },
    /** Stops the workbench's sessions/tasks and deletes its saved state. */
    close(id: string, server?: string): Promise<void> {
      return invoke<void>('workbench_close', { id, server })
    },
    attach(server: string, id: string, mode: AttachMode, attachmentId: string): Promise<WorkbenchAttached | null> {
      return invoke<WorkbenchAttached | null>('workbench_attach', { server, id, mode, attachmentId })
    },
    detach(server: string, id: string, attachmentId: string): Promise<void> {
      return invoke<void>('workbench_detach', { server, id, attachmentId })
    },
    save(server: string, id: string, snapshot: string): Promise<void> {
      return invoke<void>('workbench_save', { server, id, snapshot })
    },
    onChanged(handler: (event: { server: string | null }) => void): Promise<Unsubscribe> {
      return getHostTransport().subscribe('workbenches-changed', handler)
    }
  },

  folders: {
    recentList(): Promise<RecentFolder[]> {
      return invoke<RecentFolder[]>('recent_folders_list')
    },
    recentTouch(path: string): Promise<RecentFolder[]> {
      return invoke<RecentFolder[]>('recent_folders_touch', { path })
    },
    recentRemove(paths: string[]): Promise<RecentFolder[]> {
      return invoke<RecentFolder[]>('recent_folders_remove', { paths })
    },
    onRecentFoldersChanged(handler: (folders: RecentFolder[]) => void): Promise<Unsubscribe> {
      return getHostTransport().subscribe('recent-folders-changed', handler)
    },
    claim(folderPath: string): Promise<void> {
      return invoke<void>('folder_claim', { folderPath })
    },
    /** Canonicalize a folder path; rejects missing paths and non-directories. */
    resolve(path: string): Promise<FolderInfo> {
      return invoke<FolderInfo>('folder_resolve', { path })
    },
    listEntries(path: string, showHidden: boolean): Promise<FolderEntry[]> {
      return invoke<FolderEntry[]>('folder_list_entries', { path, showHidden })
    },
    /** Where the browser's path bar starts for a folder; remote roots come back as `axome://` paths. */
    pathRoot(path: string): Promise<PathRoot> {
      return invoke<PathRoot>('folder_path_root', { path })
    },
    /** Canonical home folder of `server`, or of this host when omitted. */
    async home(server?: string): Promise<string> {
      const home = await invoke<string>('folder_home', { folderPath: server ? `axome://${server}/` : null })
      return server ? `axome://${server}${home}` : home
    },
    /** Canonical process working folder of `server`, or of this host when omitted. */
    async workingDirectory(server?: string): Promise<string> {
      const path = await invoke<string>('folder_working_directory', {
        folderPath: server ? `axome://${server}/` : null
      })
      return server ? `axome://${server}${path}` : path
    },
    /** Drop backend resources scoped to a folder that no longer has any open tab. */
    release(folderPath: string): Promise<void> {
      return invoke<void>('folder_release', { folderPath })
    }
  },

  harnesses: {
    list(): Promise<HarnessListing> {
      return invoke<HarnessListing>('harness_list')
    },
    listForFolder(folderPath: string): Promise<HarnessListing> {
      return invoke<HarnessListing>('harness_list_for_folder', { folderPath })
    }
  },

  sessions: {
    /** Reattach a matching retained run, or start a new run using harness resume policy. */
    start(spec: SessionSpec, cols: number, rows: number, sinks: PtySinks): StreamHandle<SessionStartInfo> {
      return getHostTransport().openStream({ method: 'session_start', params: { ...spec, cols, rows } }, sinks)
    },
    stop(runId: string): Promise<void> {
      return invoke<void>('session_stop', { runId })
    },
    ompResolveUri(uri: string, cwd?: string | null): Promise<{ path: string; is_dir: boolean }> {
      return invoke<{ path: string; is_dir: boolean }>('omp_resolve_uri', { uri, cwd: cwd ?? null })
    }
  },

  chat: {
    models(folderPath: string): Promise<ChatModelCatalog> {
      return invoke<ChatModelCatalog>('chat_models', { folderPath })
    },
    create(folderPath: string, model: string, thinking: ChatThinking, serviceTier: ChatServiceTier): Promise<string> {
      return invoke<string>('chat_create', { folderPath, model, thinking, serviceTier })
    },
    prompt(folderPath: string, sessionId: string, requestId: string, parts: ChatPart[]): Promise<void> {
      return invoke<void>('chat_prompt', { folderPath, sessionId, requestId, parts })
    },
    steer(folderPath: string, sessionId: string, parts: ChatPart[]): Promise<void> {
      return invoke<void>('chat_steer', { folderPath, sessionId, parts })
    },
    followUp(folderPath: string, sessionId: string, parts: ChatPart[]): Promise<void> {
      return invoke<void>('chat_follow_up', { folderPath, sessionId, parts })
    },
    withdraw(folderPath: string, sessionId: string, submissionId: string): Promise<ChatPart[]> {
      return invoke<ChatPart[]>('chat_withdraw', { folderPath, sessionId, submissionId })
    },
    sendNow(folderPath: string, sessionId: string): Promise<void> {
      return invoke<void>('chat_send_now', { folderPath, sessionId })
    },
    abort(folderPath: string, sessionId: string): Promise<void> {
      return invoke<void>('chat_abort', { folderPath, sessionId })
    },
    close(folderPath: string, sessionId: string): Promise<void> {
      return invoke<void>('chat_close', { folderPath, sessionId })
    },
    answer(folderPath: string, sessionId: string, callId: string, answers: ChatAnswer[]): Promise<void> {
      return invoke<void>('chat_answer', { folderPath, sessionId, callId, answers })
    },
    dismiss(folderPath: string, sessionId: string, callId: string): Promise<void> {
      return invoke<void>('chat_dismiss', { folderPath, sessionId, callId })
    },
    /** Runs a catalog command with its accepted JSON arguments. */
    runCommand(folderPath: string, sessionId: string, name: string, args: Record<string, unknown>): Promise<void> {
      return invoke<void>('chat_run_command', { folderPath, sessionId, name, args })
    },
    resolveLink(folderPath: string, sessionId: string, url: string): Promise<ChatLinkTarget> {
      return invoke<ChatLinkTarget>('chat_resolve_link', { folderPath, sessionId, url })
    },
    navigate(folderPath: string, sessionId: string, entryId: string | null): Promise<void> {
      return invoke<void>('chat_navigate', { folderPath, sessionId, entryId })
    },
    selectBranch(folderPath: string, sessionId: string, entryId: string): Promise<void> {
      return invoke<void>('chat_select_branch', { folderPath, sessionId, entryId })
    },
    fork(folderPath: string, sessionId: string, entryId: string): Promise<ChatForkInfo> {
      return invoke<ChatForkInfo>('chat_fork', { folderPath, sessionId, entryId })
    },
    background(folderPath: string, sessionId: string, callId: string): Promise<boolean> {
      return invoke<boolean>('chat_background', { folderPath, sessionId, callId })
    },
    killJob(folderPath: string, sessionId: string, jobId: string): Promise<boolean> {
      return invoke<boolean>('chat_kill_job', { folderPath, sessionId, jobId })
    },
    resumeWakes(folderPath: string, sessionId: string): Promise<void> {
      return invoke<void>('chat_resume_wakes', { folderPath, sessionId })
    },
    resume(folderPath: string, sessionId: string): Promise<void> {
      return invoke<void>('chat_resume', { folderPath, sessionId })
    },
    watchJob(
      folderPath: string,
      sessionId: string,
      attachId: string,
      jobId: string | null
    ): Promise<ChatJobOutput | null> {
      return invoke<ChatJobOutput | null>('chat_watch_job', { folderPath, sessionId, attachId, jobId })
    },
    contextDetails(folderPath: string, sessionId: string): Promise<ChatContextDetails> {
      return invoke<ChatContextDetails>('chat_context_details', { folderPath, sessionId })
    },
    localFiles(folderPath: string, sessionId: string): Promise<ChatLocalFiles> {
      return invoke<ChatLocalFiles>('chat_local_files', { folderPath, sessionId })
    },
    squashChanges(folderPath: string, sessionId: string, changes: unknown[]): Promise<ChatSquashedChange[]> {
      return invoke<ChatSquashedChange[]>('chat_squash_changes', { folderPath, sessionId, changes })
    },
    turnDiffFile(
      folderPath: string,
      sessionId: string,
      beforeSha: string | null,
      afterSha: string
    ): Promise<DiffFileContent> {
      return invoke<DiffFileContent>('chat_turn_diff_file', { folderPath, sessionId, beforeSha, afterSha })
    },
    /** Switches the persona; null selects Default. */
    setAgent(folderPath: string, sessionId: string, agent: string | null): Promise<void> {
      return invoke<void>('chat_set_agent', { folderPath, sessionId, agent })
    },
    /** Enters a mode (null exits), then submits `parts` when non-empty. */
    setMode(folderPath: string, sessionId: string, mode: string | null, parts: ChatPart[]): Promise<void> {
      return invoke<void>('chat_set_mode', { folderPath, sessionId, mode, parts })
    },
    /** Names the session; null or a blank name clears it, and the first prompt titles the chat again. */
    setTitle(folderPath: string, sessionId: string, title: string | null): Promise<void> {
      return invoke<void>('chat_set_title', { folderPath, sessionId, title })
    },
    /** Keyword occurrences per text, in UTF-16 offsets. */
    keywordMatches(folderPath: string, sessionId: string, texts: string[]): Promise<ChatKeywordMatch[][]> {
      return invoke<ChatKeywordMatch[][]>('chat_keyword_matches', { folderPath, sessionId, texts })
    },
    acceptResult(folderPath: string, sessionId: string, callId: string): Promise<void> {
      return invoke<void>('chat_accept_result', { folderPath, sessionId, callId })
    },
    reviseResult(folderPath: string, sessionId: string, callId: string, feedback: string): Promise<void> {
      return invoke<void>('chat_revise_result', { folderPath, sessionId, callId, feedback })
    },
    /** Saves a reviewed plan under `plansDir` and accepts it; resolves to the saved path. */
    acceptPlan(
      folderPath: string,
      sessionId: string,
      callId: string,
      saveAs: string,
      then: ChatPlanThen
    ): Promise<string> {
      return invoke<string>('chat_accept_plan', { folderPath, sessionId, callId, saveAs, then })
    },
    liveSummaries(folderPath: string): Promise<ChatLiveSummary[]> {
      return invoke<ChatLiveSummary[]>('chat_live_summaries', { folderPath })
    },
    onSummariesChanged(handler: (event: ChatSummariesChangedEvent) => void): Promise<Unsubscribe> {
      return getHostTransport().subscribe('chat-summaries-changed', handler)
    },
    attach(params: ChatStreamRequest['params'], sinks: ChatSinks): StreamHandle<void> {
      return getHostTransport().openStream({ method: 'chat_attach', params }, sinks)
    }
  },

  shells: {
    list(): Promise<ShellListing> {
      return invoke<ShellListing>('shell_list')
    },
    listForFolder(folderPath: string): Promise<ShellListing> {
      return invoke<ShellListing>('shell_list_for_folder', { folderPath })
    },
    start(
      spec: { runId: string; folderPath: string; preset: string | null },
      cols: number,
      rows: number,
      sinks: PtySinks
    ): StreamHandle<void> {
      return getHostTransport().openStream({ method: 'shell_start', params: { ...spec, cols, rows } }, sinks)
    },
    stop(runId: string): Promise<void> {
      return invoke<void>('shell_stop', { runId })
    }
  },

  runs: {
    write(runId: string, data: Uint8Array): Promise<void> {
      return invoke<void>('run_write', { runId, data: Array.from(data) })
    },
    resize(runId: string, cols: number, rows: number): Promise<void> {
      return invoke<void>('run_resize', { runId, cols, rows })
    }
  },

  git: {
    listWorktrees(path: string): Promise<GitWorktreeList> {
      return invoke<GitWorktreeList>('git_worktree_list', { path })
    },
    defaultWorktreePath(path: string, checkout: GitWorktreeCheckout, root: string | null = null): Promise<string> {
      return invoke<string>('git_worktree_default_path', { path, checkout, root })
    },
    worktree(path: string, op: GitWorktreeOp): Promise<GitWorktreeResult> {
      return invoke<GitWorktreeResult>('git_worktree', { path, op })
    },
    getSummary(path: string): Promise<GitSummary> {
      return invoke<GitSummary>('git_get_summary', { path })
    },
    /** Branch, changed-path count and ahead/behind from one `git status`; for Home cards. */
    getBrief(path: string): Promise<GitBrief> {
      return invoke<GitBrief>('git_get_brief', { path })
    },
    /** Arm or repair the folder's git watcher; idempotent and silent on non-repos. */
    watch(projectPath: string): Promise<void> {
      return invoke<void>('git_watch', { projectPath })
    },
    onChanged(handler: (event: GitChangedEvent) => void): Promise<Unsubscribe> {
      return getHostTransport().subscribe('git-changed', handler)
    },
    getGraph(path: string, limit = 100): Promise<GraphCommit[]> {
      return invoke<GraphCommit[]>('git_get_graph', { path, limit })
    },
    getCommitDetail(path: string, hash: string): Promise<CommitDetail | null> {
      return invoke<CommitDetail | null>('git_get_commit_detail', {
        path,
        hash
      })
    },
    /** Return file content at a git revision (ref and path validated server-side). */
    showFile(projectPath: string, gitRef: string, filePath: string): Promise<string> {
      return invoke<string>('git_show_file', { projectPath, gitRef, filePath })
    },
    /**
     * Load one file's contents for a working-tree, commit or stash diff.
     */
    getDiffFile(
      path: string,
      source: DiffSource,
      filePath: string,
      oldPath: string | null,
      status: FileDiff['status']
    ): Promise<DiffFileContent> {
      return invoke<DiffFileContent>('diff_get_file', { path, source, filePath, oldPath, status })
    },
    /**
     * Cheap working-tree diff index; file list + metadata only,
     * no content. Pair with [`getDiffFile`] to load each
     * file lazily so a 200-file working tree doesn't ship a
     * multi-megabyte payload before the user has expanded any row.
     */
    getWorkingDiffIndex(path: string, staged: boolean): Promise<FileDiff[]> {
      return invoke<FileDiff[]>('diff_get_working_index', { path, staged })
    },

    // Write operations
    paths(path: string, op: GitPathsOp): Promise<void> {
      return invoke<void>('git_paths', { path, op })
    },
    getFullPatch(path: string): Promise<string | null> {
      return invoke<string | null>('git_get_full_patch', { path })
    },
    getPathPatch(path: string, files: string[], staged: boolean | null = null): Promise<string | null> {
      return invoke<string | null>('git_get_path_patch', {
        path,
        files,
        staged
      })
    },
    getQuickDiffData(projectPath: string, filePath: string): Promise<GitQuickDiffData> {
      return invoke<GitQuickDiffData>('git_get_quick_diff_data', {
        projectPath,
        filePath
      })
    },
    stageFileContent(projectPath: string, filePath: string, content: string | null): Promise<void> {
      return invoke<void>('git_stage_file_content', {
        projectPath,
        filePath,
        content
      })
    },
    commit(path: string, message: string, options: Partial<GitCommitOptions> = {}): Promise<string> {
      return invoke<string>('git_commit', {
        path,
        message,
        options: { amend: false, all: false, signOff: false, noVerify: false, ...options }
      })
    },
    undoLastCommit(path: string): Promise<string> {
      return invoke<string>('git_undo_last_commit', { path })
    },
    remote(path: string, op: GitRemoteOp): Promise<void> {
      return invoke<void>('git_remote', { path, op })
    },
    stash(path: string, op: GitStashOp): Promise<void> {
      return invoke<void>('git_stash', { path, op })
    },
    stashCount(path: string): Promise<number> {
      return invoke<number>('git_stash_count', { path })
    },
    stashList(path: string): Promise<StashEntry[]> {
      return invoke<StashEntry[]>('git_stash_list', { path })
    },
    init(path: string): Promise<void> {
      return invoke<void>('git_init', { path })
    },
    cloneInPlace(path: string, url: string): Promise<void> {
      return invoke<void>('git_clone_in_place', { path, url })
    },

    listBranches(path: string): Promise<BranchSummary[]> {
      return invoke<BranchSummary[]>('git_list_branches', { path })
    },
    listRemotes(path: string): Promise<GitRemote[]> {
      return invoke<GitRemote[]>('git_list_remotes', { path })
    },
    getBranchCommits(path: string, branch: string, limit = 5): Promise<GraphCommit[]> {
      return invoke<GraphCommit[]>('git_get_branch_commits', { path, branch, limit })
    },
    listTags(path: string): Promise<GitTag[]> {
      return invoke<GitTag[]>('git_list_tags', { path })
    },
    tag(path: string, op: GitTagOp): Promise<void> {
      return invoke<void>('git_tag', { path, op })
    },
    remoteConfig(path: string, op: GitRemoteConfigOp): Promise<void> {
      return invoke<void>('git_remote_config', { path, op })
    },
    history(path: string, op: GitHistoryOp): Promise<void> {
      return invoke<void>('git_history', { path, op })
    },
    diffRange(path: string, base: string, target: string | null): Promise<FileDiff[]> {
      return invoke<FileDiff[]>('git_diff_range', { path, base, target })
    },
    mergedBranches(path: string): Promise<string[]> {
      return invoke<string[]>('git_merged_branches', { path })
    },
    branch(path: string, op: GitBranchOp): Promise<void> {
      return invoke<void>('git_branch', { path, op })
    },
    sequencer(path: string, operation: GitOperationKind, action: GitSequencerAction): Promise<void> {
      return invoke<void>('git_sequencer', { path, operation, action })
    },
    conflicts(path: string, op: GitConflictsOp): Promise<void> {
      return invoke<void>('git_conflicts', { path, op })
    }
  },

  files: {
    stat(folderPath: string, filePath: string): Promise<FileStat> {
      return invoke<FileStat>('file_stat', { projectPath: folderPath, filePath })
    },
    /** `version` and `size` are the approved stat; the open re-checks that identity once. */
    readStream(
      requestId: string,
      folderPath: string,
      filePath: string,
      version: string,
      size: number
    ): Promise<FileContent> {
      return invoke<FileContent>('file_read_stream', { requestId, projectPath: folderPath, filePath, version, size })
    },
    cancelReadStream(requestId: string): Promise<void> {
      return invoke<void>('file_read_stream_cancel', { requestId })
    },
    onReadProgress(handler: (event: FileReadProgress) => void): Promise<Unsubscribe> {
      return getHostTransport().subscribe('file-read-progress', handler)
    },
    /** One directory as the explorer renders it. `dirPath` '' is the project root. */
    readDir(projectPath: string, dirPath: string, showHidden: boolean): Promise<ExplorerDirEntry[]> {
      return invoke<ExplorerDirEntry[]>('files_read_dir', { projectPath, dirPath, showHidden })
    },
    /** Flat searchable path list for Quick Open and the sidebar filter. */
    listPaths(projectPath: string, showHidden: boolean): Promise<ExplorerPathList> {
      return invoke<ExplorerPathList>('files_list_paths', { projectPath, showHidden })
    },
    /** Watch exactly the directories currently rendered. */
    watchDirs(projectPath: string, dirs: string[]): Promise<void> {
      return invoke<void>('files_watch_dirs', { projectPath, dirs })
    },
    onChanged(handler: (event: FilesChangedEvent) => void): Promise<Unsubscribe> {
      return getHostTransport().subscribe('files-changed', handler)
    },
    onPathChanged(handler: (event: FilePathChangedPayload) => void): Promise<Unsubscribe> {
      return getHostTransport().subscribe('file-path-changed', handler)
    },
    onDeleted(handler: (event: { filePath: string }) => void): Promise<Unsubscribe> {
      return getHostTransport().subscribe('file-deleted', handler)
    },
    /**
     * File text plus the version of the bytes read, for conflict-checked writes.
     * Files over 16 MiB reject with `{ kind: 'tooLarge', size, limit }`; those stream.
     */
    read(projectPath: string, filePath: string): Promise<FileContent> {
      return invoke<FileContent>('file_read', { projectPath, filePath })
    },
    /**
     * `expectedVersion` is the version the caller last read; a mismatch rejects
     * the write with a `conflict` error carrying the on-disk version. `null`
     * overwrites whatever is there. Resolves with the written content's version.
     */
    write(
      projectPath: string,
      filePath: string,
      content: string,
      expectedVersion: string | null = null
    ): Promise<string> {
      return invoke<string>('file_write', { projectPath, filePath, content, expectedVersion })
    },
    createDir(projectPath: string, dirPath: string): Promise<void> {
      return invoke<void>('file_create_dir', { projectPath, dirPath })
    },
    rename(projectPath: string, oldPath: string, newPath: string): Promise<void> {
      return invoke<void>('file_rename', { projectPath, oldPath, newPath })
    },
    delete(projectPath: string, filePath: string, permanent = false): Promise<void> {
      return invoke<void>('file_delete', { projectPath, filePath, permanent })
    },
    paste(
      projectPath: string,
      targetDir: string,
      op: 'copy' | 'cut',
      sources: string[],
      collisionPolicy: 'auto_rename' | 'replace' | 'skip' | 'rename' | 'error' = 'auto_rename',
      renameMap?: Record<string, string>,
      permanent = false
    ): Promise<FilePasteMapping[]> {
      return invoke<FilePasteMapping[]>('file_paste', {
        projectPath,
        targetDir,
        op,
        sources,
        collisionPolicy,
        renameMap: renameMap ?? null,
        permanent
      })
    },
    pasteCollisions(projectPath: string, targetDir: string, sources: string[]): Promise<FilePasteCollision[]> {
      return invoke<FilePasteCollision[]>('file_paste_collisions', {
        projectPath,
        targetDir,
        sources
      })
    }
  },

  sessionIndex: {
    get(folderPath: string): Promise<SessionIndexSnapshot> {
      return invoke<SessionIndexSnapshot>('session_index_get', { folderPath })
    },
    /** Schedules source refreshes and returns the cache with refreshing flags set. */
    refresh(folderPath: string): Promise<SessionIndexSnapshot> {
      return invoke<SessionIndexSnapshot>('session_index_refresh', { folderPath })
    },
    onChanged(handler: (event: { folderPath: string }) => void): Promise<Unsubscribe> {
      return getHostTransport().subscribe('sessions-changed', handler)
    }
  },

  nix: {
    onChanged(handler: (event: { folderPath: string }) => void): Promise<Unsubscribe> {
      return getHostTransport().subscribe('nix-changed', handler)
    },
    detect(folderPath: string): Promise<NixDetection> {
      return invoke<NixDetection>('nix_detect', { folderPath })
    },
    select(folderPath: string, nixFile: string): Promise<NixEnvRecord> {
      return invoke<NixEnvRecord>('nix_select', { folderPath, nixFile })
    },
    evaluate(folderPath: string): Promise<NixEnvRecord> {
      return invoke<NixEnvRecord>('nix_evaluate', { folderPath })
    },
    clear(folderPath: string): Promise<void> {
      return invoke<void>('nix_clear', { folderPath })
    },
    lint(folderPath: string, filePath: string): Promise<NixDiagnostic[]> {
      return invoke<NixDiagnostic[]>('nix_lint', { folderPath, filePath })
    }
  },

  axo: {
    models(folderPath?: string | null): Promise<ChatModel[]> {
      return invoke('axo_models', { folderPath: folderPath ?? null })
    },
    providers(scopeFolder: string | null = null, folderPath?: string | null): Promise<AxoProvidersPayload> {
      return invoke('axo_providers', { folderPath: folderPath ?? null, scopeFolder })
    },
    setProviderKey(
      provider: string,
      key: string,
      scopeFolder: string | null = null,
      folderPath?: string | null
    ): Promise<void> {
      return invoke('axo_provider_set_key', { provider, key, folderPath: folderPath ?? null, scopeFolder })
    },
    removeProvider(
      provider: string,
      scopeFolder: string | null = null,
      folderPath?: string | null
    ): Promise<string | null> {
      return invoke('axo_provider_remove', { provider, folderPath: folderPath ?? null, scopeFolder })
    },
    refreshModels(folderPath?: string | null): Promise<void> {
      return invoke('axo_models_refresh', { folderPath: folderPath ?? null })
    },
    onModelsChanged(handler: (event: { server: string | null }) => void): Promise<Unsubscribe> {
      return getHostTransport().subscribe('models-changed', handler)
    },
    extensions(scopeFolder: string | null, folderPath?: string | null): Promise<AxoExtensions> {
      return invoke('axo_extensions_get', { scopeFolder, folderPath: folderPath ?? null })
    },
    setExtensionsEnabled(enabled: AxoEnabledExtension[], folderPath?: string | null): Promise<void> {
      return invoke('axo_extensions_set_enabled', { enabled, folderPath: folderPath ?? null })
    },
    setProjectExtensionsEnabled(
      scopeFolder: string,
      enabled: AxoEnabledExtension[],
      folderPath?: string | null
    ): Promise<void> {
      return invoke('axo_project_extensions_set_enabled', { scopeFolder, enabled, folderPath: folderPath ?? null })
    },
    retryExtension(id: string, scopeFolder: string | null, folderPath?: string | null): Promise<void> {
      return invoke('axo_extension_retry', { id, scopeFolder, folderPath: folderPath ?? null })
    },
    reloadExtension(id: string, scopeFolder: string | null, folderPath?: string | null): Promise<void> {
      return invoke('axo_extension_reload', { id, scopeFolder, folderPath: folderPath ?? null })
    },
    installExtensionDependencies(id: string, scopeFolder: string | null, folderPath?: string | null): Promise<void> {
      return invoke('axo_extension_install_dependencies', { id, scopeFolder, folderPath: folderPath ?? null })
    },
    inspectExtension(source: AxoExtensionSource, folderPath?: string | null): Promise<AxoExtensionStage> {
      return invoke('axo_extension_inspect', { source, folderPath: folderPath ?? null })
    },
    discardExtensionStage(stageId: string, folderPath?: string | null): Promise<void> {
      return invoke('axo_extension_stage_discard', { stageId, folderPath: folderPath ?? null })
    },
    installExtension(stageId: string, replaceOrigin: boolean, folderPath?: string | null): Promise<AxoInstallOutcome> {
      return invoke('axo_extension_install', { stageId, replaceOrigin, folderPath: folderPath ?? null })
    },
    rollbackExtension(id: string, folderPath?: string | null): Promise<void> {
      return invoke('axo_extension_rollback', { id, folderPath: folderPath ?? null })
    },
    uninstallExtension(id: string, deleteData: boolean, folderPath?: string | null): Promise<void> {
      return invoke('axo_extension_uninstall', { id, deleteData, folderPath: folderPath ?? null })
    },
    extensionsMemory(scopeFolder: string | null, folderPath?: string | null): Promise<AxoRunnerMemory[]> {
      return invoke('axo_extensions_memory', { scopeFolder, folderPath: folderPath ?? null })
    },
    checkExtensionsBun(folderPath?: string | null): Promise<void> {
      return invoke('axo_extensions_check_bun', { folderPath: folderPath ?? null })
    },
    onExtensionsChanged(handler: (event: { server: string | null }) => void): Promise<Unsubscribe> {
      return getHostTransport().subscribe('extensions-changed', handler)
    },
    onExtensionStopped(
      handler: (event: {
        server: string | null
        id: string
        folder: string | null
        problem: AxoExtensionProblem
      }) => void
    ): Promise<Unsubscribe> {
      return getHostTransport().subscribe('extension-stopped', handler)
    },
    loginStart(
      provider: string,
      scopeFolder: string | null = null,
      folderPath?: string | null
    ): Promise<AxoLoginStarted> {
      return invoke('axo_login_start', { provider, folderPath: folderPath ?? null, scopeFolder })
    },
    loginSubmit(loginId: string, questionId: number, answer: string, folderPath?: string | null): Promise<void> {
      return invoke('axo_login_submit', { loginId, questionId, answer, folderPath: folderPath ?? null })
    },
    loginStatus(loginId: string, folderPath?: string | null): Promise<AxoLoginStatus> {
      return invoke('axo_login_status', { loginId, folderPath: folderPath ?? null })
    },
    loginCancel(loginId: string, folderPath?: string | null): Promise<void> {
      return invoke('axo_login_cancel', { loginId, folderPath: folderPath ?? null })
    },
    keywords(folderPath?: string | null): Promise<AxoKeywordInfo[]> {
      return invoke('axo_keywords', { folderPath: folderPath ?? null })
    },
    webToolsStatus(folderPath?: string | null): Promise<AxoWebToolsStatusPayload> {
      return invoke('axo_web_tools_status', { folderPath: folderPath ?? null })
    },
    setSearchKey(provider: string, key: string, folderPath?: string | null): Promise<void> {
      return invoke('axo_search_key_set', { provider, key, folderPath: folderPath ?? null })
    },
    removeSearchKey(provider: string, folderPath?: string | null): Promise<void> {
      return invoke('axo_search_key_remove', { provider, folderPath: folderPath ?? null })
    },
    refreshDocs(source: string | null, folderPath?: string | null): Promise<void> {
      return invoke('axo_docs_refresh', { source, folderPath: folderPath ?? null })
    },
    removeDocs(source: string | null, folderPath?: string | null): Promise<void> {
      return invoke('axo_docs_remove', { source, folderPath: folderPath ?? null })
    }
  },

  memory: {
    status(folderPath?: string | null): Promise<MemoryStatus> {
      return invoke('memory_status', { folderPath: folderPath ?? null })
    },
    retryDownload(folderPath?: string | null): Promise<void> {
      return invoke('memory_retry_download', { folderPath: folderPath ?? null })
    },
    ensureBank(name: string, folderPath?: string | null): Promise<string> {
      return invoke('memory_ensure_bank', { name, folderPath: folderPath ?? null })
    },
    captures(query: MemoryCaptureQuery, folderPath?: string | null): Promise<MemoryCaptureRun[]> {
      return invoke('memory_captures', { query, folderPath: folderPath ?? null })
    },
    revertFact(factId: number, folderPath?: string | null): Promise<MemoryCaptureRun> {
      return invoke('memory_capture_revert', { factId, folderPath: folderPath ?? null })
    },
    /** The active bank name for a folder; `null` when memory is off for it. */
    folder(folderPath: string): Promise<string | null> {
      const scopeFolder = splitRemotePath(folderPath)?.path ?? folderPath
      return invoke('memory_folder', { scopeFolder, folderPath })
    },
    onCaptureChanged(
      handler: (event: { server: string | null; capture: MemoryCaptureRun; notify: boolean }) => void
    ): Promise<Unsubscribe> {
      return getHostTransport().subscribe('memory-capture-changed', handler)
    }
  },

  /** Always this desktop's predictor: typed text never leaves the machine. */
  predict: {
    /** `before` and `after` come from the one run of typed text holding the caret. */
    complete(before: string, after: string): Promise<PredictSuggestion | null> {
      return invoke('predict_complete', { before, after })
    },
    status(): Promise<PredictStatus> {
      return invoke('predict_status')
    },
    /** Forgets everything learned. */
    reset(): Promise<void> {
      return invoke('predict_reset')
    }
  },

  settings: {
    setMemory(settings: MemorySettings, folderPath?: string): Promise<MemorySettings> {
      return invoke<MemorySettings>('settings_set_memory', { settings, folderPath: folderPath ?? null })
    },
    setPredict(settings: PredictSettings): Promise<PredictSettings> {
      return invoke('settings_set_predict', { settings })
    },
    setAxo(patch: Record<string, unknown>, folderPath?: string): Promise<AxoSettings> {
      return invoke('settings_set_axo', { patch, folderPath: folderPath ?? null })
    },
    // The global layer of the host that runs `folderPath` — the exact layer
    // the setters below write. Omit it for this desktop.
    get(folderPath?: string): Promise<SettingsPayload> {
      return invoke<SettingsPayload>('settings_get', { folderPath: folderPath ?? null })
    },
    getEffective(folderPath?: string): Promise<EffectiveSettingsPayload> {
      return invoke<EffectiveSettingsPayload>('settings_get_effective', { folderPath: folderPath ?? null })
    },
    openFolderFile(folderPath: string): Promise<SettingsFileResult> {
      return invoke<SettingsFileResult>('settings_open_folder_file', { folderPath })
    },
    onChanged(handler: (event: SettingsChangedEvent) => void): Promise<Unsubscribe> {
      return getHostTransport().subscribe('settings-changed', handler)
    },
    setWindow(settings: WindowSettings): Promise<WindowSettings> {
      return invoke<WindowSettings>('settings_set_window', { settings })
    },
    setTypography(settings: TypographySettings): Promise<TypographySettings> {
      return invoke<TypographySettings>('settings_set_typography', { settings })
    },
    setNix(settings: NixSettings, folderPath?: string): Promise<NixSettings> {
      return invoke<NixSettings>('settings_set_nix', { settings, folderPath: folderPath ?? null })
    },
    setFormatting(formatting: FormattingSettings, folderPath?: string): Promise<FormattingSettings> {
      return invoke<FormattingSettings>('settings_set_formatting', {
        formatting,
        folderPath: folderPath ?? null
      })
    },
    setHarnessConfig(config: HarnessConfig, folderPath?: string): Promise<HarnessConfig> {
      return invoke<HarnessConfig>('settings_set_harness_config', { config, folderPath: folderPath ?? null })
    },
    setShellConfig(config: ShellConfig, folderPath?: string): Promise<ShellConfig> {
      return invoke<ShellConfig>('settings_set_shell_config', { config, folderPath: folderPath ?? null })
    }
  },

  shortcuts: {
    getGlobal(): Promise<ShortcutsFilePayload> {
      return invoke<ShortcutsFilePayload>('shortcuts_get_global')
    },
    setGlobal(value: unknown): Promise<ShortcutsFilePayload> {
      return invoke<ShortcutsFilePayload>('shortcuts_set_global', { value })
    }
  },

  integrations: {
    getCatalog(folderPath?: string | null): Promise<IntegrationCatalog> {
      return invoke<IntegrationCatalog>('integrations_get_catalog', { folderPath: folderPath ?? null })
    },
    getGrammars(folderPath?: string | null): Promise<IntegrationGrammar[]> {
      return invoke<IntegrationGrammar[]>('integrations_get_grammars', { folderPath: folderPath ?? null })
    },
    reload(folderPath?: string | null): Promise<void> {
      return invoke<void>('integrations_reload', { folderPath: folderPath ?? null })
    }
  },

  environment: {
    reload(folderPath?: string | null): Promise<void> {
      return invoke<void>('environment_reload', { folderPath: folderPath ?? null })
    }
  },

  configSchemas: {
    list(folderPath?: string | null): Promise<ConfigSchemaEntry[]> {
      return invoke<ConfigSchemaEntry[]>('config_schemas_list', { folderPath: folderPath ?? null })
    }
  },

  issues: {
    onChanged(handler: (event: { folderPath: string }) => void): Promise<Unsubscribe> {
      return getHostTransport().subscribe('issues-changed', handler)
    },
    list(folderPath: string, filters: IssueListFilters = {}): Promise<Issue[]> {
      return invoke<Issue[]>('issues_list', { folderPath, filters })
    },
    get(folderPath: string, issueId: string): Promise<IssueDetail> {
      return invoke<IssueDetail>('issues_get', { folderPath, issueId })
    },
    create(folderPath: string, input: IssueCreateInput): Promise<Issue> {
      return invoke<Issue>('issues_create', { folderPath, input })
    },
    update(folderPath: string, issueId: string, patch: IssueUpdateInput): Promise<Issue> {
      return invoke<Issue>('issues_update', { folderPath, issueId, patch })
    },
    delete(folderPath: string, issueId: string): Promise<void> {
      return invoke<void>('issues_delete', { folderPath, issueId })
    },
    currentGitUser(folderPath: string): Promise<string> {
      return invoke<string>('issue_current_git_user', { folderPath })
    },
    epics: {
      create(folderPath: string, input: IssueEpicCreateInput): Promise<IssueEpic> {
        return invoke<IssueEpic>('issue_epics_create', { folderPath, input })
      },
      list(folderPath: string): Promise<IssueEpic[]> {
        return invoke<IssueEpic[]>('issue_epics_list', { folderPath })
      },
      get(folderPath: string, epicId: string): Promise<IssueEpic> {
        return invoke<IssueEpic>('issue_epics_get', { folderPath, epicId })
      },
      update(folderPath: string, epicId: string, patch: IssueEpicUpdateInput): Promise<IssueEpic> {
        return invoke<IssueEpic>('issue_epics_update', {
          folderPath,
          epicId,
          patch
        })
      },
      delete(folderPath: string, epicId: string): Promise<void> {
        return invoke<void>('issue_epics_delete', { folderPath, epicId })
      }
    },
    comments: {
      add(folderPath: string, input: IssueCommentCreateInput): Promise<IssueComment> {
        return invoke<IssueComment>('issue_comments_add', { folderPath, input })
      }
    }
  },

  tasks: {
    onChanged(handler: (folderPath: string) => void): Promise<Unsubscribe> {
      return getHostTransport().subscribe('tasks-changed', handler)
    },
    /** Return the parsed task list for a folder. Empty array when no `.axome/tasks.jsonc` exists. */
    list(folderPath: string): Promise<TaskDefinition[]> {
      return invoke<TaskDefinition[]>('tasks_list', { folderPath })
    },
    /**
     * Start or attach a task PTY. `attachOnly` rejects unknown IDs instead of
     * executing again; `runId` identifies write/resize/stop calls.
     */
    start(
      runId: string,
      folderPath: string,
      taskId: string,
      activeFilePath: string | null,
      cols: number,
      rows: number,
      attachOnly: boolean,
      sinks: PtySinks
    ): StreamHandle<void> {
      return getHostTransport().openStream(
        {
          method: 'tasks_start',
          params: { runId, folderPath, taskId, activeFilePath, cols, rows, attachOnly }
        },
        sinks
      )
    },
    stop(runId: string): Promise<void> {
      return invoke<void>('tasks_stop', { runId })
    }
  },

  formatting: {
    format(folderPath: string, filePath: string, formatterId: string, content: string): Promise<string> {
      return invoke<string>('formatting_format', { folderPath, filePath, formatterId, content })
    },
    listFormatters(folderPath?: string | null): Promise<FormatterStatus[]> {
      return invoke<FormatterStatus[]>('formatting_list_formatters', { folderPath: folderPath ?? null })
    }
  },

  lsp: {
    listServers(folderPath?: string): Promise<LspServerSettingsEntry[]> {
      return invoke<LspServerSettingsEntry[]>('lsp_list_servers', {
        folderPath: folderPath ?? null
      })
    },
    setServerConfig(config: LspServerConfig, folderPath?: string): Promise<LspServerConfig> {
      return invoke<LspServerConfig>('lsp_set_server_config', {
        config,
        folderPath: folderPath ?? null
      })
    },
    start(
      sessionId: string,
      folderPath: string,
      serverDefinitionId: string,
      rootPath: string,
      sinks: { onEvent: (event: LspEvent) => void }
    ): StreamHandle<void> {
      return getHostTransport().openStream(
        {
          method: 'lsp_start',
          params: { sessionId, folderPath, serverDefinitionId, rootPath }
        },
        sinks
      )
    },
    send(sessionId: string, messageJson: string): Promise<void> {
      return invoke<void>('lsp_send', { sessionId, messageJson })
    },
    stop(sessionId: string): Promise<void> {
      return invoke<void>('lsp_stop', { sessionId })
    }
  }
}
