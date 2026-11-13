# axo Pre-Adoption Batch for Axome

- **Target repository:** `/repo/rust/axo`
- **Inspected revision:** `843cd73`
- **Source design & decisions:** [`../design.md`](../design.md) (`D5`, `D6`, `D8`, `D10`, `D15`, `D16`, `D18`, `D20`, `D24`) and [`../plan.md`](../plan.md) (`T10`)

This handoff collects every engine/API change in `axo` decided during Axome's adoption planning so they can land in `axo` in one clean pass before Axome migrates off `26066e2`. All file paths and line numbers below are verified against `/repo/rust/axo` at `843cd73`.

---

## Summary of Items

| # | Area | Summary | Priority |
|---|---|---|---|
| 1 | Prompt sections | Render `AgentSpec.body` in its own `<agent>` section instead of overwriting `<system>` (both root & child) | Blocker (`D5`) |
| 2 | Definition names | Allow `:` namespace separators in mode and skill names (`/plan:roadmap`, `/plan:step`, `/review:adversarial`) | Blocker (`D6`) |
| 3 | Execution sandbox | Native Linux kernel-enforced read-only `bash` mode (Landlock: writable `/tmp` + session `local/`, read-only elsewhere) | Blocker (`D8`, `D19`) |
| 4 | Magic keywords | First-class `KeywordSpec` (`keywords/*.md` + host specs, `Catalog`, per-turn `model` & `thinking` overrides, hidden notice) | Blocker (`D10`) |
| 5 | Tool inspection | Side-effect-free `Session::tools()` getter + host `Schemes::read("tool://<name>")` must not trigger promotion | Blocker (`D15`) |
| 6 | Auto-pause control | Public `Session::resume_wakes()` to clear `auto_paused` and deliver pending job results | Host API (`D16`) |
| 7 | Tree usage rollup | `tree_usage: Usage` on `LiveSnapshot` / `Session::tree_usage()` summing root + all child runs across parking & reopen | Host API (`D20`, `D24`) |
| 8 | Mode review durability | Structured `{ mode, outcome, result }` in `ModeYield` result `details` + outcome on `SessionEvent::ModeResultResolved` | Host API (`D7`, `D24`) |
| 9 | Child open ergonomics | Open or resolve a child session cleanly without forcing the host to manually juggle dummy root handles | Host API (`D20`, `D24`) |
| 10 | Child live tool | Include `current_tool: Option<String>` on `ChildProgress` so parent rows show live tool activity without N streams | Host API (`D18`, `D24`) |
| 11 | `kill` / `send` details | Populate structured `details` on `kill` and `send` `ToolOutput`s so hosts never match prose | Cleanup (`D13`, `D24`) |
| 12 | Live `TeamInfo` | Expose read-only live team state (waiting members, unread mail count, deadlocked flag) on `Session` | Host API (`D18`, `D24`) |

---

## Detailed Specifications & Code Anchors

### 1. Keep `<system>` intact; place `AgentSpec.body` in `<agent>` (`D5`)

**Problem**
In `crates/axo/src/session/prompt.rs:99-110`, any `AgentSpec` with a non-empty `body` (both root personas via `set_agent` and spawned subagents via `Session::spawn`) inserts `"SYSTEM"` into `PromptShape.prompts`, overwriting `prompts/SYSTEM.md` (`crates/axo/src/context/prompt.rs:100-103`). Personas and subagents change how the agent *behaves*, not how the harness *works* — overwriting `<system>` silently drops the harness's persistence/branching, interrupted-tool, steering, and `local://` rules.

**Required change**
- Never insert `AgentSpec.body` under `"SYSTEM"`. `<system>` is replaced **only** by an explicit `SYSTEM` entry in `ConfigLayer.prompts` (`SYSTEM.md`) or a host `set_section("system", ...)`.
- Render a non-empty `AgentSpec.body` as its own `<agent>` section (e.g. immediately after `system` in `build_sections`, `crates/axo/src/context/prompt.rs:17-20,107-108`) on **both** root sessions and child sessions:
  - Root with persona: `<system>` → `<agent>` → `<docs>` → `<tools>` → …
  - Child subagent: `<delegation>` → `<system>` → `<agent>` → `<docs>` → `<tools>` → …
- Add `agent` to the wrapper-tag sanitizer regex in `escape_wrappers` (`crates/axo/src/context/prompt.rs:26-30`).
- Keep `spec.sections` (`sections: { prose: false }`, etc.) working as before (`session/prompt.rs:111-116`).

---

### 2. Allow `:` namespace separators in mode & skill names (`D6`)

**Problem**
`crates/axo/src/context/markdown.rs:53-77` (`name_problems`) restricts definition names to `[a-z0-9-]` only. Axome's built-in planning modes (`/plan`, `/plan:roadmap`, `/plan:step`) and user workflows (`/review:adversarial`, `/decomplect:staged`) group command families with `:`.

