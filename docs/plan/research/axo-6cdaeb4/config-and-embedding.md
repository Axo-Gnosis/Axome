# axo 6cdaeb4: config, construction, embedding

Raw survey report, kept verbatim as evidence. Produced by a read-only subagent on 2026-11-09 against axo `6cdaeb4`, verified against source code (not docs). Host-owned config, `Axo::builder`, axo-ai changes, errors, tool trait. Still current for construction and config; superseded for snapshot/events by the 843cd73 reports.

Citation prefixes are defined inside the report. Claims marked [INFERENCE] were not observed directly.

---

# Research Report: Axo Harness Upgrades (`26066e2` → `6cdaeb4`) — Host-Facing Systems

## 1. What axo Now Provides

### Process-Wide Entry Point & Embedding
* **`Axo::builder(models: Arc<Models>, store: Arc<dyn SessionStore>) -> AxoBuilder`** (`crates/axo/src/axo.rs:132`):
  Requires host-owned `Models` and `SessionStore`. Filesystem discovery (`AXO_DIR`, `$HOME/.axo`) is eliminated.
* **`AxoBuilder::build(self) -> Axo`** (`crates/axo/src/axo.rs:108`):
  Synchronous and infallible. No configuration I/O occurs during `build()`.
* **`AxoBuilder::name(mut self, name: impl Into<String>) -> Result<Self, Error>`** (`crates/axo/src/axo.rs:77`):
  Sets the harness name (default `"axo"`). Validated via `check_harness_name(&name)` against `^[a-z][a-z0-9+.-]+$` and reserved schemes (`artifact`, `file`, `skill`, `local`, `proc`, `agent`) (`crates/axo/src/schemes/mod.rs:27-42`). Used in bundled prompts and docs scheme `<name>://`.
* **`AxoBuilder::config(mut self, config: Arc<dyn ConfigSource>) -> Self`** (`crates/axo/src/axo.rs:85`):
  Plugs in the host's configuration provider. Defaults to `NoConfig` (empty user layer and empty project layer) (`crates/axo/src/axo.rs:60-70`).
* **`AxoBuilder::cache_dir(mut self, cache_dir: PathBuf) -> Self`** (`crates/axo/src/axo.rs:91`):
  Configures axo-owned cache directory. When `None`, no `<name>://` docs scheme is mounted, the `docs` prompt section is omitted, and Nix dev-shell falls back to host environment (`crates/axo/src/axo.rs:331-337`).
* **`AxoBuilder::supervision(mut self, supervision: Supervision) -> Self`** (`crates/axo/src/axo.rs:103`):
  Configures process supervision (`Supervision::ProcessGroup` or `Supervision::Cgroup`) (`crates/axo/src/exec/host.rs:41-45`).

### Pluggable Configuration & Resolution
* **`pub trait ConfigSource: Send + Sync`** (`crates/axo/src/config.rs:428-436`):
  ```rust
  fn user(&self) -> Result<UserConfig, Error>;
  fn project(&self, cwd: &Path) -> Result<ConfigLayer, Error>;
  ```
  `user()` is sampled once per `create_session`, `open_session`, and `Axo::schemes`. `project(cwd)` is called at most once per session when effective access is `Read` or `Trust` and `user.settings.project.files.config` is `true`.
* **`pub struct ConfigLayer`** (`crates/axo/src/config.rs:401-412`):
  `origin: String`, `settings: Option<Value>`, `prompts: IndexMap<String, PromptOverride>`, `agent_roots: Vec<PathBuf>`.
* **`pub struct UserConfig`** (`crates/axo/src/config.rs:415-422`):
  `layer: ConfigLayer`, `skill_roots: Vec<PathBuf>`, `context_files: Vec<PathBuf>`.
* **`pub struct EffectiveConfig`** (`crates/axo/src/config.rs:439-448`):
  `access: ProjectAccess`, `settings: Settings`, `prompts: IndexMap<String, PromptOverride>`, `skill_roots: Vec<PathBuf>`, `agent_roots: Vec<PathBuf>`, `context_files: Vec<PathBuf>`.
