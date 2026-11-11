# axo 6cdaeb4: supervision and background jobs

Raw survey report, kept verbatim as evidence. Produced by a read-only subagent on 2026-11-09 against axo `6cdaeb4`, verified against source code (not docs). Superseded for job shapes and events by `../axo-843cd73/host-surface.md`; still useful for supervision, promotion and delivery mechanics.

Citation prefixes are defined inside the report. Claims marked [INFERENCE] were not observed directly.

---

# Survey: Command Supervision + Background Jobs (`axo` vs `Axome`)

This audit covers command supervision, background jobs, process containment, and the generalized job registry across commits `74e5128`, `d789775`, `c972f19`, `c430d30`, and `ce1feb2`. Every claim is verified against source code at HEAD (`/repo/rust/axo` at `6cdaeb4` and `/repo/rust/Axome` at HEAD).

---

## 1. What `axo` Now Provides

### Process Containment and Command Supervision
- **`axo::Supervision`** (`crates/axo/src/exec/host.rs:69-87`): Host-selected process container strategy:
  - `Supervision::ProcessGroup`: Default (`host.rs:75`). Spawns commands in their own process group (`process_group(0)` at `host.rs:211`). A best-effort `sh` guard (`sh -c 'read _; kill -KILL -- -"$0"' <pgid>`, piped stdin from axo at `host.rs:219-236`) kills the process group if axo or the host dies. If the guard fails to spawn, a warning is logged and the command still runs (`host.rs:232-234`).
  - `Supervision::Cgroup { program: PathBuf, args: Vec<OsString>, cgroup_root: Option<PathBuf> }` (`host.rs:81-86`): Spawns each command in a dedicated cgroup v2 job directory (`/sys/fs/cgroup/.../axo-job-<uuid>` at `host.rs:253`). Watched by a supervisor process invoking `program args... guard <dir>` (`host.rs:260-272`). If axo dies or the guardian pipe closes, the guardian terminates and removes the cgroup (`crates/axo/src/exec/supervisor.rs:52-118`). Spawns commands via `program args... enter <dir> <command...>`, passing the environment over stdin as `KEY=VALUE\0` bytes so `LD_PRELOAD` or other variables cannot execute outside the cgroup (`supervisor.rs:141-177`).
- **`axo::exec::supervisor::run`** (`crates/axo/src/exec/supervisor.rs:40-50`): Public entrypoint for the embedded supervisor (`guard <job-dir>` or `enter <job-dir> <program> [args...]`). Synchronous `std` + `rustix` only (no tokio, no threads, minimal overhead).
- **`axo::ProcessSupervisor`** (`crates/axo/src/exec/host.rs:98-175`):
  - `ProcessSupervisor::new(supervision: Supervision) -> Arc<Self>` (`host.rs:107`): Checks nothing on construction; hosts that never run commands work even without cgroup delegation.
  - `spawn(&self, spec: ExecSpec) -> Result<Process, String>` (`host.rs:114-131`): Checks/probes cgroup root on first use if cgroup supervision is configured (`host.rs:182-216`), or spawns process group.
  - `output(&self, spec: ExecSpec, deadline: Option<Instant>) -> Result<Output, OutputError>` (`host.rs:136-160`): Spawns, collects stdout/stderr, terminates container, and waits for container to empty.
- **`axo::Container`** (`crates/axo/src/exec/host.rs:596-663`): Handle to the command's container (`Scope::Group(Pid)` or `Scope::Cgroup(PathBuf)`).
  - `kill(&self)`: SIGKILL to process group or writes `1` to `cgroup.kill` (`host.rs:604-613`).
  - `populated(&self) -> bool`: Checks if processes remain (`host.rs:617-622`).
  - `terminate(&self)`: Sends SIGTERM to group or to each cgroup pid via `pidfd_send_signal`, waits up to `KILL_GRACE` (1 second, `host.rs:32`), then escalates to SIGKILL (`host.rs:628-662`).

