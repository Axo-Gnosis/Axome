# axo 843cd73: typed host surface

Raw survey report, kept verbatim as evidence. Produced by a read-only subagent on 2026-11-10 against axo `843cd73`, verified against source code (not docs). Covers events, snapshot, jobs, children, personas, catalog, host messages, and audits every point of Axome's host-surface request to axo (§8).

Citation prefixes are defined inside the report. Claims marked [INFERENCE] were not observed directly.

---

## Summary

The typed host surface is substantially present, but **“ordered host surface” does not mean the proposed timestamped event envelope shipped**. HEAD still broadcasts bare `SessionEvent`; only journal entries have a durable sequence and entry timestamp. Exact snapshot/subscription splitting, per-row job events, live command-output handles, richer child rows, persona changes, host messages, and usage rollups did ship. Process-wide lifecycle events, complete team state, and full host/model control parity did not.

**Citation prefixes below:**  
- `a/…` = `/repo/rust/axo/crates/axo/src/…`, at HEAD.  
- `agent/…` = `/repo/rust/axo/crates/axo-agent/src/…`.  
- `ai/…` = `/repo/rust/axo/crates/axo-ai/src/…`.  
- `x/…` = `/repo/rust/Axome/…`.

Research used source, `git show e070490`, `git show 0782cfc`, and diffs against `6cdaeb4`; the earlier pin’s snapshot contract was also inspected. No builds/tests or working-tree modifications.

## 1. What axo now provides

### Events, ordering, attach, and lag recovery

**There is no public event envelope, event order counter, or event timestamp.**

`Attach.events` is still `broadcast::Receiver<SessionEvent>`, not a receiver of `{ order, at, event }`. The broadcast has capacity **1024**. `SeqEntry.seq: u64` is the zero-based line position after the header; `EntryBase.timestamp: String` is ISO-8601 UTC with milliseconds, with `timestamp_ms() -> Option<i64>`. These properties do not extend to arbitrary non-entry events. (`a/session/mod.rs:299-307,1418`; `a/store/entry.rs:182-209,655-659`)

Current `SessionEvent` variants:

| Variant | Payload |
|---|---|
| `Agent` | `AgentEvent` |
| `Entry` | `SeqEntry` |
| `QuestionPending` | `call_id: String, questions: Vec<Question>` |
| `QuestionResolved` | `call_id: String` |
| `MaintenanceStart` | `kind: MaintenanceKind` |
| `MaintenanceEnd` | `kind: MaintenanceKind, error: Option<String>, aborted: bool` |
| `Retry` | `attempt: u32, delay_ms: u64` |
| `Job` | `JobInfo` — row upsert |
| `JobGone` | `id: String` |
| `Context` | `ContextUsage` |
| `AutoPaused` | `paused: bool` |
| `Mode` | `mode: Option<String>` |
| `ModeResult` | `call_id: String, mode: String, result: Value, awaiting_review: bool` |
| `ModeResultResolved` | `call_id: String` |

The last three are later HEAD additions beyond the two assigned host-surface commits, but must be covered by an exhaustive HEAD match. (`a/session/mod.rs:201-252`)

**Exact attach split:** `Session::attach(from: Option<u64>) -> Attach` takes the journal publication read lock, then live-state lock, then the registry lock around subscribing and copying **last-announced** rows. Entries with `seq > from` become `missed`; live state and the replacement receiver are returned together. It does not wait for a disk write already in flight. (`a/session/mod.rs:2322-2375`; `a/jobs/mod.rs:938-944`)

Underlying publication:
- Journal entries are published and `Entry` events sent under the journal publication lock, after their writes. (`a/session/journal.rs:134-169,254-286`)
- Streaming/tool live mutations and corresponding `Agent` events share the live mutex. (`a/session/mod.rs:581-590`)
- Changed listed rows produce `Job`; removed rows produce `JobGone`, under the registry lock. This replaces the former separate coalescing-watch relay. (`a/jobs/mod.rs:909-944`)
- Pending questions retain an explicit **“duplicated, never missed”** caveat; attach is not universal exactly-once delivery for every event category. (`a/session/mod.rs:321-324`)

**Important limitation:** there is a broadcast reception order, but no exposed order coordinate tying non-entry events to journal entries. For example, command/child end handling first changes registry state and announces it, then journals `JobEnd`, then announces the deliverable state. A host must not equate a `Job` update with a committed `JobEnd`. (`a/jobs/mod.rs:1116-1180,1868-1979`)

**Lag recovery is host-driven:** call `attach(last_entry_seq)`, append `missed`, replace derived live state, and replace the receiver. Intermediate transient events are not replayed. Axome already implements this mechanism; its rebuild now needs the new snapshot shape and fields. (`a/session/mod.rs:2322-2375`; `x/src-crates/axome-core/src/services/axo/live.rs:524-570`)

### Complete `LiveSnapshot` shape

```rust
run_id: Option<String>
maintenance: Option<MaintenanceKind>
running: bool
streaming: Option<AssistantMessage>
pending_tool_calls: Vec<ToolCall>
queued_steering: Vec<AgentMessage>
queued_follow_up: Vec<AgentMessage>
messages: Vec<AgentMessage>
pending_questions: Vec<(String, Vec<Question>)>
pending_results: Vec<PendingResult>
jobs: Vec<JobInfo>
usage: Usage
model: Model
thinking: ThinkingLevel
agent: Option<String>             // persona catalog name, not AgentSnapshot
mode: Option<String>
context: Option<ContextUsage>
auto_paused: bool
```

`usage` is recorded session spend, including maintenance and failed assistant requests, **excluding derived run/job totals**. There is no leaf/path field, retry snapshot field, quiet-message queue, mail queue, or process/tree snapshot. (`a/session/mod.rs:311-337,2357-2370`)

### Complete public job shapes

```rust
JobInfo {
    id: String,
    status: JobStatus,
    call_id: Option<String>,
    started_at: i64,
    kind: JobKind,
    messages: Vec<JobMessageInfo>,
}

JobKind::Command {
    command: String,
    tool: String,
    cwd: PathBuf,
    artifact: u64,
    promotion: Option<Promotion>,
    promoted_at: Option<i64>,
    ended_at: Option<i64>,
    exit: Option<CommandExit>,
}

JobKind::Child {
    session_id: String,
    path: PathBuf,
    depth: u32,
    agent: Option<String>,
    model: String,
    team: Option<TeamRef>,
    resident: bool,
    progress: Option<ChildProgress>,
}
```