* **`EffectiveConfig::resolve(source: &dyn ConfigSource, cwd: &Path, access: Option<ProjectAccess>) -> Result<Self, Error>`** (`crates/axo/src/config.rs:456-512`):
  Single evaluation point. Strips user-only keys (`project`, `homeAgentsDir`, `modelRoles`, `subagents`) from project layer (`crates/axo/src/config.rs:457,487`). Forces `bash.devShell = Off` if access != `ProjectAccess::Trust` (`crates/axo/src/config.rs:498-505`).
* **`EffectiveConfig::default_model(&self) -> Result<Option<String>, Error>`** (`crates/axo/src/config.rs:517-531`):
  Evaluated lazily only when falling back to default model (`crates/axo/src/axo.rs:317`). Interpolates `$VAR` / `${VAR}`; rejects values starting with `!`.
* **`project.files.config` switch** (`crates/axo/src/config.rs:172`):
  Boolean flag on `ProjectFiles` (default `true`) gating whether the project configuration layer is loaded.
* **`DiagnosticOrigin`** (`crates/axo/src/context/mod.rs:27-32`):
  `pub enum DiagnosticOrigin { Path(PathBuf), Host(String) }`.
  `Diagnostic.origin` changed from `Option<PathBuf>` to `Option<DiagnosticOrigin>` (`crates/axo/src/context/mod.rs:21`).

### Errors
* **`pub enum Error`** (`crates/axo/src/error.rs:5-66`):
  - Added: `Host(Box<dyn std::error::Error + Send + Sync>)` (`error.rs:20`), `InvalidSessionHeader { path: PathBuf, message: String }` (`error.rs:23`), `AgentNotFound(String)` (`error.rs:32`), `SpawnBlocked(String)` (`error.rs:35`), `SessionNotFound(String)` (`error.rs:38`), `ChildSession { path: PathBuf }` (`error.rs:41`), `UnknownModelRole(String)` (`error.rs:44`), `InvalidModelSelector { selector: String, message: String }` (`error.rs:47`).
  - Modified: `Config { origin: String, message: String }` (`error.rs:17`, previously held `path: PathBuf`).
  - Removed: `NoAxoDir` (`[DELETED]`).

### `axo-ai` Runtime & Transport
* **`ModelsOptions.http_client: Option<reqwest::Client>`** (`crates/axo-ai/src/models/mod.rs:40`):
  Shared HTTP client injected by host for catalog fetching, discovery, streaming, and OAuth refresh (`crates/axo-ai/src/models/mod.rs:57`).
* **`ProviderAuth`** (`crates/axo-ai/src/auth/resolve.rs:14-22`):
  `pub(crate) struct ProviderAuth { pub id: String, pub config_api_key: Option<String>, pub env: Vec<String>, pub requires_key: bool }`. Replaced `ProviderAuthInfo` and `ProviderEntry`.
* **Command Execution Rejection (`!cmd`)** (`crates/axo-ai/src/auth/resolve.rs:29-34`):
  `resolve_config_value` explicitly rejects values starting with `!`. Hosts must resolve command keys before providing configuration.
* **Transport Retry** (`crates/axo-ai/src/api/http.rs:27-40`):
  Internal fixed 2-retry policy (`MAX_REQUEST_RETRIES = 2`, `MAX_SERVER_DELAY = 60s`).
* **Turn Retry Policy in Settings** (`crates/axo/src/config.rs:205-219`):
  `pub struct RetrySettings { pub max_retries: u32, pub base_delay_ms: u64, pub max_delay_ms: u64 }`. Evaluated in `crates/axo/src/recovery.rs:18-37`.
* **Removed types/fields**:
  `SimpleStreamOptions::api_key` (`[DELETED]`), `Model.headers` (`[DELETED]`), `Context` struct (`[DELETED]`), `axo_ai::retry::retry_delay` (`[DELETED]`).

### Tool & Hooks Seam
* **`pub trait ToolHooks: Send + Sync`** (`crates/axo-agent/src/hooks.rs:154-171`):
  ```rust
  async fn before_tool_call(&self, context: &BeforeToolCallContext<'_>, cancel: &CancellationToken) -> Result<ToolCallDecision, ToolError>;
  async fn after_tool_call(&self, context: &AfterToolCallContext<'_>, cancel: &CancellationToken) -> Result<Option<AfterToolCallResult>, ToolError>;
  ```
  Host tool policy seam passed via `SessionOptions::tool_hooks: Vec<Arc<dyn ToolHooks>>` (`crates/axo/src/session/mod.rs:103`).