### Embedded Supervisor Binary & Packaging Mechanics
- **No separate binary target**: `crates/axo/Cargo.toml` has **no** `[[bin]]` section and **no** `build.rs`.
- **How it is located/executed**: The embedding host (or any helper binary) provides its own executable path as `program` and an arbitrary CLI route argument (e.g., `vec!["axo-supervisor".into()]`) in `Supervision::Cgroup` (`crates/axo/tests/supervisor.rs:48-54`). At the top of `main()`, before starting GUI, tokio, or threads, the host checks for this route argument and delegates directly to `axo::exec::supervisor::run(args)` (`exec/supervisor.rs:40-50`).
- **Packaging requirements**:
  - `Supervision::ProcessGroup`: Requires **zero** binaries or host CLI routing. Relies solely on system `/bin/sh` for the group death guard.
  - `Supervision::Cgroup`: Host must either route CLI args in its own binary or ship a thin binary wrapper. It also requires Linux 5.14+ with cgroup v2 enabled and a delegated cgroup subtree (`systemd: Delegate=yes` or write permission to `cgroup.procs` and `cgroup.subtree_control`).

### Bash Tool & Configuration
- **`axo::BashSettings`** (`crates/axo/src/config.rs:334-360`):
  - `promote_after_ms: u64` (`config.rs:344`, default `60_000` ms / 60 s): Timeout after which an active foreground command auto-promotes to a background job. Set to `0` to disable timed auto-promotion.
  - `ci: bool` (`config.rs:342`, default `true`): Injects `CI=true` into the execution environment.
  - `dev_shell: DevShellMode` (`config.rs:335`, default `Off`).
- **`BashArgs` schema** (`crates/axo/src/tools/bash.rs:24-32`):
  - `command: String` (required)
  - `timeout: Option<f64>` (optional seconds, capped at `2_147_483.647`s)
  - `async: Option<bool>` (serde alias for `run_async`)
- **Auto-Promotion Mechanics** (`crates/axo/src/jobs/command.rs:188-251`):
  A command promoted to a background job moves to `bg_N` under three conditions:
  1. `run_async == Some(true)`: Immediately promoted on launch (`command.rs:197`).
  2. `steer.pending()`: User submitted a steer or prompt message while the command was running in the foreground (`command.rs:199`).
  3. `Instant::now() >= promote_at`: `promote_after_ms` elapsed (`command.rs:200`).
  - On promotion, the tool execution ends with `ToolOutput` containing the partial output snapshot (`command.rs:205-217`) and `details: Some(json!({ "job": "bg_N", "fullOutput": "artifact://<id>" }))` (`command.rs:218-221`). The job continues running detached from the tool call's cancellation token (`command.rs:203`).

### Job Registry & Types
- **`axo::JobRegistry`** (`crates/axo/src/jobs/mod.rs:314-350`): Manages jobs across two namespaces (`Namespace::Bg` for commands and `Namespace::Agent` for subagents, `jobs/mod.rs:37-67`).
- **`axo::JobInfo`** (`crates/axo/src/jobs/mod.rs:152-157`):
  ```rust
  pub struct JobInfo {
      pub id: String, // "bg_N" or "agent_N"
      pub status: JobStatus,
      pub kind: JobKind,
  }
  ```
- **`axo::JobStatus`** (`crates/axo/src/jobs/mod.rs:130-149`):
  - `Running` (label `"running"`)
  - `HelpersRunning` (label `"helpers running"` — root shell exited, but spawned subprocesses remain active in container)
  - `Finished` (label `"finished"`)
  - `Failed` (label `"failed"` — non-zero exit code, killed, timed out, or interrupted)
- **`axo::JobKind`** (`crates/axo/src/jobs/mod.rs:159-173`):
  - `Command { command: String, artifact: u64 }`
  - `Child { session_id: String, agent: Option<String>, model: String, team: Option<TeamRef> }`
- **Host Session Methods** (`crates/axo/src/session/mod.rs:1669-1676`):
  - `Session::jobs(&self) -> Vec<JobInfo>`: Returns all jobs ascending (`bg_1..bg_N` followed by `agent_1..agent_N`).
  - `Session::kill_job(&self, id: &str) -> bool`: Stops command or child job; returns `false` if unknown or already ended.

### Virtual Schemes & Tools
- **`proc://` Scheme** (`crates/axo/src/schemes/proc.rs:1-148`):
  - `read proc://`: Lists jobs formatted as `<id>  <status>  <headline>` (`schemes/proc.rs:56-82`).
  - `read proc://<id>`: Detailed view (`schemes/proc.rs:83-138`). For commands: `Job:`, `Command:`, `Status:`, `Output: artifact://<n>`, and `Result:` if finished.
  - `write proc://<id>/kill` (empty content): Terminates the job (`schemes/proc.rs:139-147`).
  - **No `streams` path**: `proc://` strictly supports `proc://`, `proc://<id>`, and `proc://<id>/kill`. Any other path returns a `ToolError` (`schemes/proc.rs:36-40`).