`JobStatus` includes `Queued`, `Running`, `HelpersRunning`, `Finished`, `Failed`. `JobInfo`/`JobKind` now implement `PartialEq`, **not `Eq`**. (`a/jobs/mod.rs:134-205`)

Specific semantics and omissions:
- `started_at` is Unix milliseconds: actual command spawn time, or the child-start/admission-entry timestamp. (`a/jobs/command.rs:289-318`; `a/jobs/mod.rs:1746-1768`)
- **No `created_at`; no explicit command origin enum.** `call_id` identifies the originating tool call, and is `None` for host **child spawns**. Commands run through `SessionTools::run_command` still require a `ToolContext` and retain its call ID/tool name. (`a/jobs/mod.rs:160-194`; `a/tools/mod.rs:143-149`; `a/jobs/command.rs:257-265,312-318`)
- **`command` is the display label**, not a separate executable/argv plus label pair. `CommandRequest` has separate `spec: ExecSpec` and `label: String`, but only the label is copied into the row’s `command`. Built-in bash labels with the original command text, before URL rewriting. (`a/jobs/mod.rs:173-184`; `a/jobs/command.rs:88-94,312-318`; `a/tools/bash.rs:141-169`)
- `promotion` is `None` for a foreground command retained only because helpers remain. (`a/jobs/mod.rs:180-185`)
- Child model is expanded `provider/id`; selector is instead available in the child’s `SpawnLink.model`. (`a/jobs/mod.rs:187-196`; `a/store/entry.rs:99-105`)

```rust
Promotion::{Async, Timer, Steer, Host}

CommandExit::{
    Exited { code: i32 },
    Signaled { signal: i32 },
    TimedOut { after_secs: f64 },
    Killed { by: Killer },
    Aborted,
    Interrupted,
}

Killer::{Model, Host}
```

`CommandExit` serializes as a camel-case internally tagged object, e.g. `{"type":"killed","by":"host"}`. Shutdown uses `Interrupted`, distinct from host kill and tool-call abort; waiting on a process can also fail with `exit: None`. (`a/jobs/mod.rs:226-258`; `a/jobs/command.rs:592-607`)

`Session::kill_job(&str) -> bool` records `Killer::Host` for commands; the model’s `kill` tool uses model initiation. Killing a child aborts its active run rather than recording a command-style killer on a child row. (`a/session/mod.rs:1961-1970`; `a/jobs/mod.rs:1246-1279`)

### Job messages, deliveries, and closed-session replay

```rust
JobMessageInfo {
    id: String,                 // row or row/n message reference
    note: bool,
    is_error: bool,
    partial: bool,
    run_id: Option<String>,
    delivery: MessageDelivery,
}
MessageDelivery::{Held, Writing, Pending, Reserved, Delivered}
```

This is an ordered per-row message metadata list, but **not a complete message object**: no text, timestamp, usage, yielded value, reservation owner, or delivering-entry ID. Those durable contents must be joined from entries. `Writing` maps from private `NotDeliverable`, and `Held` represents pre-admission child output. (`a/jobs/mod.rs:208-224,290-303,390-412`)

Both:
- automatic custom `job_result` message details, and
- a successful `wait` delivering results

contain `{"jobs": [message-reference strings]}`. Committed `Entry::Message.delivered_jobs` records delivery receipts. Human-facing text is `"[row] label\ntext"` joined with blank lines, with an automatic-delivery prefix `Background job results:\n\n`. (`a/jobs/mod.rs:472-479,721-749,782-803`; `a/tools/wait.rs:86-99`)

```rust
pub fn jobs_from_entries(
    header: &Header,
    entries: &[SeqEntry],
) -> Vec<JobInfo>
```

Pure, no session open/recovery/write. It reconstructs listed rows, command exits/timestamps, message metadata and committed delivery state. Child replay has `resident: false`, `progress: None`; private reservations and live progress are not recoverable. **An unended recorded command can still appear `Running` in this read-only view**; opening performs a separate reconciliation that journals an `Interrupted` result. Thus “Running” from closed-session replay is not proof of a currently running OS process. (`a/jobs/mod.rs:333-345,489-503,527-589,601-617,702-716,809-849`)

### Live command output and host promotion

```rust
Session::command_output(&self, call_id: &str) -> Option<CommandOutput>
Session::background(&self, call_id: &str) -> bool

CommandOutput::job(&self) -> &str
CommandOutput::artifact(&self) -> u64
CommandOutput::progress(&self) -> watch::Receiver<OutputProgress>
CommandOutput::read(&self, range: Range<u64>)
    -> io::Result<Vec<u8>>

OutputProgress { bytes: u64, done: bool }
```

(`a/session/mod.rs:1972-1982`; `a/jobs/command.rs:159-239`)

Output contract:
- A file-backed raw stream with **stdout and stderr merged, without tags**; offsets are raw bytes, not chunks/lines/UTF-8 character positions. Both capture pumps send `Vec<u8>` into the same channel. (`a/jobs/command.rs:170-175`; `a/exec/host.rs:472-476,566-578`)
- Each successful spool write is flushed before increasing `bytes`; reads clamp to the published prefix and use positional file reads. Watch notifications can coalesce, but byte offsets allow catch-up without losing intervening bytes. (`a/jobs/command.rs:208-237,745-778`)
- Available during foreground execution **after registry registration**, through promotion and exit; acquiring the handle does not depend on the row being listed. (`a/jobs/command.rs:306-320`; `a/jobs/mod.rs:1522-1539`)
- An already-acquired handle owns an open file and remains readable even if a non-retained foreground spool is unlinked. A new lookup can fail after cleanup; never-promoted rows are removed when their container closes. (`a/jobs/command.rs:171-175,575-586`; `a/jobs/mod.rs:1210-1229`)
- `done` becomes true only after helper/output draining, process close, and live-artifact release—not merely root-shell exit. (`a/jobs/command.rs:687-730`)
- Reopened jobs **do not rebuild this API**: replay installs an empty output path and `{bytes:0, done:true}`, so `command_output` cannot reopen the artifact through that registry state. Historical artifacts remain separately resolvable/readable. (`a/jobs/mod.rs:557-565,1522-1539`)