* **`LoopHooks` Trimmed** (`crates/axo-agent/src/hooks.rs:93-149`):
  `before_tool_call`, `after_tool_call`, `transform_context`, `follow_up`, `PrepareRequestContext`, and `TurnUpdate` removed.
* **Owned `ToolContext` + `ToolUpdates`** (`crates/axo-agent/src/tool.rs:64-96`):
  `Tool::execute(&self, cx: ToolContext) -> Result<ToolOutput, ToolError>` (`crates/axo-agent/src/tool.rs:34`).
  `ToolContext` holds `call: Arc<ToolCall>`, `cancel: CancellationToken`, `updates: ToolUpdates`, `steer: SteerSignal`, `invocation: ToolInvocationId`.
  Tools run as spawned tasks with a 3-second cooperative cancellation grace on stop before task abort (`crates/axo-agent/src/tool.rs:30-33`, `crates/axo-agent/src/run.rs:727-742`).

### Session Live State & Methods
* **`LiveSnapshot` Flattened Fields** (`crates/axo/src/session/mod.rs:251-267`):
  `AgentSnapshot` deleted. `LiveSnapshot` exposes:
  `run_id: Option<String>`, `maintenance: Option<MaintenanceKind>`, `running: bool`, `streaming: Option<AssistantMessage>`, `pending_tool_calls: Vec<ToolCall>`, `queued_steering: Vec<AgentMessage>`, `queued_follow_up: Vec<AgentMessage>`, `messages: Vec<AgentMessage>`, `pending_questions: Vec<(String, Vec<Question>)>`, `jobs: Vec<JobInfo>`.
* **`Session::abort(&self)`** (`crates/axo/src/session/mod.rs:1736`):
  Synchronous method.
* **Session Metadata & Custom State APIs**:
  - `Session::name(&self) -> Option<String>` (`crates/axo/src/session/mod.rs:1599`)
  - `Session::set_name(&self, name: Option<String>) -> Result<SeqEntry, Error>` (`crates/axo/src/session/mod.rs:1584`)
  - `Session::custom(&self, custom_type: &str) -> Option<Value>` (`crates/axo/src/session/mod.rs:1634`)
  - `Session::append_custom(&self, custom_type: &str, data: Option<Value>) -> Result<SeqEntry, Error>` (`crates/axo/src/session/mod.rs:1615`)
  - `Session::diagnostics(&self) -> &[Diagnostic]` (`crates/axo/src/session/mod.rs:1663`)
* **`StaticEnv::host()`** (`crates/axo/src/exec/mod.rs:59`):
  Replaced `HostEnv::snapshot()`.
* **`SessionWriter::path(&self) -> &Path`** (`crates/axo/src/store/mod.rs:56`):
  Replaced `planned_path()`. Always returns the path, even before lazy file creation.

### Compaction & Commands
* **`CompactionReason`** (`crates/axo/src/store/entry.rs:538-545`):
  Enum: `Manual`, `Threshold`, `Overflow`. Recorded in `Entry::Compaction` (`crates/axo/src/store/entry.rs:265`).
* **Compaction & Navigation Focus**:
  - `session.compact(focus: Option<String>) -> Result<(), Error>` (`crates/axo/src/session/maintenance.rs:599`)
  - `Command::Compact { focus: Option<String> }` (`crates/axo/src/commands.rs:20`)
  - `Command::Navigate { entry: EntryId, summarize: bool, focus: Option<String> }` (`crates/axo/src/commands.rs:32`)
  - `NavigateOptions { pub summarize: bool, pub focus: Option<String> }` (`crates/axo/src/session/mod.rs:165-169`)
  - Parameter decoding parses trailing `"format": "text"` properties verbatim (`crates/axo/src/commands.rs:263-274`).

### Stored Entries & Header
* **New `Entry` variants** (`crates/axo/src/store/entry.rs:369-414`):
  - `Entry::JobStart { base, session_id, job_id, call_id, command, artifact }`
  - `Entry::ChildStart { base, session_id, job_id, call_id, child_session_id, child_path, agent, model, team }`
  - `Entry::JobEnd { base, job_id, text, is_error, partial, run_id, note }`