**Required change**
- Allow single `:` separators between valid `[a-z0-9-]+` segments in mode and skill names (no leading/trailing `:`, no consecutive `::`, no `-:` or `:-`).
  - Note: if `AgentSpec` names should stay `: `-free (or if `:` is only for modes/skills because model selectors use `:thinking`, though agent names are never suffixed with `:thinking`), either allow `:` on modes & skills specifically or across markdown definitions where unambiguous; verify `Session::parse_command` (`crates/axo/src/session/mod.rs:2000-2027`) and `commands::catalog` (`crates/axo/src/commands.rs:336-367`) handle `/plan:roadmap [prompt]` cleanly (whitespace split in `parse_command` already keeps `plan:roadmap` intact).
- For filesystem discovery where filenames may live in subdirectories (e.g. `modes/plan/roadmap.md` vs `modes/plan:roadmap.md` or explicit frontmatter `name: plan:roadmap`), ensure frontmatter `name: plan:roadmap` always works (and optionally support `modes/plan/roadmap.md` deriving `plan:roadmap` if desired, or explicit `name:`).

---

### 3. Native Linux kernel-enforced read-only `bash` mode (`D8`, `D19`)

**Problem**
Mode `ToolLimits` (`crates/axo/src/context/modes.rs:35-65`) and `AgentSpec.tools` (`crates/axo/src/context/agents.rs:46-47`) filter tools by name only. Read-only workflows (`/plan*`, `/research`, and `scout` subagents) need `bash` for inspection (`git log`, `git diff`, `git blame`, scripts) while guaranteeing zero mutations to the repository or user home directory, with **no interactive permission prompts**.

**Required change**
- Add a kernel-enforced read-only filesystem execution mode for local commands in `crates/axo/src/exec/host.rs` (Linux Landlock LSM via `rustix`/syscalls or `landlock`, applied in the child/supervisor `enter` path before `exec`):
  - Entire filesystem read/execute-only (`LANDLOCK_ACCESS_FS_READ_FILE | READ_DIR | EXECUTE`), **except** read-write access granted to `/tmp` (and `$TMPDIR` if under `/tmp`), `/dev/null`, and the session's `local/` scratch directory (`<session_dir>/local`).
  - Fail closed with a clear `ToolError` if read-only enforcement was requested by the mode/agent and the kernel cannot enforce it (Linux 5.13+ has Landlock; workstation is Linux 7.3).
- Expose this declaratively in frontmatter on both **`ModeSpec`** (`crates/axo/src/context/modes.rs`) and **`AgentSpec`** (`crates/axo/src/context/agents.rs`) — e.g. `bash: readOnly` (or `sandbox: readOnly` / `tools.bash: readOnly`).
- Propagate as a sticky ceiling into descendants in `SpawnLink` (`crates/axo/src/store/entry.rs:103-111` & `crates/axo/src/session/spawn.rs:324-335`): a child spawned while the parent is in a read-only-bash mode (or from a read-only-bash parent) inherits the read-only `bash` ceiling even if the child's own `AgentSpec` did not set it.

---

### 4. Magic keywords architecture (`keywords/*.md` + `Catalog`) (`D10`)

**Problem**
`axo` has no magic-keyword support yet. Hosts like Axome want standalone prose words (such as `orchestrate`, `ultrathink`, `chillax`) defined in `.md` files (host-bundled, user `~/.config/axome/keywords/*.md`, and trusted project `.axome/keywords/*.md`) to inject turn-scoped instructions and optionally switch model/thinking for that turn.

**Required change**
- **Definition format (`KeywordSpec`):** Markdown with YAML frontmatter (sharing `context/markdown.rs` walker & frontmatter splitter):
  - `name` / `word`: lowercase trigger word (defaults to file stem).
  - `description`: human-readable summary for settings UI.
  - `requiresTools`: `Vec<String>` (default empty) — keyword only triggers when all listed tools are active in the session (e.g. `requiresTools: [spawn]` for `orchestrate`).
  - `model`: `Option<String>` — optional model selector (`@conduct`, `provider/id[:thinking]`, `:thinking`) applied for the turn (or switched via model-restore semantics; clarify/implement clean turn or restore behavior so `orchestrate` can run on `@conduct`!).
  - `thinking`: optional effort directive (`max` | `min` | explicit `ThinkingLevel`) for that turn.
  - `extra`: `serde_json::Map<String, Value>` (for host UI metadata such as highlight `hue`).
  - `body`: Markdown notice text injected when triggered.