`background(call_id)` is a **request**, not synchronous confirmation: it notifies the foreground command; the command’s select races completion and records `Promotion::Host` only if promotion wins. There is no API named `promote_job`. (`a/jobs/mod.rs:1541-1557`; `a/jobs/command.rs:355-379`)

### Children: queueing, residency, progress, and access

There is **no public `Session::children()`**. The host equivalent is `Session::jobs()` filtered to `JobKind::Child`, then `Axo::session(session_id)` for a child handle. The internal `children()` returns private `ChildRef` and is not exported. (`a/session/mod.rs:1954-1959`; `a/jobs/mod.rs:1771-1786`; `a/axo.rs:237-243`)

```rust
ChildProgress {
    turns: u32,
    max_turns: u32,
    wrapping_up: bool,
    yield_failures: u32,
    usage: Usage,
}
```

- Child rows begin `Queued`; a child waiting for a run permit is explicitly marked queued; `AgentStart` marks running. Queue **position, available headroom, and admission/speculative state are not public row fields**. Only admitted children are listed. (`a/session/spawn.rs:465-483`; `a/session/mod.rs:505-541,1175-1186`; `a/jobs/mod.rs:2031-2035`)
- `resident` tracks table pinning/parking, **not simply whether an `Arc<Session>` exists**: parking sets it false before dropping the table’s strong reference, while a caller may retain its own handle. (`a/session/children.rs:92-110,132-151,163-180`)
- Progress pushes to the **parent’s `Job` stream after a persisted assistant `MessageEnd`** during an open child run. It is not token-streamed or periodically sampled. Usage is accumulated in the child’s live run state; the parent update calls `child_progress` and `bump`. (`a/session/mod.rs:1211-1262`; `a/jobs/mod.rs:1810-1824`)
- `turns` counts model requests; `wrapping_up` is `turns >= max_turns`. Yield failures are recorded in finish-turn processing, while progress is sampled on assistant-message persistence; it is not an immediate failure notification. (`a/session/mod.rs:968-974,1094-1108,1244-1259`)
- Parent progress lacks latest assistant text/current tool. Full live transcript, deltas and pending calls require attaching to the child itself. `agent://` still computes latest text and total historical turns from child entries for its text rendering. (`a/session/mod.rs:311-325`; `a/schemes/agent.rs:106-134`)

### Per-run usage and typed yield values

Durable additions:
- `Entry::RunEnd.usage: Option<Usage>`, assistant-request usage for that run, excluding maintenance.
- `Entry::JobEnd.usage: Option<Usage>`, corresponding child-run usage.
- `Entry::JobEnd.value: Option<Value>`, preserved structured yield result.
- `Entry::RunEnd.queued: Vec<String>`, unconsumed submission IDs retained across reopen. (`a/store/entry.rs:382-399,461-485`)

`run_usage` sums assistant messages from the matching run start until its end/next start; resumed runs seed live usage from the journal. `Entry::usage()` intentionally excludes `RunEnd`/`JobEnd` rollups, preventing double counting when computing session spend. `Usage` includes input/output/cache counters, optional reasoning and one-hour cache-write counters, total tokens, and component/total cost. (`a/store/mod.rs:25-45`; `a/store/entry.rs:489-502`; `a/session/resume.rs:382-400`; `ai/types.rs:251-295`)

**Typed yield access:** public durable access is pattern-matching `Entry::JobEnd { value, .. }`; there is **no public `Session::yield_value`/child-result getter**. The registry’s `child_result_value` and tool helper `yielded_value` are private. `agent://…/result/<pointer>` uses the stored `Value` first, otherwise parses legacy result text. Yield’s own successful tool result remains plain `"Result accepted."` with default details, not a `details.value`. (`a/jobs/mod.rs:1729-1739`; `a/tools/output.rs:35-41,132-143`; `a/schemes/agent.rs:133-157`)

### Opening a child and process-wide state

```rust
Axo::session(&self, id: &str) -> Result<Arc<Session>, Error>
Error::ChildSession { path: PathBuf, root: Option<PathBuf> }
```

`session(id)` resolves/revives **known children**, not arbitrary roots/session IDs. For an unknown ID it waits for outstanding reopened-session child seeding to finish, then returns `SessionNotFound` if still unknown. Revival uses `RecoveryPolicy::Resume`. (`a/session/children.rs:215-247,274-313`; `a/error.rs:42-50`)

Opening a child file with `open_session` still returns `ChildSession`; root discovery follows parent IDs through the store’s cwd listing, with `root: None` if listing/ancestry fails or is cyclic. **The host must open the indicated root itself, retain it, then call `Axo::session(child_id)`**. No implicit owning-root open shipped. Read-only child-file access is available through `open_session_read`. (`a/axo.rs:178-247`; `a/store/mod.rs:69-71`)

**No process-wide Axo events or `live_sessions()` listing.** `Axo` has a private `SessionTable`; its slots cover children, while registries track open sessions. Public `list_sessions(cwd)` lists stored sessions, not live residency/lifecycle. (`a/axo.rs:31-45,250-252`; `a/session/children.rs:29-39`)

### Catalog, getters, personas, dynamic sections/tools

```rust
Axo::catalog(
    &self,
    cwd: &Path,
    project_access: Option<ProjectAccess>,
    agents: &[AgentSpec],
    modes: &[ModeSpec],
) -> Result<Catalog, Error>

Catalog {
    config: Arc<EffectiveConfig>,
    agents: Vec<AgentDef>,
    modes: Vec<Arc<ModeSpec>>,
    diagnostics: Vec<Diagnostic>,
}

Session::model(&self) -> Model
Session::thinking(&self) -> ThinkingLevel
Session::config(&self) -> &EffectiveConfig
Session::context_usage(&self) -> Option<ContextUsage>
Session::agent(&self) -> Option<AgentSpec>
Session::spawn_link(&self) -> Option<&SpawnLink>

ContextUsage { tokens: u64, window: u64, trigger: u64, reserve: u64 }
```