* **Entry Field Changes**:
  - `Entry::Message`: added `admitted_jobs: Vec<String>`, `delivered_jobs: Vec<String>` (`entry.rs:221-226`).
  - `Entry::Compaction`: added `reason: Option<CompactionReason>`, `focus: Option<String>` (`entry.rs:265-267`).
  - `Entry::BranchSummary`: added `focus: Option<String>` (`entry.rs:286`).
  - `SubmissionKind::Mail` added (`entry.rs:552`).
  - `Header.spawn: Option<SpawnLink>` added (`entry.rs:82`).
  - `SessionInfo.spawn: Option<SpawnLink>` added (`entry.rs:573`).
  - `Entry::requires_sync(&self) -> bool` (`entry.rs:418-433`): identifies entries requiring durable flush (`RunStart`, `RunEnd`, `JobStart`, `ChildStart`, `JobEnd`, and job-admitting/delivering `Message`s).

### Prompts & Sections
* **Prompt Templates**:
  - `SYSTEM.md` (`crates/axo/prompts/SYSTEM.md:1,6`): Mentions `{name}` and directs plans/scratch files to `local://<path>`.
  - `SUMMARY.md` (`crates/axo/prompts/SUMMARY.md:1,11`): Mentions `{name}`.
  - `DELEGATION.md` (`crates/axo/prompts/DELEGATION.md:1`): Autonomous subagent directive.
* **`PromptSources`** (`crates/axo/src/context/prompt.rs:46-64`):
  Single owner of session prompt sources. Section rendering order:
  `delegation, system, docs, tools, schemes, work, prose, project_context, skills, team, cwd, dev_shell, date, <host sections>` (`crates/axo/src/context/prompt.rs:18`).
* **`escape_wrappers`** (`crates/axo/src/context/prompt.rs:24-35`):
  Sanitizes wrapper XML tags in input content.

### Tools & Schemes
* **`tools/output.rs`** (`crates/axo/src/tools/output.rs:1-139`):
  Provides `yield` tool for structured subagent results.
* **`truncate.rs`** (`crates/axo/src/tools/truncate.rs:40-73`):
  `last_line_partial` field and `lastLinePartial` JSON key removed. `truncate_tail` and `truncate_middle` removed.
* **`edit.rs` Details** (`crates/axo/src/tools/edit.rs:206`):
  Tool execution details now include `"ledger": ledger` tracking file reads and writes via `tools/ledger.rs`.
* **Schemes**:
  - `artifact://`: `Artifacts` tracks live writing handles (`LiveArtifact`) (`crates/axo/src/schemes/artifact.rs:22-42`). `bash_outputs` renamed to `command_logs` (`crates/axo/src/schemes/artifact.rs:161`). `bash_tail` renamed to `command_log_tail` (`crates/axo/src/schemes/artifact.rs:150`).
  - `DirScheme::docs(name, root)` (`crates/axo/src/schemes/dir.rs:66`): Accepts harness name and rewrites `axo://` in bundled docs to `<name>://`.

---

## 2. Breaking Changes Hitting Axome's Code

