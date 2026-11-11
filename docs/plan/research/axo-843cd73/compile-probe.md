# axo 843cd73: Axome compile probe

Raw survey report, kept verbatim as evidence. Produced by a read-only subagent on 2026-11-10 against axo `843cd73`, verified against source code (not docs). A throwaway copy of Axome HEAD compiled against local axo; errors grouped by cause, plus frontend wire-key findings.

Citation prefixes are defined inside the report. Claims marked [INFERENCE] were not observed directly.

---

## Compile probe result

**38 compiler errors: 28 in production code, 10 test-only.** Cargo reported `could not compile axome-core (lib test) due to 38 previous errors`. This is one `--tests` compilation, not separate lib/test runs. Full diagnostic evidence: `artifact://45:302-340`.

Executed exactly the isolated archive/path-dependency approach, then:

```text
nix develop -c env CARGO_TARGET_DIR=/tmp/axome-probe-target \
  cargo check -p axome-core --tests --message-format short
```

Completed in **79.36 seconds**. Nix warned about an untrusted flake setting; the shell hook printed `fatal: --local can only be used inside a git repository`, but neither blocked compilation. No copy-local git initialization was necessary.

**Both `/tmp/axome-probe` and `/tmp/axome-probe-target` were removed. Neither source working tree nor its git state was modified.**

Below, Axome paths beginning `axo/` mean `src-crates/axome-core/src/services/axo/`; axo source paths are relative to `/repo/rust/axo`.

## Production errors grouped by root API change

| Root change / commit | Every compiler hit in Axome | Replacement / source evidence at HEAD |
|---|---|---|
| Host-owned construction/configuration, principally **de732d7** | `axo/mod.rs:137` (builder arguments), `:139` (awaiting synchronous build), `:234`, `:300`, `:302`, `:308` (removed `models()`), `:330`, `:334` (removed `settings()`), `:71` (removed `Error::NoAxoDir`) — **9 errors** | Supply `Arc<Models>` and `Arc<dyn SessionStore>` to `Axo::builder`; `build() -> Axo` is synchronous. Host retains model/config ownership; `AxoBuilder::config` accepts `ConfigSource`. `crates/axo/src/axo.rs:60-79,93-103,119-151`; current error enum `crates/axo/src/error.rs:5-51`. |
| `AgentSnapshot` removed / session snapshot flattened, **a0409ab**; `LiveSnapshot.agent` subsequently reused for persona name by **e070490** | `axo/convert.rs:5`; `axo/live.rs:65,71,100` — **4 errors** | Use `LiveSnapshot.running`, `.pending_tool_calls`, `.streaming`, `.queued_steering`, `.queued_follow_up` directly. `.agent` is now `Option<String>`, not the old nested agent state. `crates/axo/src/session/mod.rs:311-336`. |
| `Session::abort` async → synchronous, **a0409ab** | `axo/mod.rs:423` — **1 error** | `Session::abort(&self)` returns `()`, not a future. `crates/axo/src/session/mod.rs:2070`. |
| `NavigateOptions.focus` added, **cbfd24d** | `axo/mod.rs:527,541` — **2 errors** | Supply `focus: Option<String>` or use default struct update. `crates/axo/src/session/mod.rs:175-180`. |
| `CommandKind::Mode` added, **b80fea8** | `axo/convert.rs:91` — **1 error** | Existing Builtin/Skill mapping is no longer exhaustive; new variant is `Mode { argument_hint: Option<String> }`. `crates/axo/src/commands.rs:63-70`. |
| Additional journal variants: prior job/child additions plus `AgentChange` (**e070490**) and `ModeChange` (**b80fea8**) | `axo/convert.rs:200,256,341,497,534,563,597`; `axo/live.rs:282`; `axo/mod.rs:627` — **9 errors** | Missing set is `AgentChange`, `ModeChange`, `JobStart`, `ChildStart`, `JobEnd`. Sources: `crates/axo/src/store/entry.rs:264-277,423-482`. Existing exhaustive branch/transcript/bookkeeping matches all need explicit treatment. |
| Job/event surface expanded; `Jobs { jobs }` → `Job(JobInfo)` / `JobGone { id }`, **e070490**, mode events **b80fea8** | `axo/live.rs:265` — **1 error** | Missing `Job`, `JobGone`, `Context`, `AutoPaused`, `Mode`, `ModeResult`, `ModeResultResolved`. `crates/axo/src/session/mod.rs:225-253`. |
| `SubmissionKind::Mail` (**f9f4030**) and `Quiet` (**e070490**) | `axo/live.rs:295` — **1 error** | Exhaustive Prompt/Steer/FollowUp handling no longer sufficient. `crates/axo/src/store/entry.rs:644-652`. |

**Total: 28 production errors.** Compiler diagnostics can conceal additional errors until these are repaired; this is not a claim that 28 edits produce a successful build.

## Test-only breakage

| Root change / commit | Every Axome hit | HEAD source |
|---|---|---|
| `ModelsOptions.http_client` added, **a0409ab** | `axo/tests.rs:65` | `crates/axo-ai/src/models/mod.rs:32-40` |
| Host-owned builder/configuration, **de732d7** | `axo/tests.rs:75` (`builder()` missing arguments), `:76` (removed `.dir(...)`) | `crates/axo/src/axo.rs:60-103,143-151` |
| `Entry::Submission.sender` (**f9f4030**) and `.custom` (**e070490**) | `axo/convert.rs:767` — one diagnostic for both missing fields | `crates/axo/src/store/entry.rs:406-420` |
| `Entry::Message.admitted_jobs` / `.delivered_jobs`, prior background-job work (**74e5128**) | `axo/convert.rs:777,788,900` | `crates/axo/src/store/entry.rs:239-250` |
| `Entry::RunEnd.usage` (**e070490**) and `.queued` (**0782cfc**) | `axo/convert.rs:818,905` | `crates/axo/src/store/entry.rs:383-396` |
| `AssistantMessage.end_turn` removed, **a0409ab** | `axo/convert.rs:877` | Current message definition: `crates/axo-ai/src/types.rs:78`; remove obsolete fixture field. |

