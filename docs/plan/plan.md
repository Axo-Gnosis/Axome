# Axome plan: adopting axo

This is the live planning document for adopting axo in Axome; the pinned revision is below. It changes freely as we work.

- **Goals & settled decisions:** [`design.md`](design.md)
- **Evidence:** [`research/`](research/README.md) (axo surveys, file:line cited)
- **Pinned axo:** `5c3f418` (`614089b` plus `SessionOptions::keywords_enabled`; see [T10](#t10-remaining-axo-gaps)). The `614089b` adoption is [T11](#t11-axo-614089b-providers-login-and-the-live-catalog). Earlier handoffs: Phase 2 Live Work (`97513d6`), [host capabilities](research/axo-cd12162/host-capability-handoff.md) (`cd12162`), [Phase 3](research/axo-b961066/host-capability-handoff.md) (`b961066`), and [snapshots and steer/queue control](#t10-remaining-axo-gaps) (`4d6bc34`, `a0ec20d`).

## How we work

- **One topic at a time**, in the order of the table below. For each topic: re-read its facts, answer its questions, record each answer as a decision in [`design.md`](design.md), then mark the topic **Decided**.
- **Questions keep their IDs** for the whole adoption, so notes and commits can refer to them. New questions get the next free number.
- **Facts come from axo source, not its docs.** If axo moves, re-survey the affected area into `research/axo-<rev>/` and update the facts here.
- **When a question needs axo work**, add it to [T10](#t10-remaining-axo-gaps) so requests to axo go out in batches.
- **Implementation starts per topic** once its decisions are recorded; the [mechanical migration](#mechanical-migration) can start as soon as T1 is decided.

## Topics

| Topic | Scope | Questions | Order | Status |
|---|---|---|---|---|
| [T1](#t1-foundation-config-storage-construction) | Config, storage, construction, supervision | Q1–Q4 | 1 | Decided |
| [T2](#t2-tool-model) | `kill`, `send`, `call`, deferred tools, tool cards, approval | Q5–Q10 | 3 | Implemented (Phase 2) |
| [T3](#t3-background-jobs) | Job surface, live output, wakes, closing with running jobs | Q11–Q16 | 4 | Implemented (Phase 2) |
| [T4](#t4-subagents-and-teams) | Children, teams, steering, results, sidebar | Q17–Q24 | 4 | Implemented (Phase 2) |
| [T5](#t5-personas) | Root agent per conversation, switching, authoring | Q25–Q28 | 2 | Implemented and verified (Phase 3 Stage A) |
| [T6](#t6-modes) | Plan/research modes, review, mode UI, keywords | Q29–Q35 | 2 | Implemented and verified (Phase 3) |
| [T7](#t7-models-and-roles) | `@role` selectors, roles UI, model picker | Q36–Q37 | 2 | Decided |
| [T8](#t8-host-messages-sections-dynamic-tools) | What Axome tells the model, custom message rendering | Q38–Q39 | 5 | Implemented (Phase 2) |
| [T9](#t9-session-lifecycle-and-status) | Context meter, cost, unloading, live-session registry | Q40–Q42 | 5 | Implemented (Phase 2) |
| [T10](#t10-remaining-axo-gaps) | axo gaps: accept, work around, or request | Q43 | 6 | Decided |
| [T11](#t11-axo-614089b-providers-login-and-the-live-catalog) | axo `614089b`: provider/login seam, live model catalog, layered writes | Q44–Q57 | 7 | Slices 1–7 implemented; slice 8 awaits axo |
| [T12](#t12-long-term-memory) | Native long-term memory (Basic Memory port) | Q58–Q62 (superseded) | 8 | Shipped off by default ([P31](design.md#decisions)) |

```mermaid
flowchart LR
  T1[T1 Foundation] --> T567[T5 Personas · T6 Modes · T7 Roles]
  T567 --> T2[T2 Tool model]
  T2 --> T34[T3 Jobs · T4 Subagents]
  T34 --> T89[T8 Host messages · T9 Lifecycle]
  T89 --> T10[T10 axo gaps]
```

T5–T7 go together because they define the composer and the `/plan` story; T3 and T4 go together because jobs and children share one registry and probably one surface.

## Vocabulary

- **Persona:** the root agent definition. Replaces the system prompt body and shapes the tool set. Switched with `Session::set_agent`.
- **Mode:** an overlay on a root session. Adds instructions as a hidden message, limits which tools may run, may switch model on entry, and may end with the model handing back a result. Switched with `Session::set_mode`. Leaves the system prompt alone.
- **Model role:** `@name` in a model selector, mapped to a concrete model in user settings. `@plan` is a role, not an agent.
- **Deferred tool:** a tool the model can run but whose schema is not in the declared tool list. The model finds it through `tool://` and runs it through the `call` tool.
- **Job:** a row in a session's job registry: a background command (`bg_N`) or a child session (`agent_N`).

---

## Mechanical migration

Compile and correctness fixes with nothing to decide beyond T1. Evidence: [`research/axo-843cd73/compile-probe.md`](research/axo-843cd73/compile-probe.md) (38 errors: 28 production, 10 tests).

- [x] Construction: `Axo::builder(models, store)`, sync `build()`, `ConfigSource`; Axome keeps its own `Models` and settings (shape decided in T1).
- [x] Exhaustive matches: entries `job_start`, `child_start`, `job_end`, `agent_change`, `mode_change`; events `Job`, `JobGone`, `Context`, `AutoPaused`, `Mode`, `ModeResult`, `ModeResultResolved`; submission kinds `Mail`, `Quiet`; `CommandKind::Mode`; the new error variants in `axo_error`.
- [x] Flat `LiveSnapshot` (`snapshot.agent` is now the persona name).
- [x] `abort()` is sync; `NavigateOptions.focus`; `ModelsOptions.http_client` in tests.
- [x] Drop `lastLinePartial` from `payload.ts` and its test fixture (capped read/grep/find output currently loses structured rendering).
- [x] Forward `details` on partial tool updates (`live.rs:410-423`).

How each new entry, event and message renders is decided in its topic; the migration only makes the matches explicit.

---

## T1. Foundation: config, storage, construction

**Status:** Decided · **Evidence:** [`research/axo-6cdaeb4/config-and-embedding.md`](research/axo-6cdaeb4/config-and-embedding.md)

**What axo gives**
- `Axo::builder(Arc<Models>, Arc<dyn SessionStore>)` with `.config(ConfigSource)`, `.cache_dir`, `.name`, `.supervision`. axo no longer reads `~/.axo`.
- `ConfigSource` supplies `user()` and `project(cwd)` layers, including agent roots and mode roots. axo owns the settings schema, merging, trust filtering and `$VAR` expansion; user-only keys (`modelRoles`, `subagents`, `homeAgentsDir`, `project`) are ignored in project layers. `user()` is sampled at session create and open; there is no hot reload.
- No cache dir means no bundled docs scheme and no Nix dev-shell capture.
- `.name("axome")` renames the harness in the bundled prompts and the docs scheme (`axome://`).
- Supervision: process group (default, nothing to ship) or cgroup v2 (Axome's binary must route a supervisor entry in `main()`, and needs a delegated cgroup).
- axo-ai: Axome owns `Models`; config values starting with `!` are rejected, so command-based secrets resolve in Axome.

**Questions**
- **Q1.** Where do settings, models, credentials, sessions and cache live in Axome's data dir, and what happens to existing `~/.axo` sessions and `auth.json`: import once, read in place, or ignore?
- **Q2.** Harness name: call it `axome` in prompts and the docs scheme, or keep `axo`?
- **Q3.** Supervision: process group or cgroup?
- **Q4.** Settings UI scope: which axo settings Axome surfaces (retry, `bash.promoteAfterMs`, `subagents.*`, `modelRoles`), and where project trust decisions come from.

**Decisions:** [D1](design.md#decisions), [D2](design.md#decisions), [D3](design.md#decisions), [D4](design.md#decisions).

---

## T2. Tool model

**Status:** Implemented (Phase 2) · **Evidence:** [`research/axo-843cd73/tools.md`](research/axo-843cd73/tools.md)

**What axo gives**
- **`kill`** (`{id}`, `bg_N` or `agent_N`) and **`send`** (`{to, message, steer?}`) are tools; `proc://` and `agent://` are read-only. Killing a child stops its current run, not the child. No team-wide kill.
- **`call`** runs a deferred tool (`{tool, args}`). One call has two names: the assistant's tool call is `call`; execution events and the tool result carry the inner tool's name, with the same call id.
- **`tool://`** lists deferred tools; reading `tool://<name>` describes one and asks axo to promote it into the declared list on models that support mid-conversation tool changes. A hidden `tool_catalog` message tells the model when the deferred set changes.
- **Host tools:** `SessionOptions.tools: Vec<HostTool>`, each `HostTool::direct` or `HostTool::deferred`. Axome registers none today.
- **`ToolPolicy`** (replaces `tool_hooks`): async `before_tool_call` (Allow or Block; no argument rewrite) and `after_tool_call`, with a `ToolScope` (session, depth, persona, mode). Children inherit policies. No built-in approval protocol; a host builds one by awaiting inside the policy.
- **Result `details`:** bash `{exit, durationMs, fullOutput?}`, or `{job, fullOutput}` when promoted; spawn `{job}` or `{team, jobs}`; wait `{jobs}`; read, edit and write carry a ledger record. `kill`, `send` and `yield` have none.

**Where Axome stands**
- Tool cards are chosen from the assistant block's name (`turns.ts:69-80`), so every dispatched tool renders as a generic `call` card.
- `kill`, `send`, `spawn`, `wait`, `yield`, `enter_mode` and `call` have no cards.
- Bash footer parsing does not know the new stop wording; the typed `exit` makes the parsing unnecessary.

**Questions**
- **Q5.** Dispatched calls: show the inner tool's card (with a "via call" mark) or the outer `call`?
- **Q6.** Card set: which new tools get dedicated cards, and do job-related cards (promoted bash, spawn, wait, kill, send) link into the jobs and agents surface instead of repeating it?
- **Q7.** Replace bash footer text parsing with the typed `exit` and `durationMs`?
- **Q8.** Tool approval: do we want a permission system (per tool, per persona, per mode, per child), and what does the prompt look like? A whole feature, enabled by `ToolPolicy`.
- **Q9.** Host tools: which tools of its own does Axome offer the model (open in editor, show diff, notify, task list), and which are direct or deferred?
- **Q10.** Deferred catalog: user-visible anywhere, given a host read of `tool://<name>` triggers promotion?

**Decisions:** [D8](design.md#decisions), [D12](design.md#decisions), [D13](design.md#decisions), [D14](design.md#decisions), [D15](design.md#decisions).

---

## T3. Background jobs

**Status:** Implemented (Phase 2) · **Evidence:** [`research/axo-843cd73/host-surface.md`](research/axo-843cd73/host-surface.md), [`research/axo-6cdaeb4/jobs.md`](research/axo-6cdaeb4/jobs.md)

**What axo gives**
- Per-job events (`Job(JobInfo)` upsert, `JobGone`) and `snapshot.jobs`. A row has id, status (`Queued`, `Running`, `HelpersRunning`, `Finished`, `Failed`), `call_id`, `started_at`, and messages with delivery state. Command rows add label, tool, cwd, artifact, promotion cause and time, end time, and a typed exit (`exited`, `signaled`, `timedOut`, `killed{by}`, `aborted`, `interrupted`).
- `Session::command_output(call_id)`: a byte-offset progress watch plus range reads, continuous from foreground through promotion to exit. stdout and stderr are merged. Not restored after reopen; the artifact file stays readable.
- `Session::background(call_id)` (the user's "send to background") and `Session::kill_job(id)`.
- `jobs_from_entries(header, entries)` for closed sessions. A command that never ended shows as `Running` there until the session is opened and reconciled.
- Finished jobs wake an idle session and arrive as a `job_result` custom message whose details list the delivered message refs.
- `AutoPaused` event: job wakes are suppressed after an error or abort.

**Questions**
- **Q11.** Where jobs live in the UI: session bar, side panel, dock, or expandable inline cards. Per session only, or also a window-wide view across tabs?
- **Q12.** Terminal view: live output from the merged stream; scrollback size; what it shows after reopen (the static artifact).
- **Q13.** Delivered results in the transcript: today a muted notice. A proper "bg_1 finished" element linked to its row?
- **Q14.** The wake: a finished job starts a turn on its own. How the UI makes that legible, and how auto-pause is shown and cleared.
- **Q15.** Closing a tab or the app with running jobs: stop, keep the session alive, or block. Axome's idle unload would drop the session, which stops its commands and children [INFERENCE: read from code, not run].
- **Q16.** History views: how to label "was running when the session closed".

**Decisions:** [D12](design.md#decisions), [D16](design.md#decisions), [D17](design.md#decisions).

---

## T4. Subagents and teams

**Status:** Implemented (Phase 2) · **Evidence:** [`research/axo-843cd73/host-surface.md`](research/axo-843cd73/host-surface.md), [`research/axo-6cdaeb4/agents.md`](research/axo-6cdaeb4/agents.md)

**What axo gives**
- Children are job rows (`JobKind::Child`): session id, path, depth, persona, model, team ref, `resident`, and progress (`turns`, `max_turns`, `wrapping_up`, `yield_failures`, `usage`), pushed after each child assistant message. `Queued` is a real status.
- `Axo::session(child_id)` returns the live child; `child.attach()` is the same stream as a root, so a child renders as a chat and accepts steer, follow-up and abort.
- Host spawn: `Session::spawn`, `spawn_team`. `SpawnHook` (Allow, Block, Replace) for policy.
- Durable: `JobEnd.value` (typed yield result), `JobEnd.usage`, notes vs results, mail with sender.
- Opening a child file directly fails with `ChildSession { root }`; the host opens the root, then asks for the child. Read-only open works without the root.
- Not exposed: current tool and latest text on the parent row, queue position, live team state (who waits, pending mail, mutual wait), teammate mail and parent notes from the host, dismissing a row, tree usage rollup.

**Questions**
- **Q17.** Mental model: children as nested conversations (tree in the sidebar), rows in an agents panel, tabs, or split panes? What does a team look like?
- **Q18.** Depth of the live view: parent-row progress only, or attach each visible child for current tool and streaming text?
- **Q19.** User control: steer, follow up, stop a child run; can the user spawn a child, and from where?
- **Q20.** Results: how a child's result and structured value appear in the parent transcript, and how the user gets from the result to the child's transcript.
- **Q21.** Sidebar and history: child sessions appear in `list_sessions`; hide them, nest them under the root, and what a click does when the root is closed.
- **Q22.** Spawn approval: does `SpawnHook` drive a UI (confirm, pick the model, replace the agent) or stay silent?
- **Q23.** Cost: per-child and whole-tree usage (Axome sums it).
- **Q24.** Agent definitions: where they live (app data, project, both), whether Axome ships any, and whether there is an editor.

**Decisions:** [D12](design.md#decisions), [D18](design.md#decisions), [D19](design.md#decisions), [D20](design.md#decisions).

---

## T5. Personas

**Status:** Implemented and verified (Phase 3 Stage A; verification record in the roadmap's Phase 3 status) · **Evidence:** [`research/axo-843cd73/host-surface.md`](research/axo-843cd73/host-surface.md)

**What axo gives**
- `agent_change` entry; reopen restores the persona by name from the definitions loaded at that time. A missing definition gives a diagnostic and the session runs without it.
- `Session::set_agent(name | None)`: root only, allowed during a run, applies from the next request, and applies the persona's model selector if it has one. Clearing does not restore the previous model.
- `runAs: root | subagent | any` on definitions.
- `Axo::catalog(cwd, access, agents, modes)` lists agents, teams, modes, config and diagnostics without a session.
- Forks inherit the current persona.

**Questions**
- **Q25.** Is the persona a per-conversation choice (picker in composer or header), a per-project default, a global default, or all three?
- **Q26.** Does Axome ship a default persona of its own (its own system prompt)?
- **Q27.** Switching mid-conversation: how the transcript marks it, and how the user learns the model may change with it.
- **Q28.** Where personas are authored and stored, and their relation to T4's agent definitions (same files; `runAs` decides where each appears).

**Decisions:** [D5](design.md#decisions), [P17](design.md#decisions) (`defaultAgent` is an Axome-owned key inside `"axo"`).

---

## T6. Modes

**Status:** Implemented and verified on `b961066` (both Phase 3 stages; verification record in the roadmap) · **Evidence:** [`research/axo-843cd73/modes.md`](research/axo-843cd73/modes.md), [`research/axo-b961066/`](research/axo-b961066/host-capability-handoff.md)

Stage A on `cd12162` added mode commands, set-mode-then-steer with attachments intact, the selected/effective mode pill, non-plan review cards, composer keyword highlighting and frontmatter editing. Stage B adopts axo K and W from `b961066`: durable sent-message keyword chips, `plansDir`, bundled `/research` and `/plan*` modes, safe plan saving, and the Plan view with all five review actions.

**What axo gives** (surveyed at `843cd73`; historical where [T10](#t10-remaining-axo-gaps) records a later change: edited results, the effective-mode signal and recorded review outcomes arrived in `cd12162`)
- Definitions are markdown with frontmatter: `name`, `description`, `argument-hint`, `disable-model-invocation`, `model`, `tools.allow`, `tools.deny`, `result.schema`, `result.review`, `result.then`, plus free extra keys for host policy. Roots come from `ConfigSource` or `SessionOptions`. axo ships none.
- Enter with `Session::set_mode(Some(name))`, the `/name [prompt]` command through `Session::parse_command`, or the model's `enter_mode` tool (unless the mode disables model invocation). Leave with `set_mode(None)` or by the model finishing through `yield`.
- Effects: a hidden `mode_notice` message carrying the body; tool limits enforced at call time by tool name (not a shell sandbox); model and thinking switched on entry and restored on exit unless the user changed the model in between.
- Result review: with `result.review`, `yield` waits; the host gets `ModeResult { call_id, mode, result, awaiting_review }` and answers with `accept_result(call_id, message?)`, `revise_result(call_id, feedback)` or `dismiss(call_id)`. No way to accept an edited result. Pending reviews are in memory only. `result.then`, or the accept message, continues the run.
- State: the `mode_change` entry follows the branch; `SessionEvent::Mode`, `snapshot.mode`, `snapshot.pending_results`. These report the selected mode; nothing signals when it takes effect.
- Children spawned in a mode inherit its tool limits as a permanent ceiling, not its instructions.
- Mode and persona are independent; a mode cannot select a persona.

**Where Axome stands**
- Modes have their own composer group. The selected mode changes immediately; the effective mode changes at the next model request. Pending reviews recover from axo's journal on reopening.
- Plan drafts stay in `local://`, render live and open in the normal editor. Accepted plans are saved without overwriting an existing file, then optionally implemented in the same session, after compaction, or in a new chat.
- Native `write`/`edit` scopes follow child sessions. Read-only bash is a separate native restriction with writable scratch paths; projects under those paths are not protected from bash writes ([P18](design.md#decisions)).

**Questions**
- **Q29.** Which modes Axome ships (plan first), and what plan mode is: read-only tools, a `@plan` model role, a reviewed result that is the plan.
- **Q30.** "Plan mode with a plan agent": mode only, mode plus a persona switch (Axome does both), or persona only. Decides whether the system prompt changes.
- **Q31.** Review UI: where the plan is shown; approve, ask for changes, dismiss; what approval continues with (implement now, new session, hand to a child).
- **Q32.** Does the approved plan persist as its own object? axo keeps it only as the tool call's arguments.
- **Q33.** Mode indicator and exit control in the composer; how model-initiated entry (`enter_mode`) is shown or gated.
- **Q34.** Read-only really meaning read-only: tool-name limits still allow `bash`. Deny bash in plan mode, or add a `ToolPolicy`?
- **Q35.** User-defined modes: supported, where stored, and how they sit in the `/` picker next to skills and built-ins.

**Decisions:** [D6](design.md#decisions), [D7](design.md#decisions), [D8](design.md#decisions), [D9](design.md#decisions), [D10](design.md#decisions), [P17](design.md#decisions) (Q34 answered by axo-enforced `writes`, not a `ToolPolicy`), [P18](design.md#decisions) (native bash scratch-space exception retained and documented).

---

## T7. Models and roles

**Status:** Decided · **Evidence:** [`research/axo-6cdaeb4/agents.md`](research/axo-6cdaeb4/agents.md), [`research/axo-843cd73/modes.md`](research/axo-843cd73/modes.md) §1.5

**What axo gives**
- Selectors: `provider/model`, `@role`, `:thinking`, and combinations. `modelRoles` is a user-only setting. Personas, modes and `subagents.models` all take selectors.

**Questions**
- **Q36.** A roles screen in settings (default, plan, fast, …): which roles Axome defines and what uses them.
- **Q37.** The model picker offers concrete models only today; show roles too?

**Decisions:** [D11](design.md#decisions).

---

## T8. Host messages, sections, dynamic tools

**Status:** Implemented (Phase 2) · **Evidence:** [`research/axo-843cd73/host-surface.md`](research/axo-843cd73/host-surface.md)

**What axo gives**
- `Session::post(HostMessage { custom_type, text, display, delivery: Steer | FollowUp | Quiet })`: durable; `Quiet` never starts a run. Seven engine custom types are reserved.
- `set_section(name, body | None)` and `set_tools(...)`: root only, apply from the next request, must be supplied again on reopen.
- No `prepare_request` hook for hosts.

**Where Axome stands**
- Axome's notice wire type keeps only text, so job results, mail, notes and host messages all look alike.

**Questions**
- **Q38.** What Axome tells the model on its own (open file, selection, git state, UI events), and through which channel: quiet message, prompt section, or nothing.
- **Q39.** Rendering custom messages: how job results, mail, child notes and host messages are told apart.

**Decisions:** [D21](design.md#decisions), [D22](design.md#decisions).

---

## T9. Session lifecycle and status

**Status:** Implemented (Phase 2) · **Evidence:** [`research/axo-843cd73/host-surface.md`](research/axo-843cd73/host-surface.md)

**What axo gives**
- Context meter: `ContextUsage { tokens, window, trigger, reserve }` with `Context` events. Axome currently derives context from the last response's token count.
- Cumulative usage in the snapshot; per-run usage on `run_end`.
- `run_end.queued`: queued input survives reopen and is requeued without starting a run by itself.
- `/compact` and `/navigate` take a focus; compaction entries record reason and focus.
- No process-wide events on `Axo` and no list of live sessions.

**Questions**
- **Q40.** Context meter and cost display in the session bar.
- **Q41.** When Axome may unload a session, now that a session owns jobs and children (ties to Q15).
- **Q42.** Does Axome keep its own registry of live sessions for a window-wide activity view?

**Decisions:** [D17](design.md#decisions), [D20](design.md#decisions), [D23](design.md#decisions).

---

## T10. Remaining axo gaps

**Status:** Decided · **Handoff:** [`handoff/axo-pre-adoption-batch.md`](handoff/axo-pre-adoption-batch.md), [`handoff/axo-phase-3-batch.md`](handoff/axo-phase-3-batch.md) (fulfilled in `b961066`) · **Evidence:** [`research/axo-843cd73/host-surface.md`](research/axo-843cd73/host-surface.md) §7–8, [`research/axo-843cd73/modes.md`](research/axo-843cd73/modes.md) §7, [`research/axo-843cd73/tools.md`](research/axo-843cd73/tools.md) §7, [`research/axo-b961066/`](research/axo-b961066/host-capability-handoff.md)

Axome's 35-point request for a typed host surface (made against axo `6cdaeb4`) came back 14 shipped, 16 partly, 3 not shipped, 2 decided differently. What remains:

| Gap | State in axo | Axome impact |
|---|---|---|
| Ordered, timestamped event envelope | Not shipped; events are bare, only entries have seq and time | Re-attach still resyncs; a `Job` event can arrive before its `job_end` entry |
| Process-wide events, live session list | Not shipped | Axome keeps its own registry (Q42) |
| stdout/stderr separation | Merged stream | Terminal view cannot color stderr |
| Opening a child whose root is closed | Host opens root, then child | Axome orchestrates it (Q21) |
| Child row: current tool, latest text, queue position | Partly; turns and usage only | Attach children for rich rows (Q18) |
| Team live state, host mail and notes, dismissing a row | Partly | Team UI is roster plus per-child state |
| Command output after reopen | Artifact file only | Static view for old jobs (Q12) |
| Tree usage rollup | Per-run usage only | Axome sums (Q23) |
| Prompt and tool introspection with origin, config provenance, file ledger listing | Not shipped or partly | No "what the model sees" panel without more axo work |
| Host `prepare_request` hook, retry detail | Not shipped | Sections and quiet messages instead (Q38) |
| Mode: effective-mode signal, accept an edited result, durable review record | Not present | Shapes Q31–Q33 |
| **Pre-adoption blocker:** Side-effect-free tool & deferred-catalog inspection | `Session` has no public tools getter, and `Schemes::read("tool://<name>")` mutates promotion state | Request axo addition: `Session::tools()` returning direct/deferred/promoted/mode-restricted state, and keep host `Schemes::read` side-effect-free ([D15](design.md#decisions)) |
| Typed `kill`/`send` receipts, dispatch provenance on events | Not present | Shapes Q5–Q6 |
| **Pre-adoption blocker:** `AgentSpec.body` overwrites `<system>` | Root personas and spawned subagents both replace `SYSTEM.md`, losing harness mechanics (`local://`, resume, steering) | Request axo change: keep `<system>` strictly for `SYSTEM.md`/host override and place `AgentSpec.body` in its own `<agent>` section on both root and child sessions ([D5](design.md#decisions)) |
| **Pre-adoption blocker:** `:` disallowed in mode and skill names | `markdown::name_problems` permits `[a-z0-9-]` only | Request axo change: allow `:` namespace separators (`plan:roadmap`, `plan:step`, `review:adversarial`) ([D6](design.md#decisions)) |
| **Pre-adoption blocker:** Kernel-enforced read-only `bash` | Mode tool limits are name-only; no shell filesystem sandbox | Request axo feature: native Linux read-only `bash` execution mode (e.g. Landlock: writable `/tmp` + session `local/`, read-only everywhere else) usable by modes and subagents ([D8](design.md#decisions)) |
| **Pre-adoption blocker:** Magic keywords (`orchestrate`, …) | Not implemented in axo | Request axo feature: `KeywordSpec` (`keywords/*.md` + host specs), standalone lowercase prose matching, tool gates, optional thinking overrides, hidden per-turn notice injection, and `Catalog` metadata ([D10](design.md#decisions)) |
| Explicit `Session::resume_wakes()` for `auto_paused` | `auto_paused` clears on next user prompt; no dedicated host method | Request small axo helper so `SessionBar`'s `Resume` button clears `auto_paused` and delivers pending job results without sending a dummy prompt ([D16](design.md#decisions)) |
| ~~**Found in Phase 1:** no authoritative idle signal~~ | **Shipped in `cd12162`:** `SessionEvent::Activity` / `Session::is_running()` publish idle only after queued input and wakes drain | Adopted: `ChatLive.running/maintenance` follow `Activity`; the test fixture's snapshot polling is gone |
| **`cd12162` handoff, adopted in Phase 2** | stdout/stderr labels (`.log.streams`, `read_chunks`, `CommandOutput::open`); typed `SpawnReceipt`/`WaitReceipt`/`KillReceipt`/`SendReceipt`; `inspect_prompt`, `context_files`, `file_ledger`; `Retry` detail; `ConfigLayer.skill_roots` | Jobs view colours stderr including reopened logs; receipts parse typed details (wait reason shown); Context view adds Prompt, Context files and Files tabs; retry row on the running reply; `.axome/skills/` is a project skill root. Supersedes the "stdout/stderr separation", "typed kill/send receipts" and "prompt introspection" rows above |
| **`cd12162` handoff, for Phases 3 and 4** | `effective_mode` + `EffectiveMode`; `AcceptResultOptions { replacement, message }` with schema validation; `Session::send` + `SubmissionProvenance`; project skills and settings load at `ProjectAccess::Read` | Addenda in the roadmap's Phase 3 and Phase 4 sections list what each phase plan must verify and plan for; the Phase 3 items are settled in [P17](design.md#decisions). Supersedes the "Mode: effective-mode signal, accept an edited result" row above |
| **Phase 3 batch, shipped in `b961066`** | **K:** `Entry::Submission.keywords: Vec<String>` (fired names, deduplicated, text order, model-only included; old journals read empty). **W:** `writes: Option<Vec<String>>` on `ModeSpec`, `AgentSpec`, `ToolScope`, `SpawnLink`; entries are scheme URL prefixes or cwd-relative/absolute directories, absent unrestricted, `[]` none; enforced in `write`/`edit` by real path; effective scope is agent ∩ effective mode ∩ inherited ceiling, persisted on `SpawnLink`, `local://` meaning each child's own; does not restrict `bash` | Pin bumped; Stage B (keyword chip, bundled modes' `writes`, Toolbox `writes` line) uses the names as requested ([`research/axo-b961066/`](research/axo-b961066/host-capability-handoff.md)) |
| **Adopted from axo `4d6bc34`:** per-turn file snapshots & `write` diff metadata | `write` and `edit` record `details.change` (`FileChange`) with content-addressed `<session>/snapshots/<sha256>` blobs, `LineDiff` (`additions`, `deletions`, `firstChangedLine`, `patch`, `patchTruncated`), `Axo::{read_snapshot, squash_changes}`, `Session::{read_snapshot, squash_changes}`, and `axo::{file_change, squash}`. **Breaking:** `edit` dropped top-level `diff`/`patch`/`first_changed_line` | Adopted: `editDiff` / `toolHeader` read `details.change.diff`; transcript-only per-turn `N files changed +A -D [Review]` cards open Monaco `DiffStack` ([`handoff/axo-turn-diff-snapshots.md`](handoff/axo-turn-diff-snapshots.md)) |
| **Adopted from axo `a0ec20d`:** steer reliability & queue control | `abort` latches when pressed (not after settle); `resume_wakes` delivers held steers/follow-ups; queued steers settle pending `ask` & reviewed `yield` (`ResultResolution::Steered`); `steeringMode` defaults to `"all"`; `LiveSnapshot::queued: Vec<QueuedInput>` replaces `queued_steering`/`queued_follow_up`; `Session::withdraw` removes queued input & writes `Entry::SubmissionWithdrawn` | Pin bumped to `a0ec20d`; collapsible `QueuedMessages` card (`→` promote/interrupt-and-send, Edit prepending w/ `\n`, Cancel), double-Enter guard, and `whitespace: 'pre'` newline preservation landed ([`handoff/axo-steer-queue.md`](handoff/axo-steer-queue.md)) |
| **axo `614089b`:** provider/login/rewrite seam, models.dev v2 + Codex account catalog with refresh, layered write scopes, `withdraw` → `QueuedInput` | Shipped; Axome pins `5c3f418`, which builds on it | Adoption is [T11](#t11-axo-614089b-providers-login-and-the-live-catalog); registry change signal and logout requested in [`handoff/axo-models-batch.md`](handoff/axo-models-batch.md) |
| **axo `5c3f418`:** `SessionOptions::keywords_enabled` | Shipped; `false` loads no keywords for the session and its forks | The memory writer sets it, so keyword words quoted in a captured transcript never inject bodies or switch the writer off `@memory` |

**Questions**
- **Q43.** For each row: live with it, work around it in Axome, or send it back to axo before designing the UI that depends on it. Answer per row once the topics that depend on it are decided.

**Decisions:** [D24](design.md#decisions) — 12 items batched for axo in [`handoff/axo-pre-adoption-batch.md`](handoff/axo-pre-adoption-batch.md); remaining rows accepted/handled in Axome. [P17](design.md#decisions) — the Phase 3 batch (W, K) gated Stage B; [`handoff/axo-phase-3-batch.md`](handoff/axo-phase-3-batch.md) is fulfilled by `b9610666`; both Phase 3 stages are implemented and verified. [P18](design.md#decisions) records the accepted native bash scratch-space exception.

---

## T11. axo `614089b`: providers, login and the live catalog

**Status:** Slices 1–7 implemented; slice 8 awaits axo · **Evidence:** [`research/axo-614089b/`](research/README.md) (compile probe, provider/login, catalog, session/write-scope surveys; harness comparisons; Axome code answers; legacy journals) · **Handoff:** [`handoff/axo-models-batch.md`](handoff/axo-models-batch.md)

**What axo gives** (`a0ec20d..614089b`: `54dccae`, `0f4f890`, `614089b`)
- **Providers and login:** `Models::{register, unregister, reload, login}`; `ProviderLogin` + `LoginInteraction` (`open_url`, `device_code`, `prompt{secret}`, `select`, `progress`; cancel by dropping); `RequestRewrite`; `CodexLogin` replaces `auth::codex::{login, refresh}`; `ProviderInfo.oauth` → `login`; OAuth credentials gain `headers`/`baseUrl` (auth.json readable as is); requests resolve against the current registry (`Error::UnknownModel`).
- **Live catalog:** models.dev v2 (`models.dev.v2.json`, old cache ignored), the ChatGPT account's `/codex/models` list (fetched after login, offline floor otherwise), `Models::refresh` and a 2 h timer (`refreshInterval`, `catalogUrl`); open sessions adopt changed definitions at the next request, clamping thinking with an ordinary `ThinkingLevelChange`.
- **Sessions:** layered real-path write scopes (`WriteScope`; old persisted child ceilings no longer parse); `withdraw` returns `QueuedInput` with `custom`; delivery race and reentrancy fixes; accepted-review details in camelCase; `PipeChunk` capture; concurrent squash.
- **Breaking for Axome:** 7 production + 3 test compile errors ([`compile-probe.md`](research/axo-614089b/compile-probe.md)).

**Questions** (all answered)
- **Q44.** Registry lifetime and what Reload Models means → [P21](design.md#decisions).
- **Q45.** First-run catalog wait after the v1 → v2 cache change → [P21](design.md#decisions).
- **Q46.** Exposing refresh, interval, feed URL, freshness → [P21](design.md#decisions).
- **Q47.** Keeping frontend model lists current, including the post-login Codex list → [P22](design.md#decisions).
- **Q48.** Which login interactions Axome supports, and remote hosts → [P23](design.md#decisions).
- **Q49.** Provider card semantics now that `login` is not OAuth → [P24](design.md#decisions).
- **Q50.** Telling offline, unauthenticated and invalid config apart → [P24](design.md#decisions), [P21](design.md#decisions).
- **Q51.** A selected model that no longer exists → [P25](design.md#decisions).
- **Q52.** Grounded search auth and default model → [P26](design.md#decisions).
- **Q53.** Runtime providers and request rewrites → [P27](design.md#decisions).
- **Q54.** Automatic thinking clamps → [P27](design.md#decisions).
- **Q55.** Unknown and tiered prices, extended context → [P27](design.md#decisions); unknown-price support requested as optional item U.
- **Q56.** Child journals with legacy write ceilings → [P27](design.md#decisions).
- **Q57.** Queued custom host messages → [P27](design.md#decisions).

**Implementation**

Slices 1–7 run on `614089b`; slice 8 waits for the handoff.

1. **Pin bump and mechanical fixes.** Bump `Cargo.toml:28-30`; snapshot `Models::all()` at `providers.rs:13` and `mod.rs:688,696`; `withdraw(..).parts` at `mod.rs:1018` (ponytail comment: Axome posts no host messages, P27); `custom: None` in the three `convert.rs` queue fixtures. `login.rs:61` and `web/auth.rs:160` go away in slices 3 and 6. Update the `withdraw` signature in [`handoff/axo-steer-queue.md`](handoff/axo-steer-queue.md).
2. **Registry lifetime (P21).** Build the engine once per host; `models.jsonc`/`extendedContext` changes call `Models::reload` (error kept as `models_error`, last good registry kept); credential actions stop invalidating; **Refresh models** runs `reload` + `refresh` and reports errors; the model picker shows a loading state while the first catalog fetch runs.
3. **Generic login (P23).** `login.rs` implements `LoginInteraction` as a current-state view (`url`, `deviceCode`, `progress`, and at most one `prompt` or `select`). Questions await an answer by id; dropping the task cancels. Protocol, RPC, `backend.ts` and the Providers login dialog carry that view. The provider's prompt accepts a pasted redirect, with a loopback explanation on remote hosts. Any provider with `login` can start one.
4. **Provider cards (P24).** `login` replaces `oauth` on the wire; Log in and Add key independent; Remove/Log out only for stored credentials; environment hint; per-provider auth status from the backend (`available_models` already checks auth per provider: return the reason instead of dropping it).
5. **Model freshness (P22).** `HostEvent::ModelsChanged` (global, not folder-claimed) through desktop and server event mappings; emitted after key set/remove, login and Refresh models; the chat catalog cache, `axoModels` Settings states and Providers view refetch on it.
6. **Grounded search (P26).** `WebTools` gets the registry; grounded credentials via `Models::auth` for a registry model of the chosen provider; delete the Codex refresh in `web/auth.rs`; the grounded-model field resolves selectors (roles included) through `Models::find`; unset uses `@dash` when it lands on a grounded provider, else the hardcoded default.
7. **Unavailable model and copy (P25, P27).** Composer marks a selected model missing from the catalog as unavailable and blocks Send with a pick-another hint; `extendedContext` description explains the standard-price cap. README: login dialog, provider cards, Refresh models, `refreshInterval`/`catalogUrl`, grounded default.
8. **After the handoff lands.** Bump again; subscribe to `Models::subscribe` (S) and emit `ModelsChanged` from it; route Log out/Remove through `Models::logout` (L).

---

## T12. Long-term memory

**Status:** Shipped as native memory, off by default ([P31](design.md#decisions), superseding P28–P30). The Hindsight research below stays as evidence; its gates Q58–Q62 applied to a remote backend that Axome no longer uses. Workspace Trust gating of folder-level bank settings is tracked in `TODO.md`.

**Open questions / gates before enabling remote memory**

- **Q58. Deployment authority.** Where will credentials and the scope-bound broker live for each desktop/server principal, out of reach of model tools and repository code? Demonstrate real Workspace Trust and the code/web/admin split on the target deployment, not just in the isolated policy smoke. Until then: disabled.
- **Q59. Retrieval value.** On held-out decisions from more projects and dates, does ranking *approved note IDs* outperform searching the same local notes, with positive coverage ≥0.80, graded precision ≥0.70, zero forbidden output and on-demand p95 ≤2 s? Legacy-bank scores and a synthetic isolation check are not acceptance of the new corpus.
- **Q60. Forget and sharing.** Can deletion of one reviewed version be verified after an ambiguous retain acknowledgement, a late worker, or web recall of a code document? Qualify derivative/model refresh, backup expiry and already-delivered transcript limits before promising deletion outside the host.
- **Q61. Operating budget.** Does extraction **plus consolidation** of approved notes stay within the research budget on the single shared inference slot, including retries? Establish daily operator quota and energy/price accounting. Qualify cache loss on the actual target provider with deferred tools, compaction and resume.
- **Q62. Human scope mapping.** Which trusted worktrees, principals and deployments should share a note namespace, and does the user want code→web visibility for that scope? Require an explicit host-owned mapping; do not infer identity from repo text, basename, remote URL or a tag. Decide whether any legacy evidence warrants individual review; no bulk import.