| Axome File & Line | What Breaks | Replacement / Required Change |
| :--- | :--- | :--- |
| `src-crates/axome-core/src/services/axo/mod.rs:140-143` | `Axo::builder().build().await` fails. `Axo::builder` now requires `(models: Arc<Models>, store: Arc<dyn SessionStore>)`, and `AxoBuilder::build` is synchronous returning `Axo` (not `Result`). | Initialize `Models` and `SessionStore` (e.g. `JsonlStore`), supply optional `ConfigSource` and `cache_dir`, then call `Axo::builder(models, store)...build()`. |
| `src-crates/axome-core/src/services/axo/mod.rs:53-73` | Non-exhaustive `match &error` in `axo_error`: `axo::Error::NoAxoDir` removed; new variants `Host`, `InvalidSessionHeader`, `AgentNotFound`, `SpawnBlocked`, `SessionNotFound`, `ChildSession`, `UnknownModelRole`, `InvalidModelSelector` are unhandled. | Remove `Error::NoAxoDir`. Handle new variants: map `AgentNotFound`/`SessionNotFound` to `ApiError::NotFound`; `UnknownModelRole`/`InvalidModelSelector`/`ChildSession`/`SpawnBlocked` to `ApiError::InvalidArgument`; `InvalidSessionHeader`/`Host` to `ApiError::Internal`. |
| `src-crates/axome-core/src/services/axo/mod.rs:301` | `axo.models().auth(model, None).await` fails: `override_key` parameter was removed from `Models::auth`. | Call `axo.models().auth(model).await`. |
| `src-crates/axome-core/src/services/axo/mod.rs:331-335` | `axo.settings()` method was deleted from `Axo`. | Settings are now resolved per-session. Read default model and thinking from `ConfigSource::user()?.layer.settings` or cache resolved host settings. |
| `src-crates/axome-core/src/services/axo/mod.rs:423` | `self.live(folder, id).await?.session.abort().await;` fails: `Session::abort(&self)` is now synchronous (returns `()`, not a `Future`). | Remove `.await` on `session.abort()`. |
| `src-crates/axome-core/src/services/axo/mod.rs:530` and `:544` | `NavigateOptions { summarize: false }` fails to compile: missing required field `focus`. | Use `NavigateOptions { summarize: false, focus: None }`. |
| `src-crates/axome-core/src/services/axo/live.rs:64,71,95` | `snapshot.agent.running`, `snapshot.agent.pending_tool_calls`, `snapshot.agent.streaming` fail: `LiveSnapshot.agent` field was replaced with flattened top-level fields. | Access `snapshot.running`, `snapshot.pending_tool_calls`, and `snapshot.streaming`. |
| `src-crates/axome-core/src/services/axo/live.rs:265-462` | Non-exhaustive match in `LiveSession::apply`: `SessionEvent::Jobs { jobs }` is not handled. | Add branch for `SessionEvent::Jobs { .. }` (emit or track background job updates). |
| `src-crates/axome-core/src/services/axo/convert.rs:635-638` | `queue_at_attach(entries: &[SeqEntry], snapshot: &AgentSnapshot)` fails: `AgentSnapshot` was deleted from `axo-agent`. | Pass `&LiveSnapshot` (or `(&[AgentMessage], &[AgentMessage])`) and inspect `snapshot.queued_steering` / `snapshot.queued_follow_up`. |
| `src-crates/axome-core/src/services/axo/convert.rs:660` | `match kind` in `queue_at_attach` fails: `SubmissionKind::Mail` is unhandled. | Add branch for `SubmissionKind::Mail` (or handle mail submissions). |
| `src-crates/axome-core/src/services/axo/convert.rs:341-477` | Non-exhaustive match in `convert::entry`: missing `Entry::JobStart`, `Entry::ChildStart`, `Entry::JobEnd`. | Handle or ignore `JobStart`, `ChildStart`, `JobEnd` in the pattern match. |
| `src-crates/axome-core/src/services/axo/convert.rs:494-528` | Non-exhaustive match in `convert::title_of`: missing `Entry::JobStart`, `Entry::ChildStart`, `Entry::JobEnd`. | Add `Entry::JobStart { .. } \| Entry::ChildStart { .. } \| Entry::JobEnd { .. }` to the ignored entries list. |
| `src-crates/axome-core/src/services/axo/convert.rs:647-657` | Non-exhaustive match in `matched` filter of `queue_at_attach`: missing `Entry::JobStart`, `Entry::ChildStart`, `Entry::JobEnd`. | Add new entry kinds to the ignored filter list. |
| `src-crates/axome-core/src/services/axo/tests.rs:75-81` | `Axo::builder().dir(...).settings(...).models(...).streamer(...).build().await.unwrap()` fails on multiple removed builder methods and async `build()`. | Pass `models` and `store` to `Axo::builder(...)`, pass settings via `ConfigSource`, and remove `.await.unwrap()` on `build()`. |

---

## 3. Silent Behavior and Wire Format Changes

1. **Tool Truncation UI Broken in Frontend**:
   * *Location*: `src/lib/features/chat/elements/tool/payload.ts:27` and `readOutput.test.ts:34`.
   * *Mechanism*: `payload.ts` validates truncation using:
     ```ts
     cut.lastLinePartial === false
     ```
     `lastLinePartial` was completely removed from axo's `truncation_details` JSON output (`crates/axo/src/tools/truncate.rs:64-73`). In JavaScript, `cut.lastLinePartial` evaluates to `undefined`, and `undefined === false` is `false`.
   * *Impact*: `truncation(value)` returns `null` for every truncated output. Axome UI silently ceases displaying truncation notices and artifact view links for truncated tool outputs.