- **Configuration & Catalog:**
  - Add `keyword_roots: Vec<PathBuf>` to `ConfigLayer` and `EffectiveConfig` (`crates/axo/src/config.rs:401-452`), honoring project `keyword_roots` only at `ProjectAccess::Trust` (matching `agent_roots` and `mode_roots`).
  - Add `keywords: Vec<KeywordSpec>` to `SessionOptions` (`crates/axo/src/session/mod.rs`) for host-bundled defaults (project > user > programmatic, first name wins, so `.axome/keywords/` can override bundled `orchestrate.md`).
  - Add settings switches under `Settings` (`keywords.enabled: bool`, per-keyword `keywords.disabled` / map) so users can toggle keywords in `settings.jsonc`.
  - Expose `keywords: Vec<Arc<KeywordSpec>>` on `Catalog` (`crates/axo/src/axo.rs:48-54,294-315`) and `Session::keywords(&self)` so Axome can highlight active keywords in the ProseMirror composer and render toggles in Settings.
- **Matching & Injection:**
  - Match only standalone exact-lowercase prose words on user-submitted prompts/steers/follow-ups (after command/skill expansion; never on synthetic/agent/mail/job wakes), ignoring fenced code blocks, inline `` `code` ``, HTML/XML tags, paths (`foo/orchestrate`), identifiers (`foo_orchestrate`, `foo::orchestrate`), and call syntax (`orchestrate()`).
  - Inject a hidden (`display: false`) reserved custom message (`keyword_notice`, added to reserved custom types in `session/delivery.rs:25-35` and kept intact by `/shake` & compaction cut-point rules like `mode_notice`) for that turn.

---

### 5. Side-effect-free `Session::tools()` & `Schemes::read("tool://<name>")` (`D15`)

**Problem**
1. `Session` has no public method listing its current direct, deferred, and promoted tools (`SessionHooks::catalog()` and `declared_tools()` in `crates/axo/src/session/surface.rs:21-57` are `pub(super)`).
2. Calling `session.schemes().read(&SchemeUrl { scheme: "tool", path: "<name>" })` in `crates/axo/src/schemes/tool.rs:52` unconditionally calls `self.deferred.request(name)`, mutating session state by queuing `<name>` for promotion!

**Required change**
- Add a side-effect-free public `Session::tools(&self) -> Vec<SessionToolInfo>` returning every tool in the session's current surface with:
  - `name`, `summary`, `description`, `parameters` (`Value`), `exposure` (`Direct | Deferred`), `promoted: bool`, and `allowed_in_mode: bool` (against current selected/effective mode limits).
- Separate model-initiated `read` tool reads from host `Schemes::read` calls so host inspection of `tool://<name>` does **not** call `self.deferred.request(name)` (only model tool execution requests promotion).

---

### 6. `Session::resume_wakes()` for `auto_paused` (`D16`)

**Problem**
When a run errors or is aborted by the user while background jobs/children are active, `auto_paused` is set (`crates/axo/src/session/delivery.rs:103`) and `SessionEvent::AutoPaused { paused: true }` fires. Currently `auto_paused` only clears as a side effect of sending a new user prompt; there is no host method to explicitly unpause and flush waiting job results.

**Required change**
- Add `pub fn resume_wakes(&self)` on `Session` (`crates/axo/src/session/mod.rs` / `delivery.rs`): clears `auto_paused` (emitting `SessionEvent::AutoPaused { paused: false }` if it was true) and calls `JobHost::results_ready(&*self.hooks)` so any finished `bg_N` / `agent_N` messages deliver immediately if the session is idle.

---

### 7. Tree usage rollup (`tree_usage: Usage`) (`D20`, `D24`)

**Problem**
`LiveSnapshot.usage` (`crates/axo/src/session/mod.rs:329`) includes only the root session's own requests. Summing whole-tree spend in the host requires chasing parked/closed child `.jsonl` files, deduplicating multiple `JobEnd.usage` entries across child follow-up runs against live `ChildProgress.usage`, and handling nested grandchildren (`maxDepth > 1`).

**Required change**
- Track and expose `tree_usage: Usage` (root `usage` + cumulative descendant `usage`, both finished child runs from `JobEnd.usage` and in-flight `ChildProgress.usage` without double counting) on `LiveSnapshot` and a `Session::tree_usage(&self) -> Usage` getter (reconstructed accurately on `open_session` from journaled `JobEnd.usage` entries + live child progress).

---

### 8. Durable & typed mode review resolution (`D7`, `D24`)

**Problem**
In `crates/axo/src/session/modes.rs:414-418,534-578`:
1. When `ModeYield` finishes (`Accepted` or `Revise`), its `ToolOutput` leaves `details: None`. On session reopen, a host reconstructing transcript history has only plain English tool output text (`"Result accepted. Mode ..."` vs `"The result was not accepted..."`), not a typed outcome or the accepted `Value`.
2. `SessionEvent::ModeResultResolved { call_id }` (`crates/axo/src/session/mod.rs:248-250`) carries only `call_id`, not how the review resolved.