Catalog performs blocking definition/config I/O without creating a session. `AgentDef::{Agent(AgentSpec), Team(TeamSpec)}` includes team roster/charter definitions; model roles and subagent settings are under `catalog.config.settings`, not separate catalog vectors. (`a/axo.rs:48-54,291-318`; `a/context/agents.rs:111-150`; `a/session/mod.rs:1846-1869,2317-2319,289-296`)

**Persona persistence:**
- `agent_change` stores the root catalog name or `None`; projection tracks the latest selected name on the active path. Reopen resolves that name against definitions loaded now, **not a persisted frozen root AgentSpec**. An unavailable/ineligible recorded persona produces a diagnostic and runs without it; explicit invalid selection errors. (`a/store/entry.rs:263-269`; `a/store/project.rs:37-39,101-104`; `a/axo.rs:354-380`)
- `set_agent(Option<&str>) -> Result<(), Error>` is root-only, callable during a run, applies from the next request, journals `AgentChange` plus any model/thinking changes, and applies the persona’s model selector if present. Selecting none or a spec with no model selector leaves the current selection unchanged. It rebuilds prompt/context/tool shape and policies. (`a/session/mod.rs:2117-2196`)
- `RunAs::{Root, Subagent, Any}` is an **eligibility constraint**, not an execution mode or user/account identity. Root selection rejects `Subagent`; spawning rejects `Root`; `Any` is default. (`a/context/agents.rs:14-27,39-41`; `a/session/mod.rs:2136-2139`; `a/session/spawn.rs:282-284`)
- Children keep the resolved spec persisted in `SpawnLink.spec`, reject `set_agent`, and never write root `agent_change`. (`a/store/entry.rs:103-111`; `a/session/mod.rs:840-854,2120-2124`)
- Live forks inherit the **current** persona, model/thinking and current host sections/tool factories; a later file reopen resolves the fork independently. (`a/session/mod.rs:2382-2383,2490-2519`)

**Dynamic host mutations:**
- `set_section(name, body: Option<String>) -> Result<(), Error>`: root-only; valid section names begin lowercase ASCII and then use lowercase letters/digits/`_`/`-`; `Some` sets/replaces, `None` removes. Updates the base used for future forks/children and queues a system-prompt diff for the next request. (`a/session/mod.rs:2199-2239`; `a/context/prompt.rs:37-43`; `a/session/prompt.rs:259-297`)
- `set_tools(Vec<HostTool>) -> Result<(), Error>`: root-only, replaces **host** factories, keeps built-ins except named replacement, respects persona allowlists, partitions direct/deferred exposure, and installs/rebuilds the next-request surface. (`a/session/mod.rs:2241-2288`; `a/session/prompt.rs:120-200`)
- Neither persists reusable host runtime configuration/factories. **Hosts must provide sections/tools again on reopen.** Effective section/tool diffs that reach requests are recorded as system messages; tool schemas/deltas are journalable transcript data, not executable factory persistence. (`a/session/mod.rs:2199-2200,2241-2243`; `a/session/prompt.rs:336-399`; `agent/run.rs:1120-1185`)

### Durable host messages

```rust
HostMessage {
    custom_type: String,
    text: String,
    display: bool,
    delivery: HostDelivery,
}
HostDelivery::{Steer, FollowUp, Quiet}
Session::post(&self, message: HostMessage) -> Result<(), Error>
```

`post` bypasses skill expansion and changes neither system prompt nor tools. It first journals a `Submission` with `custom: Some(SubmittedCustom { custom_type, display })`, then queues an `AgentMessage::Custom` with text blocks and no details. On consumption that becomes a normal journaled `Message` linked by submission ID. (`a/session/mod.rs:273-287`; `a/session/delivery.rs:341-387,1203-1215` **[correction: consumption implementation is `a/session/mod.rs:1203-1215`]**)

Delivery:
- `Steer`: next turn boundary; wakes an idle session.
- `FollowUp`: when current run would end; wakes an idle session.
- `Quiet`: next boundary/run start, **never wakes a run**, and does not clear auto-pause.
- Quiet messages have their own private queue, drained after results and mail; not represented in `LiveSnapshot.queued_steering`/`queued_follow_up`. (`a/session/mod.rs:282-287`; `a/session/delivery.rs:181-201,413-437,484-501`)

Reserved custom types at HEAD are **all seven**:
`tool_catalog`, `mode_notice`, `job_result`, `turn_budget`, `yield_nudge`, `compactionSummary`, `branchSummary`. Empty custom type or blank text also errors. (`a/session/delivery.rs:25-35,344-357`)

Custom content reaches the model as user content; `display` controls host presentation, not whether the model sees it. Axome currently renders displayed custom messages as generic notices and drops type/details from that notice wire representation. (`a/session/mod.rs:957-962`; `x/src-crates/axome-core/src/services/axo/convert.rs:414-426`; `x/src-crates/axome-protocol/src/chat.rs:186-190`)

### Team state, control parity, artifacts

**Teams:** parent rows expose `TeamRef { id, member }`; child `spawn_link().team` exposes `TeamLink { id, member, name, charter, brief, roster }`, and roster entries include member session IDs. Live `TeamState`, waiting sets, mutual-wait detection and notifications remain private. `Submission.sender: Option<String>` now persists teammate sender identity, but there is no public pending-mail snapshot/count. (`a/store/entry.rs:117-157,417-420`; `a/session/team.rs:11-32,68-111`)

**Control parity:**
- Host spawn: `Session::spawn`, `spawn_team` return typed handles. (`a/session/spawn.rs:71-91,197-204`)
- Steer/follow-up: resolve a child row’s session ID through `Axo::session`, then call public `steer`/`follow_up`. No direct host `send(job_id, …)` wrapper. (`a/tools/send.rs:97-124`; `a/session/delivery.rs:329-339`)
- Teammate mail: `post_mail(sender, content)` remains `pub(crate)`.
- Child note to parent: `child_message` remains `pub(crate)`.
- No public close/dismiss-child-row API; `kill_job` cancels current work, not row history. (`a/session/delivery.rs:389-400`; `a/jobs/mod.rs:1868-1874,1246-1279`; `a/jobs/mod.rs:2031-2035`)