2. **Rejection of `!cmd` in Models Configuration & Settings**:
   * *Location*: `crates/axo-ai/src/auth/resolve.rs:29-34` and `crates/axo/src/config.rs:528`.
   * *Mechanism*: Previously, API keys or headers beginning with `!command` executed shell commands. Now, `resolve_config_value` immediately returns `Error::Config("command config values starting with '!' are not supported...")`.
   * *Impact*: Any user configuration with shell command interpolation will fail at request/session start.
3. **Ignored Compaction Reason and Focus in Session History**:
   * *Location*: `crates/axo/src/store/entry.rs:265-267` vs `src-crates/axome-core/src/services/axo/convert.rs:434-444`.
   * *Mechanism*: `Entry::Compaction` records `reason: Option<CompactionReason>` (`Manual`, `Threshold`, `Overflow`) and `focus: Option<String>`. Axome extracts `summary` and `tokens_before` using `..` ignore patterns, dropping reason and focus without exposing them on `ChatEntryBody::Compaction`.
4. **Ignored Background Job Receipts in Messages**:
   * *Location*: `crates/axo/src/store/entry.rs:221-226`.
   * *Mechanism*: `Entry::Message` now contains `admitted_jobs: Vec<String>` and `delivered_jobs: Vec<String>`. Axome's converter ignores these fields.
5. **System Prompt & Summary Formatting**:
   * *Location*: `crates/axo/prompts/SYSTEM.md` and `crates/axo/prompts/SUMMARY.md`.
   * *Mechanism*: The prompt text replaces `"running in axo"` with `"running in {name}"`. If `AxoBuilder::name` is set to `"axome"`, the agent identifies as running in axome. Furthermore, `SYSTEM.md` now explicitly directs the agent: `Use local://<path> for plans, notes, and scratch files instead of creating temporary files in the project directory.`
6. **`edit` Tool Details Includes `ledger`**:
   * *Location*: `crates/axo/src/tools/edit.rs:206`.
   * *Mechanism*: `edit` tool results include `"ledger": { "path": ..., "version": ..., "access": "modify" }` alongside `"diff"`, `"patch"`, and `"first_changed_line"`.
7. **Read-Before-Write/Edit Enforcement via `FileLedger`**:
   * *Location*: `crates/axo/src/tools/edit.rs:163,181` and `crates/axo/src/tools/write.rs:114`.
   * *Mechanism*: Axo now tracks file access versions. If an existing file is edited or overwritten before the session reads it, or if it was modified externally after being read, the tool returns a failure error result.

---

## 4. Responsibility Split

| Concern | axo Owns | Axome Must Supply / Own |
| :--- | :--- | :--- |
| **Configuration Acquisition** | Settings schema, layer merging, trust filtering, `$VAR` expansion (`EffectiveConfig::resolve`). | Implementing `ConfigSource` (`user()` and `project(cwd)`). Acquiring `.axome/settings.jsonc`, user defaults, and repo config layers. |
| **Model Registry & Auth** | Model options clamping, provider dispatch, request-level retry, streaming decode. | Building `Models` instance (`Models::load(ModelsOptions)`), HTTP client policy (`reqwest::Client`), and `CredentialStore` implementation (`FileCredentialStore` or DB-backed). |
| **Session Persistence** | Journal serialisation, lock acquisition, atomic batch commits, torn-line recovery (`JsonlStore`). | Instantiating `SessionStore` (e.g. `Arc::new(JsonlStore::new(session_root))`), choosing storage directory layout. |
| **Cache & Documentation** | Materializing bundled docs into `<cache_dir>/docs/`, Nix devshell captures into `<cache_dir>/devshell/`. | Allocating cache directory via `AxoBuilder::cache_dir` (e.g. `~/.cache/axome` or app data dir). |
| **Subprocess Supervision** | Process group / cgroup isolation, SIGTERM→SIGKILL escalation, pipeline capture (`ProcessSupervisor`). | Choosing supervision mode via `AxoBuilder::supervision`. |
| **Execution Environment** | Nix dev shell capture (`DevShellEnv`), static host env (`StaticEnv`). | Supplying `FolderExecEnv` via `SessionOptions::exec_env` bridging Nix daemon and Axome host environment. |
| **Tool Policy & Security** | Ordered hook invocation (`ToolHooks`), cancellation token propagation, execution timeout. | Supplying `Vec<Arc<dyn ToolHooks>>` in `SessionOptions::tool_hooks` to enforce confirmation, security boundaries, or workspace access rules. |
| **UI & Wire Translation** | Raw session entries, snapshots, events. | Converting axo wire models to Axome RPC protocol (`axome-protocol`), handling frontend rendering. |