**Required change**
- Populate structured `details` on `ModeYield`'s `ToolOutput` (`{ "mode": self.mode.name, "outcome": "accepted" | "revised", "result": result }`, and on error/dismiss if applicable or via typed enum) so journaled `ToolResultMessage` entries preserve the exact review outcome and value across reopen.
- Include the resolution outcome (`Accepted`, `Revised`, `Dismissed`, `Superseded`) on `SessionEvent::ModeResultResolved`.
- Verify with an integration test in `crates/axo/tests/modes.rs` that closing and reopening a session (`RecoveryPolicy::Resume`) while `ModeYield` (`review: true`) is awaiting review cleanly restores `LiveSnapshot.pending_results` and allows `accept_result` / `revise_result` to complete the resumed turn.

---

### 9. Clean child-session opening when the root is closed (`D20`, `D24`)

**Problem**
`Axo::open_session` (`crates/axo/src/axo.rs:189-195`) rejects child session files with `Error::ChildSession { path, root }`, requiring the host to open `root` first, hold the root `Session` alive in host memory to prevent `Drop` from tearing down `SessionTable` entries, and then call `Axo::session(child_id)`.

**Required change**
- Provide a clean host entry point on `Axo` (or extend `Axo::open_child` / `open_session_read` + child projection helpers) so a host inspecting or resuming a child session from history does not have to manually walk `Error::ChildSession` and manage phantom root lifetimes itself (or document/provide an `Axo`-managed root+child open handle if interactive steering requires the live root).

---

### 10. `current_tool` on `ChildProgress` (`D18`, `D24`)

**Problem**
`ChildProgress` (`crates/axo/src/jobs/mod.rs:198-205`) currently exposes `{ turns, max_turns, wrapping_up, yield_failures, usage }` and updates only after each child assistant message completes (`crates/axo/src/session/mod.rs:1244-1260`). A parent with 8 concurrent scouts cannot show `"reading foo.rs"` / `"running grep"` on collapsed sidebar rows without attaching 8 separate `SessionEvent` broadcast streams.

**Required change**
- Add `pub current_tool: Option<String>` (or active tool name(s), e.g. `"read"`, `"bash"`) to `ChildProgress` in `crates/axo/src/jobs/mod.rs`, updated when child tool execution starts/ends and pushed via the parent's `SessionEvent::Job(JobInfo)`.

---

### 11. Structured `details` on `kill` and `send` tool results (`D13`, `D24`)

**Problem**
`KillTool` (`crates/axo/src/tools/kill.rs:57-65`) and `SendTool` (`crates/axo/src/tools/send.rs:182-186`) return `details: None`. In particular, `kill` returns `is_error: false` for both `"Stopping bg_1."` and `"bg_1 is not running."`, forcing hosts to match English text to distinguish a real stop from a no-op.

**Required change**
- `KillTool`: set `details: Some(json!({ "id": id.to_string(), "stopped": stopped }))`.
- `SendTool`: set `details: Some(json!({ "to": to, "kind": "steer" | "followUp" | "mail" | "note" }))` corresponding to the route taken in `SendTool::send`.

---

### 12. Read-only `TeamInfo` live state on `Session` (`D18`, `D24`)

**Problem**
`TeamState` (`crates/axo/src/session/team.rs:12-25`) tracks which members are currently blocked in `wait` (`waiting: HashSet<String>`) and deadlock state, and each member's job registry tracks pending unread mail (`jobs.mail_ids()`), but all of this is `pub(crate)` / private. Parent rows only expose static `TeamRef { id, member }`.

**Required change**
- Expose a read-only query (e.g. `Session::teams(&self) -> Vec<TeamInfo>` on a parent session, or team status fields) reporting each spawned team's `id`, `name`, member roster (`member`, `session_id`, `job_id`), which members are currently waiting, unread mail counts per member, and whether mutual-wait deadlock triggered.

---

## Verification Gate in `/repo/rust/axo`

Before bumping Axome's Cargo pin to the new `axo` commit:
1. `nix develop --command cargo check --workspace --all-targets`
2. `nix develop --command cargo test --workspace` (including offline `ScriptedApi` tests covering `<agent>` section preservation, `:` mode/skill names, Landlock read-only `bash` `EROFS` on repo writes + writable `/tmp` & `local/`, keyword matching & `@conduct` model selection, side-effect-free `Session::tools()`, `resume_wakes()`, `tree_usage`, mode review reopen, `ChildProgress.current_tool`, `kill`/`send` details, and `TeamInfo`)
3. `nix develop --command cargo clippy --workspace --all-targets -- -D warnings`
4. `nix develop --command cargo fmt --check`