**Total: 10 test-only errors.** `convert.rs` fixtures are under `#[cfg(test)]`, starting at lines 741–742.

## New on top of the previous 6cdaeb4 probe

The supplied previous result was **41 errors: 32 lib, 9 tests**; this run reports **38: 28 production locations, 10 test-only locations**. I did not rerun the older probe or assume its diagnostics were identical.

Directly attributable additions since 6cdaeb4:

- New independent `CommandKind::Mode` exhaustiveness failure at `axo/convert.rs:91`.
- Two newly failing `RunEnd` fixtures at `axo/convert.rs:818,905`.
- `custom` joins the already-missing `sender` field in the single Submission fixture diagnostic.
- `AgentChange` / `ModeChange` expand nine already-incompatible journal matches.
- `Quiet` joins the already-unhandled `Mail` submission kind.
- The event match now requires seven new variants rather than the old full-list `Jobs` event.
- Snapshot diagnostics now complain about fields on `Option<String>` because `.agent` denotes a persona name.

**The lower total does not mean less migration work:** compiler diagnostic cascading and changed missing-field shapes prevent interpreting the count delta as a compatibility improvement.

## Requested symbol sweep: hits and non-hits

Searched `src-crates`, `src-tauri/src`, and `src/`.

- **No direct references** to `SessionEvent::Jobs`, `JobInfo`, `JobKind`, `JobStatus`, `tool_hooks`, `ToolHooks`, `ToolFactory`, calls to `.agent()`, or literal `proc://` / `agent://` URLs were found in those Axome trees.
- Thus no Axome direct field accesses to the old `JobInfo` shape, and no hard-coded writes to `proc://.../kill` or `agent://...`, were found.
- `SessionOptions` construction is at `axo/mod.rs:152` and `axo/tests.rs:647`; production construction uses `..Default::default()` at `mod.rs:164`, so new options do not independently fail compilation.
- Relevant new signatures remain material to adoption despite those non-hits:
  - `tools: Vec<HostTool>` and `tool_policies: Vec<Arc<dyn ToolPolicy>>`: `crates/axo/src/session/mod.rs:112-114`, **b80fea8**.
  - `HostTool::direct/deferred`: `crates/axo/src/tools/mod.rs:86-103`.
  - Old `Session::agent` spawn-link accessor → `spawn_link`; new `agent() -> Option<AgentSpec>`: `crates/axo/src/session/mod.rs:1847-1852`, **e070490**.
  - Current per-row job shape: `crates/axo/src/jobs/mod.rs:160-217`.

## Frontend / wire-key findings

### Definite silent incompatibility: `lastLinePartial`

Removed by **a0409ab**, before 6cdaeb4.

- Axome still requires `cut.lastLinePartial === false`: `src/lib/features/chat/elements/tool/payload.ts:27`.
- Test fixture still supplies the removed key: `.../readOutput.test.ts:34`.
- Actual axo truncation JSON has no such field: `crates/axo/src/tools/truncate.rs:58-72`.
- Consumers are `.../readOutput.ts:66` and `.../searchOutput.ts:39`.

Consequently current axo truncation records fail Axome’s validator; the test fixture masks that contract drift.

### Bash result text and metadata

- Footer recognition is hard-coded in `.../bashOutput.ts:20-36`.
- Recognized status strings are limited to exit-code, timeout, aborted, and no-exit-code messages.
- Current stopped messages include:
  - `Command was stopped (kill {id})`
  - `Command was stopped by the user`
  
  Source: `crates/axo/src/jobs/command.rs:600-613`; **b80fea8** changes model-facing stop wording to `kill`.
- Background notice now ends with `Inspect with read proc://{name}; stop with kill.`: `.../jobs/command.rs:388`.
- Leftover-process notice similarly says `stop with kill`: `.../jobs/command.rs:663`.
- Axome does **not** parse these proc/kill strings. They remain ordinary body text rather than recognized status/footer UI.
- Axome reads `details.fullOutput` at `.../bashOutput.ts:5-8`; backend details pass through without typed normalization at `axo/convert.rs:408-410`, protocol `src-crates/axome-protocol/src/chat.rs:181-183`, frontend `src/lib/types/backend.ts:231-233`.

### Tool-name specialization

- Known-tool classification: `src/lib/features/chat/elements/transcript/ToolBlock.svelte:49`.
- Specialized rendering: same file `:56,62,67,75,79`.
- Header switches: `.../tool/toolHeader.ts:46-83`.
- Other direct name gates: `.../tool/ToolDiff.svelte:23`, `searchOutput.ts:78`, `referenceNavigation.ts:51`, `.../transcript/ToolFrame.svelte:116`.
- New `kill`, `send`, and dispatcher `call` have no specialized branches in this code. They fall through to generic argument/output rendering at `ToolBlock.svelte:81-83`; no automatic reuse of the dispatched tool’s specialized renderer exists there.

No frontend build or tests were run; these findings are source-grounded, not runtime UI observations.