---

## 5. Host Observability and Controls

### Push Subscriptions
* **`Session::attach(after_seq: Option<u64>) -> Attach`** (`crates/axo/src/session/delivery.rs:24-65`):
  Atomically returns missed entries (`Attach.missed: Vec<SeqEntry>`), current point-in-time live snapshot (`Attach.live: LiveSnapshot`), and a push event broadcast receiver (`Attach.events: broadcast::Receiver<SessionEvent>`).
* **`SessionEvent` Stream** (`crates/axo/src/session/mod.rs:190-216`):
  - `SessionEvent::Agent(AgentEvent)`: LLM tokens (`MessageUpdate`), tool execution lifecycle (`ToolExecutionStart`, `ToolExecutionUpdate`, `ToolExecutionEnd`).
  - `SessionEvent::Entry(SeqEntry)`: Durable journal appends (turns, changes, compaction).
  - `SessionEvent::QuestionPending { call_id, questions }` / `QuestionResolved { call_id }`: Interactive user question state.
  - `SessionEvent::MaintenanceStart { kind }` / `MaintenanceEnd { kind, error, aborted }`: Background maintenance cycles (`Compact`, `Shake`, `BranchSummary`).
  - `SessionEvent::Retry { attempt, delay_ms }`: Automated turn retry countdowns.
  - `SessionEvent::Jobs { jobs: Vec<JobInfo> }`: Background command and child session updates.

### Point-in-Time State (Poll)
* **`LiveSnapshot`** (`crates/axo/src/session/mod.rs:251-267`):
  Read at attach or reconstructed from journal. Fields:
  `run_id`, `maintenance`, `running`, `streaming`, `pending_tool_calls`, `queued_steering`, `queued_follow_up`, `messages`, `pending_questions`, `jobs`.
* **Session Metadata & Introspection Methods**:
  - `session.id() -> &str` (`crates/axo/src/session/mod.rs:1575`): Header UUIDv7.
  - `session.path() -> &Path` (`crates/axo/src/session/mod.rs:1570`): File path (even if unwritten).
  - `session.name() -> Option<String>` (`crates/axo/src/session/mod.rs:1599`): Session title from journal.
  - `session.custom(custom_type: &str) -> Option<Value>` (`crates/axo/src/session/mod.rs:1634`): Host branch state.
  - `session.diagnostics() -> &[Diagnostic]` (`crates/axo/src/session/mod.rs:1663`): Non-fatal context/skill/scheme warnings.
  - `session.jobs() -> Vec<JobInfo>` (`crates/axo/src/session/mod.rs:1668`): Background jobs list.
  - `session.schemes() -> &Schemes` (`crates/axo/src/session/mod.rs:1679`): Registered URI schemes.
  - `session.commands() -> Vec<CommandInfo>` (`crates/axo/src/session/mod.rs:1685`): Command catalog.

### Control Operations
* **Turn Lifecycle**:
  - `Session::prompt(parts)` / `prompt_with(request)`: Starts run from idle (`crates/axo/src/session/mod.rs:1515`).
  - `Session::steer(parts)`: Injects steering message drained after current turn's tool calls (`crates/axo/src/session/mod.rs:1529`).
  - `Session::follow_up(parts)`: Injects message drained when run would otherwise end (`crates/axo/src/session/mod.rs:1535`).
  - `Session::abort(&self)`: Synchronously cancels active run (`CancelReason::User`) (`crates/axo/src/session/mod.rs:1736`).
  - `Session::shutdown(&self).await`: Gracefully stops run and terminates background processes (`crates/axo/src/session/mod.rs:1750`).