**Artifact reads:** `Session::schemes().read(&SchemeUrl)` now handles both virtual resources and resolved UTF-8 files, including artifacts. Non-UTF-8 content instructs the host to resolve and read the file. Public `Artifacts::{find, live, dir}` expose file paths and the live-writer path set. There is **no unified arbitrary-artifact bytes/range read method**; `CommandOutput::read` only addresses a live command handle. Closed-session `Axo::schemes` supplies file-backed schemes without opening/recovery, but not live `proc://`/`agent://`. (`a/schemes/mod.rs:243-270`; `a/schemes/artifact.rs:56-58,103-124`; `a/axo.rs:255-288`)

## 2. Breaking changes: survey baseline and Axome pin

| Axome/current assumption → break | HEAD replacement / distinction |
|---|---|
| `snapshot.agent.running`, `.pending_tool_calls`, `.streaming`; passing `&snapshot.agent` as `AgentSnapshot` (`x/…/axo/live.rs:61-74,98-103`; `x/…/axo/convert.rs:639-699`) | These fields were already flattened by `6cdaeb4`; HEAD `snapshot.agent` is now `Option<String>` persona. Use the top-level fields/queues. (`a/session/mod.rs:311-336`) |
| Exhaustive `SessionEvent` match ends at `Retry`/`Agent(_)` (`x/…/axo/live.rs:265-461`) | Vs `6cdaeb4`, `Jobs { jobs }` is replaced by `Job(JobInfo)`/`JobGone`; additionally handle `Context`, `AutoPaused`, and HEAD mode events. (`a/session/mod.rs:226-252`) |
| Exhaustive `Entry` matches in anchors, rendering, selection, stats and history omit jobs/personas/modes (`x/…/axo/convert.rs:203-224,268-283,461-480,493-555,565-632`; `x/…/axo/mod.rs:650-671`) | `AgentChange`/`ModeChange` are new since survey; `JobStart`/`ChildStart`/`JobEnd` were absent at the pin. New fields also affect explicit constructors. (`a/store/entry.rs:263-279,423-485`) |
| Submission-kind matches only `Prompt/Steer/FollowUp` (`x/…/axo/live.rs:296-300`; `x/…/axo/convert.rs:681-685`) | Handle `Quiet` and `Mail`; `Mail` predates these host commits, `Quiet` is new here. Custom/sender metadata distinguishes host instructions and teammate input. (`a/store/entry.rs:405-420,644-652`) |
| Test constructors omit new entry fields (`x/…/axo/convert.rs:777-810`) | `Submission` adds `sender/custom`; `Message` includes admission/delivery receipts; `RunEnd` adds `usage/queued`. (`a/store/entry.rs:223-251,382-420`) |
| Old `Session::agent()` meant child spawn link at `6cdaeb4` | Renamed `spawn_link()`; new `agent()` returns owned current `Option<AgentSpec>`. No such caller was found in the inspected Axome integration. (`a/session/mod.rs:1846-1857`) |
| Code matching old `JobKind` fields / requiring `Eq` | New full shapes above; `JobInfo` and `JobKind` are only `PartialEq`. (`a/jobs/mod.rs:159-196`) |
| Exhaustive `axo_error` mapping omits host/child/persona/model-role/mode errors (`x/…/axo/mod.rs:52-76`) | `Error::ChildSession` additionally now includes `root: Option<PathBuf>`; new root/subagent eligibility errors need classification. (`a/error.rs:18-60,79-81`) |
| Axome unloads an idle unattached root without considering its jobs/children (`x/…/axo/live.rs:614-620`; `x/…/axo/mod.rs:248-263`) | **[INFERENCE: adoption behavior risk]** parent UI-idle no longer implies the owner tree is idle; shutdown/drop kills command containers and shuts down children. (`a/session/mod.rs:2081-2109,2519-2537`) |
| Axome stats use latest assistant `usage.total_tokens` as context size (`x/…/axo/convert.rs:560-586`) | New authoritative estimated-context surface is `ContextUsage` plus `Context` events, not latest-response usage. (`a/session/mod.rs:663-680,2317-2319`) |

Here and below `x/…/axo/*.rs` abbreviates `x/src-crates/axome-core/src/services/axo/*.rs`.

## 3. Silent changes and persisted/rendered shapes

- **`agent_change` is root persona metadata, not a visible transcript message.** Its stored value is a catalog name, while children persist the actual resolved spec in the header. An entry stream consumer must not confuse these two persistence contracts. (`a/store/entry.rs:103-105,263-269`; `a/session/mod.rs:840-854`)
- **`run_end.queued` changes recovery behavior:** held IDs and post-end unconsumed submissions are requeued; held input does not itself wake a run; fresh post-end `Prompt`/`Mail` may. Under `Resume`, recovery-closed runs retain unconsumed input; `MarkInterrupted` drops it. (`a/recovery.rs:230-273,305-330`; `a/session/resume.rs:349-379`)
- **Job-result references identify messages, not just rows:** `"agent_N"` is message 1; later messages use `"agent_N/n"`. Notes and multiple child-run results coexist. (`a/store/entry.rs:459-476`; `a/jobs/mod.rs:208-218`)
- Bash foreground completion details now include `exit` and `durationMs`; promoted-return details instead contain `job` and `fullOutput`, without a terminal exit yet. Spawn returns `{"job":…}` or `{"team":…,"jobs":{member:row}}`; wait returns `{"jobs":[message refs]}`. A single presumed result-details schema is incorrect. (`a/jobs/command.rs:389-396,608-612`; `a/session/spawn.rs:974-1001`; `a/tools/wait.rs:86-99`)
- HEAD tool/prompt vocabulary uses **`kill` and `send`**, not writes to `proc://…/kill` or `agent://…`; command/spawn/team text explicitly teaches these verbs. Deferred tools introduce hidden `tool_catalog` messages, and modes hidden `mode_notice` messages—both reserved from host `post`. (`a/jobs/command.rs:385-386,650-661`; `a/session/spawn.rs:978-999`; `a/session/team.rs:126-166`; `a/session/surface.rs:168-190`; `a/session/delivery.rs:27-35`)
- Displayed custom messages become Axome `Notice { text }`, losing custom type and delivery/result references. Frontend turn grouping counts notices as reply content/timing, including automatic result-delivery notices. (`x/…/axo/convert.rs:414-426`; `x/src/lib/features/chat/turns.ts:88-108`)
- `AutoPaused` is live state, initially false on session construction, not a journal entry; child progress/residency and reservation states likewise are not historical event replay. (`a/session/mod.rs:1699,685-689`; `a/jobs/mod.rs:601-617,694-699`)
- No docs were used as implementation authority. The supplied requirements draft’s proposed timestamped envelope, implicit owning-root open, and separated output must **not** be read as shipped contracts; source chooses the narrower behavior described above.