- **`wait` Tool** (`crates/axo/src/tools/wait.rs:1-87`):
  - Takes no arguments (`wait.rs:27`).
  - If no jobs are running, returns immediately: `"No background jobs are running."` (`wait.rs:68-71`).
  - Blocks until a background job or subagent delivers results, or mail arrives, or a user steer interrupts it (`wait.rs:54-85`).
  - Renders pending results formatted as `[{id}] {label}\n{text}` (`crates/axo/src/jobs/mod.rs:98-105`).
- **`SessionTools::run_command`** (`crates/axo/src/tools/mod.rs:102-108`):
  Host tools created via `ToolFactory` can call `tools.run_command(cx, CommandRequest { spec, label, timeout, run_async })`. It executes with exact bash supervision, output shaping, spooling, `bg_N` promotion, and background delivery. Host tools also have direct access to `tools.supervisor` (`crates/axo/src/tools/mod.rs:47`).

### Fork-Safe Spools & Artifacts
- **Artifact tracking** (`crates/axo/src/schemes/artifact.rs:18-95`): Spool files are allocated as `<session_dir>/artifacts/<n>.<tool>.<escaped-call-id>.log`. Active spools hold a `LiveArtifact` guard which registers the path in `Artifacts::live` (`schemes/artifact.rs:25-41`, `73-95`).
- **Fork snapshotting** (`crates/axo/src/session/side_files.rs:62-95`): When forking a session:
  - Inactive artifacts are hard-linked (`fs::hard_link` at `side_files.rs:90`).
  - Active spools in `live` are **copied** (`fs::copy` at `side_files.rs:77`) and made read-only on the child. The parent's live file is never modified or chmodded by the fork.

### Journal Entries & Delivery Receipts
- **`Entry::JobStart`** (`crates/axo/src/store/entry.rs:361-368`, JSON type `job_start`): Appended when a command is promoted or retains helpers. Carries `session_id`, `job_id`, `call_id`, `command`, `artifact`. Never projected into LLM context.
- **`Entry::JobEnd`** (`crates/axo/src/store/entry.rs:395-408`, JSON type `job_end`): Appended when a background job completes or is interrupted. Carries `job_id`, `text`, `is_error`, `partial`, `run_id`, `note`.
- **`Entry::ChildStart`** (`crates/axo/src/store/entry.rs:369-386`, JSON type `child_start`): Appended on child admission.
- **`Entry::Message` Delivery Receipts** (`crates/axo/src/store/entry.rs:194-200`):
  - `admitted_jobs: Vec<String>` (wire `admittedJobs`): Identifies jobs whose admission was committed with this message's tool result.
  - `delivered_jobs: Vec<String>` (wire `deliveredJobs`): Identifies jobs whose results were delivered to context by this message.
  - `submission_id: Option<String>` (wire `submissionId`).

---

## 2. Breaking Changes Hitting Axome's Code