* **Maintenance & Branching**:
  - `Session::compact(focus: Option<String>)`: Trigger manual compaction with optional focus (`crates/axo/src/session/maintenance.rs:599`).
  - `Session::shake(mode: ShakeMode)`: Context elision (`crates/axo/src/session/maintenance.rs:570`).
  - `Session::navigate(entry, NavigateOptions { summarize, focus })`: Switch active branch (`crates/axo/src/session/maintenance.rs:504`).
  - `Session::fork(entry)`: Clone session into new JSONL file at specified entry (`crates/axo/src/session/maintenance.rs:608`).
* **Metadata & State Mutation**:
  - `Session::set_name(Option<String>)`: Persist session name entry (`crates/axo/src/session/mod.rs:1584`).
  - `Session::append_custom(type, data)`: Append branch-scoped host metadata (`crates/axo/src/session/mod.rs:1615`).
  - `Session::answer(call_id, answers)` / `dismiss(call_id)`: Resolve interactive questions (`crates/axo/src/session/mod.rs:1545,1555`).
  - `Session::kill_job(id)`: Terminate background job (`crates/axo/src/session/mod.rs:1673`).

---

## 6. Gaps for Axome Host Integration

1. **No Dedicated Push Event for Session Renaming**:
   When `session.set_name()` is called, axo emits a generic `SessionEvent::Entry(SeqEntry::SessionInfo)`. There is no `SessionEvent::NameChanged` event; the host must inspect every appended entry to update session tab titles.
2. **No Dedicated Push Event for Custom State**:
   `session.append_custom()` commits an `Entry::Custom` and broadcasts `SessionEvent::Entry`, but does not notify with a dedicated custom event.
3. **No Dynamic Configuration Reload on Open Sessions**:
   `ConfigSource` is sampled at session creation and open time. If the user edits `.axome/settings.jsonc` while a session is live, axo provides no hook or watcher seam to hot-reload `EffectiveConfig` without recreating or reopening the session.
4. **Non-Uniform Tool Output Metadata Schema**:
   `truncate.rs` dropped `lastLinePartial`, but the shape of tool details remains heterogeneous: `read` and `grep` return truncation records directly in `details`, `bash` nests `fullOutput` and execution details in `jobs/command.rs`, and `edit` supplies `diff`, `patch`, and `ledger`. Axome must maintain custom per-tool parsers in TypeScript (`payload.ts`).
5. **No Token-Progress Reporting During Compaction**:
   `SessionEvent::MaintenanceStart` and `MaintenanceEnd` signal that compaction is active, but provide no incremental progress (e.g. prompt token counts or completion chunks during summarization).
6. **Provider Auth Availability Inspection Requires Trial Calls**:
   Axome's catalog endpoint (`AxoService::catalog` in `mod.rs:295-325`) filters models by checking if credentials exist. Because `Models` does not expose a synchronous credential inspection API, Axome must asynchronously call `axo.models().auth(model).await` on every distinct provider.

---

## 7. Doc / Code Mismatches in axo

During verification against `crates/*/src` at HEAD (`6cdaeb4`), the following minor discrepancies were identified in documentation files:
1. **`Settings.extended_context`**:
   `crates/axo/docs/configuration.md:73` and `:355` document `extendedContext` as an example-level concept and `ModelsOptions::extended_context` as a host configuration parameter, but earlier documentation revisions described it as an active setting on `Settings`. In code, `extended_context` is strictly a field on `ModelsOptions` (`crates/axo-ai/src/models/mod.rs:36`) and was completely removed from `Settings` (`crates/axo/src/config.rs:40-50`).
2. **`command_log_tail` and `command_logs` Naming**:
   Older doc references in `crates/axo/docs/extending.md` referred to artifact helpers as `bash_tail` and `bash_outputs`. In code, these were generalized to `command_log_tail` and `command_logs` (`crates/axo/src/schemes/artifact.rs:150,161`) to support non-bash backgroundable commands.
3. **`doc_identifiers.rs` Coverage**:
   `crates/axo/tests/doc_identifiers.rs` verifies that all backticked identifiers in docs exist as tokens in Rust source code, ensuring that renamed types (such as `ToolHooks`, `LiveSnapshot`, `EffectiveConfig`, and `ConfigSource`) are referenced accurately across the guides.