## 4. Responsibility split

| axo owns | Host still supplies/decides/renders |
|---|---|
| Durable session/journal, recovery, child ownership, command supervision and promotion races. (`a/session/journal.rs:108-169`; `a/session/resume.rs:185-215`; `a/session/children.rs:274-313`; `a/jobs/command.rs:355-379`) | Root-open orchestration for child files; retaining owner sessions; application unload policy. (`a/axo.rs:178-243`; `x/…/axo/mod.rs:248-263`) |
| Typed row snapshots/upserts, output offsets, context estimates, persona/model changes. (`a/session/mod.rs:226-238,311-336,1954-1982`) | IPC serialization/wire types and UI presentation. Axome `ChatLive` currently contains no jobs, persona, mode, auto-pause, or context limits. (`x/src-crates/axome-protocol/src/chat.rs:304-318`) |
| Effective configuration resolution and definition catalogs. (`a/axo.rs:291-318`) | `ConfigSource`, project trust choice, host definitions, tool factories, and re-supplying dynamic sections/tools on reopen. (`a/config.rs:431-436`; `a/session/mod.rs:94-119,2199-2200,2241-2243`) |
| Journaled per-run totals and per-session snapshot spend without double counting. (`a/store/entry.rs:382-399,489-502`; `a/session/mod.rs:2362-2370`) | Tree-wide totals, historical joins, message-log enrichment and UI distinctions between closed replay and live state. (`a/jobs/mod.rs:489-503,601-617`) |

## 5. Host observability/control and durability map

| State/action | Push / getter / control | Durable reconstruction |
|---|---|---|
| Jobs | `Job`, `JobGone`; `jobs()`; `kill_job`, `background`. (`a/session/mod.rs:226-233,1954-1982`) | `jobs_from_entries`; raw `JobStart/ChildStart/JobEnd` plus admission/delivery receipts. Live reservations/residency/progress are not restored. (`a/jobs/mod.rs:489-503,601-617,654-699`) |
| Command bytes | Separate `CommandOutput.progress()` watch and range reads, not `SessionEvent`. (`a/jobs/command.rs:203-239`) | Retained artifact file; no reopened `CommandOutput`. (`a/jobs/mod.rs:557-565`) |
| Persona/model/thinking | Entry events; getters; snapshot; `set_agent/set_model/set_thinking`. (`a/session/mod.rs:1846-1869,2111-2196,2293-2297`) | Name and selections journaled; root definition resolved afresh, child spec in `SpawnLink`. (`a/axo.rs:354-404`; `a/store/entry.rs:103-105`) |
| Context | `Context`, `context_usage`, snapshot. (`a/session/mod.rs:234-235,663-680,2317-2319`) | Recomputed estimate from projection/current model/config, not a historical context-event log. |
| Auto-pause | `AutoPaused`, snapshot. (`a/session/mod.rs:236-239,685-689`) | Not journaled; constructed false. (`a/session/mod.rs:1699`) |
| Usage/yield | Entry events with `RunEnd.usage`, `JobEnd.usage/value`; snapshot session spend. (`a/store/entry.rs:382-399,461-485`; `a/session/mod.rs:2362-2370`) | Yes; no public convenience child-yield getter or tree rollup. |
| Host custom input | `post`, submission/message entry events. (`a/session/delivery.rs:344-387`) | Yes, including quiet/custom presentation metadata and queued-ID recovery. (`a/store/entry.rs:393-399,417-420`) |
| Teams | Child rows + `spawn_link().team`; attach each child for activity. (`a/jobs/mod.rs:187-196`; `a/store/entry.rs:110-143`) | Roster/charter/brief/sender stored; live wait/deadlock/mail-queue state private. (`a/session/team.rs:11-32`; `a/store/entry.rs:417-418`) |

## 6. Concrete desktop-host questions

1. **Does Axome require a cross-event sequence/timestamp for IPC diagnostics and causal UI ordering, or is reattach/current-state recovery sufficient despite bare events?** (`a/session/mod.rs:201-252,2322-2375`)
2. **What distinguishes “historically unfinished” from “running now” when displaying `jobs_from_entries` on a closed session?** (`a/jobs/mod.rs:333-345,489-503`)
3. **When a child link opens a closed owner tree, will Axome explicitly reopen/resume the root, or display the child read-only?** (`a/axo.rs:178-247`)
4. **What owner-tree activity prevents Axome’s existing idle/unattached unload?** (`x/…/axo/live.rs:614-620`; `a/session/mod.rs:2081-2109`)
5. **Should a persona picker disclose that selecting a persona may also change model/thinking, and that clearing it does not restore an earlier selection?** (`a/session/mod.rs:2117-2170`)
6. **Which host inputs should be visible custom notices, invisible custom instructions, or quiet messages, and where should queued quiet instructions be shown when snapshot queues omit them?** (`a/session/delivery.rs:344-387,484-501`; `a/session/mod.rs:311-336`)
7. **Does the terminal UI accept merged stdout/stderr and byte-range catch-up, and how should it present root exit versus helpers still writing?** (`a/jobs/command.rs:170-175,687-730`)
8. **Should child cards show only assistant-boundary progress, or attach each child for current tools/latest streaming text?** (`a/session/mod.rs:1244-1262,311-325`)
9. **How should Axome distinguish job-result notices, child notes, teammate mail and ordinary host notices when its current notice wire type preserves only text?** (`x/…/axo/convert.rs:414-426`; `a/store/entry.rs:417-420,459-485`)
10. **Where will Axome persist/recreate changed host sections/tools, since the journal preserves transcript effects but not reusable runtime factories/configuration?** (`a/session/mod.rs:2199-2200,2241-2243`)