| Axome File & Line | Breaking Change in `axo` | Required Replacement in Axome |
| :--- | :--- | :--- |
| `src-crates/axome-core/src/services/axo/mod.rs:137` | `Axo::builder()` now requires parameters: `pub fn builder(models: Arc<Models>, store: Arc<dyn SessionStore>) -> AxoBuilder` (`crates/axo/src/axo.rs:131`). Old chainable `.dir()`, `.settings()`, `.models()` are gone. | Pass `models` and `store` into `Axo::builder(models, store)`, then chain `.config(config)`, `.cache_dir(dir)`, `.supervision(supervision)`. |
| `src-crates/axome-core/src/services/axo/live.rs:65-75` | `LiveSnapshot` was decompleted: the inner `snapshot.agent` field was flattened. Fields are now `snapshot.running`, `snapshot.streaming`, `snapshot.pending_tool_calls`, `snapshot.queued_steering`, and new `snapshot.jobs: Vec<JobInfo>` (`crates/axo/src/session/mod.rs:251-267`). | Access fields directly on `snapshot` (e.g. `snapshot.running`, `snapshot.pending_tool_calls`), and map `snapshot.jobs` into Axome's live state. |
| `src-crates/axome-core/src/services/axo/live.rs:460` | `match event` on `SessionEvent` is exhaustive without a catch-all. `axo` added `SessionEvent::Jobs { jobs: Vec<JobInfo> }` (`crates/axo/src/session/mod.rs:222-224`). Fails compilation (`non-exhaustive patterns: SessionEvent::Jobs { .. } not covered`). | Add arm `SessionEvent::Jobs { jobs } => { /* update live state & broadcast */ }` in `live.rs:460`. |
| `src-crates/axome-core/src/services/axo/convert.rs:202-224`, `268-283`, `461-476`, `508-524`, `539-553`, `568-584`, `609-624`, `646-662` | `Entry` enum added 3 new variants: `Entry::JobStart { .. }`, `Entry::ChildStart { .. }`, `Entry::JobEnd { .. }` (`crates/axo/src/store/entry.rs:361-408`). `convert.rs` matches every variant explicitly across 8 functions. Fails compilation with pattern matching errors. | Add `Entry::JobStart { .. } \| Entry::ChildStart { .. } \| Entry::JobEnd { .. }` to the ignored/skip branches of `branch_root`, `moved_blocks`, `entry`, `title`, `selection`, `stats`, and `queue_at_attach`. |
| `src-crates/axome-core/src/services/axo/convert.rs:777`, `900` | `Entry::Message` has two new mandatory fields: `admitted_jobs: Vec<String>` and `delivered_jobs: Vec<String>` (`crates/axo/src/store/entry.rs:194-200`). Direct test constructions without these fields fail compilation. | Add `admitted_jobs: Vec::new(), delivered_jobs: Vec::new()` to `Entry::Message` constructions. |
| `src-crates/axome-protocol/src/chat.rs:309-322` | Protocol `ChatLive` struct lacks background job representation (`jobs: Vec<ChatJobInfo>`). | Extend `ChatLive` and frontend TypeScript wire models with `jobs: ChatJobInfo[]`. |

---

## 3. Silent Behavior & Format Changes

1. **Background Job Delivery via Custom Messages**:
   - When a background job finishes while the session is idle, `axo` wakes the session and delivers the result as an `AgentMessage::Custom` with `custom_type: "job_result"` and `display: true` (`crates/axo/src/jobs/mod.rs:430-438`).
   - In Axome's current `convert.rs:414-419`, any `AgentMessage::Custom` with `display: true` is converted to `ChatEntryBody::Notice { content, details }`.
   - In frontend `AgentBlock.svelte:121-122`, `ChatEntryBody::Notice` is rendered as an unstyled, muted static text block: `<div class="text-sm whitespace-pre-wrap text-muted">{step.text}</div>`.
   - **Silent mis-render**: The frontend renders what looks like an informational system notice rather than a completed background command output with artifact linkage or status pills.
2. **Bash Auto-Promotion Formatting**:
   - When a bash command auto-promotes after `promote_after_ms` (60s) or on user steer, `axo` returns `ToolOutput` with `is_error: false` and text: `"Command is still running in the background as bg_N. Full output: artifact://<n>..."` (`crates/axo/src/jobs/command.rs:205-217`).
   - Axome's `bashOutput.ts:13-33` parses bash output footers. It regex-matches exit codes only when `is_error: true`, and matches truncation notices only against the specific text `[Showing lines 1-X of Y... Full output: ...]`.
   - **Silent mis-render**: Axome does not recognize the promotion text or the `details.job` field (`"bg_N"`). It renders the promotion disclaimer as if it were the command's final output, without indicating that the command is actively running.
3. **Command Kill Status Notice**:
   - When a command is stopped via `write proc://bg_N/kill`, `axo` appends `"Command was stopped (proc://{id}/kill)"` (`crates/axo/src/jobs/command.rs:450`).
   - Axome's `bashOutput.ts:18` error footer regex matches `Command exited with code ...`, `Command timed out after ...`, `Command aborted`, and `Command terminated without an exit code`.
   - **Silent mis-render**: It fails to recognize `"Command was stopped"`, so the termination notice is treated as standard terminal output rather than a status pill.
4. **Retained Helpers Status**:
   - If a command spawns detached background helpers, `axo` succeeds with `is_error: false` and appends `"\n\nProcesses started by this command are still running as bg_N..."` (`crates/axo/src/jobs/command.rs:488-498`).
   - Axome has no parser or UI component for `HelpersRunning` and renders this as plain output text.
5. **Stored JSONL Entries**:
   - Session JSONL files now contain `job_start`, `job_end`, and `child_start` entries, and `message` entries now include `admittedJobs` and `deliveredJobs` arrays (`crates/axo/src/store/entry.rs:194-200, 361-408`).
   - Older sessions read cleanly (serde defaults), but external session visualizers or parsers ignoring new entries will encounter unexpected lines.

