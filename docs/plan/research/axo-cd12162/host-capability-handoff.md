# axo `cd12162`: host capability handoff

Survey of axo commit `cd121629` ("feat: expand host session and command output APIs", 2026-11-13) against its parent `97513d6`, read from source. Paths are under `/repo/rust/axo/crates/axo/src/`. It answers: what did axo add for Axome, and which Axome phase uses each piece.

## 1. Session activity (closes the Phase 1 idle-signal gap)

- `SessionEvent::Activity(Activity { running, maintenance })`; `Activity` is now public (`session/delivery.rs:50-55`).
- Published by `ActivityState::send_if_modified` under the watch write lock (`delivery.rs:56-95`), so attach never misses a transition.
- `release_or_deliver` drains queued input and starts job-result delivery *inside* that lock (`delivery.rs:183-219`); `running = false` is published only when nothing took over. This is the admission barrier `RunEnd` never was.
- `running` covers runs, standalone maintenance (`maintenance.rs:62-73` sets `running |= standalone`) and navigation; in-run compaction keeps `running` and adds `maintenance`.
- `Session::is_running()` reads it (`session/mod.rs:2571-2575`); `LiveSnapshot.running/maintenance` come from the same state.

**Axome:** adopted in Phase 2 (`services/axo/live.rs` drives `ChatLive.running/maintenance` from `Activity`).

## 2. Typed coordination receipts

`coordination.rs` defines the shapes tools write into `details`, and host methods return:

| Type | Shape | Producer |
|---|---|---|
| `SpawnReceipt` | untagged `{job}` or `{team, jobs: {member: agent_N}}` | `spawn` details; `ChildHandle::receipt()`, `TeamHandle::receipt()` |
| `WaitReceipt` | `{reason: results \| mail \| interrupted \| idle, jobs?}` | every successful `wait` (`tools/wait.rs`) |
| `KillReceipt` | `{id, stopping}` | `kill` details; `Session::kill_job` now returns it |
| `SendReceipt` | `{to, kind: note \| steer \| followUp \| mail, sessionId, receipt: {type: submission \| message, id}}` | `send` details; `Session::send` |
| `SubmissionProvenance` | `{sessionId, route, name?}` | recorded on `Entry::Submission.provenance` for routed steer, follow-up and mail |

- `Session::send(to, message, steer)` (`session/coordination.rs:11-30`) routes exactly like the model's `send` (owned child id, teammate name, `parent`), and returns the receipt.

**Axome:** Phase 2 renders `spawn`/`wait`/`kill`/`send` receipts through typed parsers (`src/lib/features/chat/elements/tool/receipt.ts`). Host `send` and provenance are Phase 3 material.

## 3. Prompt, context-file and ledger inspection

All read-only, no rebuild and no disk I/O (`session/prompt.rs:398-482`):

- `inspect_prompt() -> PromptInspection { latest_built, recorded }`: section name → body. `recorded` is replayed from the selected transcript (what the model saw); `latest_built` may be awaiting recording.
- `context_files() -> ContextFilesInspection { startup, projected_nested }`: loaded startup files (AGENTS.md-style) and nested instruction blocks still present in tool results.
- `file_ledger() -> HashMap<PathBuf, String>`: canonical paths with recorded full-file hashes.

**Axome:** Phase 2 Context view gains Prompt, Context files and Files tabs. Phase 3a can use the Prompt view to prove the `<agent>`/`<system>` split.

## 4. Command output with stream labels

- `exec/host.rs`: `OutputStream { Stdout, Stderr, Merged }`, `OutputChunk { stream, bytes }`; capture forwards labeled chunks and surfaces read errors.
- `jobs/command.rs`: a `.log.streams` sidecar (9-byte records: LE end offset + tag) beside each `.log` artifact; `CommandOutput::read_chunks(range)`; public `CommandOutput::open(job, artifact, path)` for retained logs; `OutputProgress.capture_failed`.
- `Session::command_output(call_id)` falls back to the retained artifact when the live spool is gone (`jobs/mod.rs:1558-1590`).

**Axome:** Phase 2 Jobs view colours stderr and reads reopened logs with labels.

## 5. Modes: effective mode and edited results

- `Session::effective_mode()`, `LiveSnapshot.effective_mode`, `SessionEvent::EffectiveMode { mode }` (emitted at the request boundary, `session/surface.rs:162-167`). `Session::mode()` / `SessionEvent::Mode` stay the *selected* mode.
- `ToolScope.mode` handed to `ToolPolicy::before_tool_call` is the **effective** mode (`session/policy.rs:150-160`).
- `accept_result(call_id, AcceptResultOptions { replacement, message })` replaces `accept_result(call_id, Option<String>)`. A replacement is validated against the mode's `result.schema` (`PendingResult.schema`); an invalid one returns `InvalidInput` and leaves the review pending (`session/modes.rs:346-366`). The model is told "The host edited your result" with the accepted value.
- The persisted `yield` result `details` record `{mode, outcome, result}` plus the original result when edited.

**Axome:** Phase 3 (mode pill, write-scoping policy, Plan review actions).

## 6. Retry detail

- `SessionEvent::Retry { attempt, delay_ms, max_retries, error, failure }`; also emitted while a compaction summary retries (`compaction/summary.rs:436-444`).

**Axome:** Phase 2 shows `Retrying n/max · in Ns · <error>` on the running turn.

## 7. Skill roots on `ConfigLayer`

- `UserConfig.skill_roots` moved to `ConfigLayer.skill_roots`, so the project layer can supply skill roots (`config.rs:408-414`).
- Effective order: project `.agents/skills` walk, allowed project `skill_roots`, user `skill_roots`, `<home_agents>/skills`; first name wins (`context/skills.rs:31-53`).
- Project skill roots load at `ProjectAccess::Read` **and** `Trust` when `project.files.skills` and `project.files.config` are on; agent, mode and keyword roots still need `Trust` (`config.rs:497-525`). Project settings and prompt overrides also merge at `Read`; only the dev shell needs `Trust`.

**Axome:** Phase 2 supplies `<root>/skills` on both layers (`~/.config/axome/skills`, `<repo>/.axome/skills`). Phase 4 must decide what Restricted Mode does with them.