## 7. Remaining host-surface gaps

- Timestamped/order-keyed event envelope and replay of transient intermediate events. (`a/session/mod.rs:201-252,299-307`)
- Process-wide live/root/child lifecycle subscription and live-session enumeration. (`a/axo.rs:31-45,241-252`; `a/session/children.rs:29-39`)
- Complete child card: selector/admission/run count/current tool/latest text; queue position/headroom. Some are recoverable via child headers/entries, not parent-row fields. (`a/jobs/mod.rs:187-205`; `a/store/entry.rs:90-111`)
- Public typed teammate-mail and note-to-parent methods; close/dismiss-row control. (`a/session/delivery.rs:389-400`; `a/jobs/mod.rs:1868-1874`)
- Team wait/deadlock/pending-mail state and tree usage rollup. (`a/session/team.rs:11-32,68-111`; `a/session/mod.rs:311-336`)
- Rich public message metadata with timestamps/delivery-entry links and direct yielded-value getters. (`a/jobs/mod.rs:208-224,1729-1739`)
- Reopened command-output handles, separated stdout/stderr, arbitrary-artifact byte/range API. (`a/jobs/mod.rs:557-565`; `a/jobs/command.rs:170-175`; `a/schemes/mod.rs:249-270`)
- Prompt/tool origin introspection, public request-preparation hook, config-key provenance, typed file-access listing, and detailed retry events. (`a/session/prompt.rs:234-246`; `a/session/mod.rs:968-988,223-225`; `a/config.rs:443-453,470-516`; `a/tools/ledger.rs:39-61`)

## 8. Complete checklist audit