---

## 4. Responsibility Split

| Capability | `axo` Ownership | Axome (Host) Ownership |
| :--- | :--- | :--- |
| **Command Containment** | Implements process group management with `sh` death guard (`exec/host.rs:219`), cgroup v2 directory lifecycle, process movement, and pidfd-based kill escalation (`exec/host.rs:628`, `exec/supervisor.rs:52`). | Selects `Supervision::ProcessGroup` or `Supervision::Cgroup` via `AxoBuilder::supervision` (`crates/axo/src/axo.rs:104`). If `Cgroup` is chosen, host routes its CLI prefix in `main()` to `axo::exec::supervisor::run` (`exec/supervisor.rs:40`). |
| **Supervisor Binary** | None. axo provides the sync library function `axo::exec::supervisor::run` (`exec/supervisor.rs:40`). | Host uses its own Tauri executable or packages a standalone helper binary if cgroup containment is desired. Zero packaging needed for default `ProcessGroup`. |
| **Execution Environment** | Non-interactive environment variables (`PAGER=cat`, `CI=true`, etc. at `jobs/command.rs:37-75`), path search (`host.rs:36`), and dev-shell capture runners. | Provides `Arc<dyn ExecEnv>` (e.g. `StaticEnv::host()` or `DevShellEnv`) via `SessionOptions::exec_env`. |
| **Background Scheduling** | Promotion timer (`promote_after_ms`), steer interruption detection, detached job execution, spool writing, and exit draining (`jobs/command.rs:188-251`). | Configures `BashSettings.promote_after_ms` in `ConfigSource` / `UserConfig` (`crates/axo/src/config.rs:344`). |
| **Job State Tracking** | `JobRegistry` (`jobs/mod.rs:314`), `bg_N` and `agent_N` ID allocation, state machine (`Running`, `HelpersRunning`, `Finished`, `Failed`), `proc://` virtual filesystem. | Stores and tracks `JobInfo` in live relay state (`live.rs`), maps to frontend protocol, and displays job status badges/drawers. |
| **Delivery to Transcript** | Idle session wake, prompt prepending, `wait` tool reservation, `deliveredJobs` receipts, and interrupted job reconciliation on session open (`session/delivery.rs:85-98`, `jobs/mod.rs:448`). | Renders delivered custom messages (`ChatEntryBody`) distinctly from generic warnings/notices in chat transcripts. |
| **Job Control** | `Session::kill_job` (`session/mod.rs:1674`) and `write proc://<id>/kill` (`schemes/proc.rs:141`). | Exposes "Stop Job" buttons in UI and invokes `Session::kill_job(id)`. |

---

## 5. Host Observability

### Push Events
- **`SessionEvent::Jobs { jobs: Vec<JobInfo> }`** (`crates/axo/src/session/mod.rs:222-224`): Broadcast over `session.events()` whenever a job is registered, promoted, changes phase, retains helpers, or finishes (`crates/axo/src/session/delivery.rs:379-392`). Provides the complete updated `Vec<JobInfo>`.
- **`SessionEvent::Entry(SeqEntry)`** (`crates/axo/src/session/mod.rs:193`): Broadcast whenever `Entry::JobStart` or `Entry::JobEnd` is written to the journal.
- **`AgentEvent::ToolExecutionUpdate { call_id, update }`** (`crates/axo-agent/src/event.rs:43-46`): Forwarded as `SessionEvent::Agent(...)` during **foreground** command execution. Emitted at most once every 100 ms (`UPDATE_THROTTLE` at `crates/axo/src/jobs/command.rs:34`). **Stops immediately once the command promotes to a background job.**

### Snapshot / Polling APIs
- **Attach Snapshot**: `LiveSnapshot.jobs: Vec<JobInfo>` (`crates/axo/src/session/mod.rs:265`) carries the complete job list upon attaching to a session.
- **On-Demand Inspection**: `Session::jobs(&self) -> Vec<JobInfo>` (`crates/axo/src/session/mod.rs:1669`).
- **Individual Job Status**: `Session::schemes().read("proc://bg_N")` (`crates/axo/src/schemes/proc.rs:83-138`) returns current status, command string, artifact URL, and undelivered output.

### Control Operations
- **`Session::kill_job(&self, id: &str) -> bool`** (`crates/axo/src/session/mod.rs:1674`): Sends SIGTERM, waits up to 1 second (`KILL_GRACE`), then sends SIGKILL. Returns `true` if the job was active.
- **`Session::schemes().write("proc://bg_N/kill", "")`** (`crates/axo/src/schemes/proc.rs:139-147`): Virtual scheme equivalent of `kill_job`.

### Can Job Output Be Streamed Live to a Host?
- **Foreground Phase**: **Yes.** Incremental output text is pushed via `AgentEvent::ToolExecutionUpdate` (`jobs/command.rs:238-248`), which Axome currently forwards to frontend terminals as `ChatDelta::ToolOutput` (`live.rs:412-423`).
- **Background Phase**: **No push stream exists in `axo`.** Once promoted, `CommandRunner` drops the tool context and updates channel. The background task appends raw bytes to `<session_dir>/artifacts/<n>.<tool>.<call_id>.log` (`jobs/command.rs:388-392`). `axo` emits state changes (`SessionEvent::Jobs`), but does not emit chunk deltas.
- **To observe background output**: The host must either:
  1. Open and follow/tail the artifact file on disk using `session.schemes().artifacts().find(artifact_id)` (`crates/axo/src/schemes/artifact.rs:98-111`).
  2. Poll `read proc://bg_N` (which returns output only after completion).

---

## 6. Gaps for a Complete Host UI

1. **No Live Streaming Seam for Background Job Output**:
   - `CommandTask` writes chunks to an internal `tokio::fs::File` spool and increments an internal `watch::Sender<u64>` (`crates/axo/src/jobs/command.rs:388-392`).
   - *Missing seam*: Neither `JobRegistry`, `Session`, nor `SessionEvent` exposes a chunk subscription or broadcast channel for background job output. A host UI wanting a live terminal tail for background jobs must bypass `axo` and poll/inotify the raw spool file on disk.
2. **Untyped Delivery in Transcript**:
   - Job delivery arrives as an `AgentMessage::Custom` with text `format!("Background job results:\n\n{}", render_results(&results))` (`crates/axo/src/jobs/mod.rs:432-434`).
   - *Missing seam*: The delivery message wraps pre-rendered human text (`[bg_1] cargo check\n...`). There is no structured, machine-readable payload on `Entry::Message` separating stdout, stderr, exit code, command label, and artifact ID. Hosts must parse string prefixes to extract per-job details.
3. **No Direct Progress / Resource Metrics on `JobInfo`**:
   - `JobInfo` contains only `id`, `status`, and `kind` (`crates/axo/src/jobs/mod.rs:152-157`).
   - *Missing seam*: It does not expose start timestamp, running duration, PID, memory usage, CPU usage, or bytes written to spool. A host dashboard cannot display job execution time or resource consumption without external OS lookups.
4. **No Granular Sub-Process Inspection for `HelpersRunning`**:
   - When a job is in `JobStatus::HelpersRunning`, `Container::populated()` confirms leftover processes remain, but `axo` does not expose their PIDs, process names, or tree hierarchy.
   - *Missing seam*: Host UI cannot show *which* specific daemon or background process is keeping `bg_N` alive.
5. **No Output Tail or Seek via `proc://`**:
   - `proc://bg_N` returns metadata while running, and metadata plus full result once finished (`crates/axo/src/schemes/proc.rs:83-138`). It does not support query parameters or subpaths (e.g. `proc://bg_N?tail=100` or `proc://bg_N/log`).

---

## 7. Documentation Verification Notes

All claims in `crates/axo/docs/` regarding supervision and background jobs were cross-referenced with `crates/*/src` at `6cdaeb4`:
- `docs/configuration.md:216-292` accurately matches `crates/axo/src/config.rs:334-360` and `crates/axo/src/exec/host.rs:69-290`.
- `docs/extending.md:143-240` accurately matches `crates/axo/src/session/mod.rs:1669-1676` and `crates/axo/src/jobs/mod.rs:130-173`.
- `docs/prompts-and-context.md:268-306` accurately matches `crates/axo/src/tools/bash.rs:24-184` and `crates/axo/src/schemes/proc.rs:1-148`.
- `docs/sessions-and-compaction.md:102-110` accurately matches `crates/axo/src/jobs/mod.rs:410-530` and `crates/axo/src/session/delivery.rs:85-150`.
- *Observation*: Documentation in this area is up to date with code at HEAD.