| ID | Verdict from HEAD source |
|---|---|
| **A1** | **Not shipped as specified:** bare `SessionEvent`, no universal `at/order`; only `SeqEntry.seq` and entry-base timestamp. (`a/session/mod.rs:201-252,299-307`; `a/store/entry.rs:182-209,655-659`) |
| **A2** | **Partly:** per-row `Job/JobGone` directly broadcast under registry lock replace the watch relay; no public common order key, and row transitions can precede the causal durable end append. (`a/jobs/mod.rs:909-944,1116-1180`) |
| **A3** | **Partly:** snapshot gains selection/persona/context/auto-pause/usage/results, but no leaf/path, retry, quiet/mail queues or full child/team derived state. (`a/session/mod.rs:311-337`) |
| **A4** | **Not shipped:** no Axo lifecycle channel or live-session enumeration; private child table and stored `list_sessions` only. (`a/axo.rs:31-45,250-252`; `a/session/children.rs:29-39`) |
| **B1** | **Partly:** `call_id/started_at/tool/cwd/promotion/promoted_at/ended_at/exit` shipped; no creation timestamp, explicit command origin, or separate public command+label fields. (`a/jobs/mod.rs:159-196,226-258`; `a/jobs/command.rs:88-94`) |
| **B2** | **Shipped:** `JobStart.started_at/tool/cwd/promotion` and `JobEnd.exit`; promotion-time entry base is distinct from actual start. (`a/store/entry.rs:423-440,461-485`; `a/jobs/mod.rs:1014-1034`) |
| **B3** | **Shipped for live merged output:** `command_output(call_id)`, byte-progress watch and bounded range read span foreground/promotion/exit; no stream tags or reopen restoration. (`a/session/mod.rs:1972-1976`; `a/jobs/command.rs:159-239`; `a/jobs/mod.rs:557-565`) |
| **B4** | **Partly:** `JobMessageInfo` exposes message reference/note/error/partial/run/delivery, but not timestamp/text/usage/value/reservation owner/delivering entry. (`a/jobs/mod.rs:208-224`) |
| **B5** | **Shipped:** both `job_result` and `wait` details carry message refs as `{"jobs":[…]}`; committed message receipts record delivered refs. (`a/jobs/mod.rs:721-749,782-803`; `a/tools/wait.rs:86-99`) |
| **B6** | **Shipped:** pure `jobs_from_entries(&Header, &[SeqEntry]) -> Vec<JobInfo>`; unreconciled command state remains distinguishable only by host context. (`a/jobs/mod.rs:489-503,333-345`) |
| **B7** | **Shipped:** `Session::background(call_id)` requests host promotion; `kill_job` marks `Killer::Host`. (`a/session/mod.rs:1961-1982`; `a/jobs/mod.rs:1541-1557`) |
| **B8** | **Partly:** unified UTF-8 scheme read plus public artifact paths/live set; no arbitrary-artifact bytes/range method. (`a/schemes/mod.rs:249-270`; `a/schemes/artifact.rs:103-124`) |
| **C1** | **Partly:** filter `Session::jobs()` for enriched child rows; no public `children()` and no explicit admission/selector/run-count fields; selector/spec available through child spawn link. (`a/session/mod.rs:1954-1959,1846-1849`; `a/jobs/mod.rs:187-205`; `a/store/entry.rs:90-111`) |
| **C2** | **Partly:** parent `Job` updates carry turns/budget/wrap-up/yield-failures/usage after child assistant messages; current tool/latest text and immediate fine-grained progress require child attach. (`a/session/mod.rs:1244-1262`; `a/jobs/mod.rs:199-205,1810-1824`) |
| **C3** | **Partly:** explicit `JobStatus::Queued` before acquiring a slot; no queue position/headroom API. (`a/session/mod.rs:505-541`; `a/jobs/mod.rs:134-151`) |
| **C4** | **Decided otherwise:** child write-open still errors with optional root path; host opens root, then `Axo::session` waits for seeding/revives child; read-only file open works independently. (`a/axo.rs:178-247`; `a/session/children.rs:229-247`) |
| **C5** | **Shipped:** public durable `Entry::JobEnd.value: Option<Value>`; convenience registry accessor remains private. (`a/store/entry.rs:481-485`; `a/jobs/mod.rs:1729-1739`) |
| **C6** | **Partly:** host can obtain child and call `steer/follow_up`, and kill current work; teammate mail/parent notes remain private and rows have no dismissal API. (`a/session/delivery.rs:329-339,389-400`; `a/jobs/mod.rs:1246-1279,1868-1874`) |
| **C7** | **Partly:** parent `TeamRef`, child `TeamLink` roster/charter/brief and durable submission sender; live team waits/deadlock/mail backlog not public. (`a/store/entry.rs:117-157,417-420`; `a/session/team.rs:11-32`) |
| **C8** | **Partly:** durable per-run `RunEnd.usage` and child `JobEnd.usage`, session snapshot spend; no tree rollup API. (`a/store/entry.rs:389-391,481-482`; `a/session/mod.rs:327-336,2362-2370`) |
| **D1** | **Shipped:** `agent_change` + projection/reopen lookup; accessor named `Session::agent() -> Option<AgentSpec>`, child identity via `spawn_link()`. (`a/store/entry.rs:263-269`; `a/axo.rs:354-380`; `a/session/mod.rs:1846-1857`) |
| **D2** | **Shipped:** root-only journaled `set_agent(Option<&str>)`, next-request prompt/tools plus optional model application. (`a/session/mod.rs:2117-2196`) |
| **D3** | **Shipped:** `model() -> Model`, `thinking() -> ThinkingLevel`, both in snapshot. (`a/session/mod.rs:330-331,1860-1865`) |
| **D4** | **Shipped:** sessionless `Axo::catalog(cwd, access, agents, modes)` with agents/teams/modes/config/diagnostics; roles/subagent settings through config. (`a/axo.rs:48-54,291-318`; `a/context/agents.rs:143-150`) |
| **E1** | **Shipped:** `ContextUsage`, getter/snapshot/`Context` events; estimate and effective window/trigger/reserve. (`a/session/mod.rs:289-296,663-680,2317-2319`) |
| **E2** | **Shipped:** `LiveSnapshot.usage` sums `Entry::usage()` for session cumulative spend; no separate usage getter. (`a/session/mod.rs:327-328,2362-2370`; `a/store/entry.rs:489-502`) |
| **E3** | **Partly:** journaled system sections/tool schemas can be replayed, but effective prompt state is private and no complete typed origin/size inventory exists. (`a/session/prompt.rs:234-246,336-399`; `agent/run.rs:1120-1185`) |
| **E4** | **Shipped:** `Session::config() -> &EffectiveConfig`. (`a/session/mod.rs:1868-1869`) |
| **E5** | **Shipped live:** snapshot `auto_paused` and `AutoPaused { paused }`; not persisted across construction/reopen. (`a/session/mod.rs:336,685-689,1699`) |
| **E6** | **Partly:** `project(entries, …).path()` reconstructs the path, but snapshot has no cursor and projection leaf remains private. (`a/store/project.rs:31-65`; `a/session/mod.rs:311-337`) |
| **E7** | **Partly:** successful tool results persist `details.ledger` with path/hash/access, but no typed session-wide read/modified listing; underlying records/decoder are private. (`a/tools/ledger.rs:39-61,69-83`; `a/tools/mod.rs:57-63`) |
| **E8** | **Not shipped:** `EffectiveConfig` contains merged values/roots, not key provenance; merge discards source-by-key mapping. (`a/config.rs:443-453,470-516`) |
| **F1** | **Not shipped:** request-preparation hook is internal `SessionHooks`; host option seams are tool/spawn policies, not `LoopHooks::prepare_request`. (`a/session/mod.rs:87-122,968-988`; `a/session/policy.rs:25-46`) |
| **F2** | **Partly:** `set_section`/`set_tools` dynamically update the next-request surface and transcript diffs, but reusable settings/factories must be supplied again on reopen. (`a/session/mod.rs:2199-2288`; `agent/run.rs:1120-1185`) |
| **F3** | **Partly:** bash exit/duration, spawn IDs and wait refs now have JSON details; successful child yield still has default/no details rather than value. (`a/jobs/command.rs:608-612`; `a/session/spawn.rs:974-1001`; `a/tools/wait.rs:86-99`; `a/tools/output.rs:132-143`) |
| **F4** | **Not shipped as requested:** `Retry` remains attempt/delay only; transport retries remain tracing output, not host retry events with class/message. (`a/session/mod.rs:223-225,1060-1064`; `ai/api/http.rs:41-88`) |
| **F5** | **Shipped for UTF-8:** `Schemes::read` now reads resolved files as well as virtual resources; binary/range handling remains resolve-plus-host-I/O. (`a/schemes/mod.rs:249-270`) |

### Five open decisions

| Decision | Verdict from HEAD source |
|---|---|
| **1. Background stdin / detach** | **Not shipped / left out:** normal command stdin remains null, and session shutdown/drop stops supervised command work; no detach control appears on `Session`. (`a/exec/host.rs:238-243`; `a/session/mod.rs:2081-2109,2519-2537`) |
| **2. stdout/stderr separation** | **Decided otherwise:** one untagged merged raw byte stream; `capture_output` forwards both pipes into the same `Vec<u8>` channel. (`a/jobs/command.rs:170-175`; `a/exec/host.rs:472-476,566-578`) |
| **3. Child whose root is closed** | **Decided otherwise:** no implicit owner-tree opening; `ChildSession { root }` tells the host what to open, followed by `Axo::session`. (`a/axo.rs:178-243`; `a/error.rs:45-50`) |
| **4. Persona switching semantics** | **Decided:** may be invoked while running, takes effect from next request, applies the persona model selector when present; no selector/clearing persona preserves current selection; children cannot switch. (`a/session/mod.rs:2117-2196`) |
| **5. Job-event shape** | **Shipped proposed row form:** `Job(JobInfo)` upserts + `JobGone { id }` + snapshot rows; no timestamp/order envelope. (`a/session/mod.rs:226-233,325-326`; `a/jobs/mod.rs:909-944`) |
