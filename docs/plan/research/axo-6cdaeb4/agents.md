# axo 6cdaeb4: agents, children, teams

Raw survey report, kept verbatim as evidence. Produced by a read-only subagent on 2026-11-09 against axo `6cdaeb4`, verified against source code (not docs). Superseded for persona, catalog and child rows by `../axo-843cd73/host-surface.md`; still useful for definition format, spawn, teams and storage.

Citation prefixes are defined inside the report. Claims marked [INFERENCE] were not observed directly.

---

# Track 2 Agents Survey: axo 26066e2 → HEAD (6cdaeb4)

This survey details the additions and changes in axo across commits `e8f33d9`, `ce1feb2`, `35ec4e8`, `f9f4030`, and `6cdaeb4`. Every claim and line number is verified directly against axo source code at HEAD (`/repo/rust/axo/crates/*/src`) and Axome source code at HEAD (`/repo/rust/Axome/src-crates/` and `src/`).

---

## 1. What axo Now Provides in Track 2

### 1.1 Model Selectors & Resolution
- **Selector Syntax (`crates/axo/src/config.rs:136-200`)**:
  - `provider/model`: Literal provider and model ID (e.g. `anthropic/claude-3-5-sonnet`). Resolved via `Models::find` before checking for `:thinking` suffixes, so models whose IDs contain colons match intact (`config.rs:185-188`).
  - `@role`: Named role defined in user settings `modelRoles`. Strips `@` and expands through `Settings::model_roles` (`config.rs:138-164`). Chained role references (role pointing to `@another`) are explicitly rejected with `Error::InvalidModelSelector` (`config.rs:159-164`).
  - `:thinking`: A thinking level alone (`:low`, `:medium`, `:high`, etc.) without a model, selecting thinking level while leaving the model unset (`config.rs:172-184`).
  - Combined `[@role | provider/model]:thinking`: Parsed by stripping the `:thinking` suffix from either the role selector or the expanded role target; the selector's explicit suffix takes precedence over the role's default thinking (`config.rs:142-154`, `165-167`).
- **`Settings::resolve_model` & `ModelChoice` (`crates/axo/src/config.rs:98-103, 137-168`)**:
  - `pub struct ModelChoice { pub model: Option<Model>, pub thinking: Option<ThinkingLevel> }` (`config.rs:99-103`).
  - `pub fn resolve_model(&self, models: &Models, selector: &str) -> Result<ModelChoice, Error>` (`config.rs:137`).
  - Cascaded resolution via internal `first_model(&self, models, selectors)` (`config.rs:115-134`): picks the first model found among selectors, retaining the first thinking level encountered.
- **User-Only `modelRoles` Setting (`crates/axo/src/config.rs:27-28, 461`)**:
  - `pub model_roles: IndexMap<String, String>` on `Settings`.
  - Enforced in `USER_ONLY_KEYS = &["project", "homeAgentsDir", "modelRoles", "subagents"]` (`config.rs:461`). Ignored if present in project-level configuration layers.
- **`/model` Built-in Command Handling (`crates/axo/src/session/mod.rs:1696-1721`)**:
  - `/model <selector>` parses into `Command::Model { model: String }`.
  - Resolved dynamically via `settings.resolve_model(&self.inherited.models, &model)?`.
  - If `choice.model` is `Some(model)`, calls `hooks.select_model(&agent, model, choice.thinking)`.
  - If `choice.model` is `None` (e.g. `/model :high`), falls back to `self.set_thinking(choice.thinking.unwrap())`.

### 1.2 Agent Definition File Format & Catalog
- **Definition File Parsing (`crates/axo/src/context/agents.rs:14-46, 142-174, 280-365`)**:
  - YAML frontmatter parsed between `---` fences via `split_frontmatter` (`markdown.rs:20-56`).
  - Frontmatter fields for `AgentSpec` (`agents.rs:17-46`):
    - `name: Option<String>` (defaults to filename stem, validated via `valid_name` / `name_problems`: lowercase alphanumeric + hyphens/underscores).
    - `description: String` (defaults to empty string).
    - `model: Option<String>` (selector: `provider/model`, `@role`, optional `:thinking`).
    - `tools: Option<ToolList>` (YAML array of strings or comma-separated CSV string; allowlist of tool names).
    - `contextFiles: Option<ContextFiles>` (`false` for `ContextFiles::Off`, or a map of `String -> bool` for `ContextFiles::Names`).
    - `sections: IndexMap<String, bool>` (`false` disables prompt section; validated with `valid_section_name`).
    - `output: Option<Value>` (JSON Schema; validated at load time with `jsonschema::validator_for`; used by child sessions via `yield`).
    - `maxTurns: Option<u32>` (overrides child turn budget; ignored for root sessions).
    - Ignored/deprecated keys with warnings: `blocking`, `spawns`, `thinkingLevel`.
    - Body: Markdown prompt instructions after frontmatter (trimmed).
  - Frontmatter fields for `TeamSpec` (distinguished by the presence of `members:` in frontmatter mapping; `agents.rs:89-120, 276-288`):
    - `name: Option<String>`, `description: String`.
    - `members: Vec<RawMember>` where each member has `agent: String` (must name an agent in catalog), `as: Option<String>` (alias for roster name; `parent` is reserved), `brief: Option<String>` (standing instructions).
    - Body: Team charter prompt.
- **`AgentSpec` & `InlineSpec` Types (`crates/axo/src/context/agents.rs:17-46`, `crates/axo/src/session/spawn.rs:76-85`)**:
  - `AgentSpec`: Persisted in session headers and child spawn links.
  - `InlineSpec`: Dynamic ad-hoc agent spec used in `SpawnRequest` or `spawn` tool (`prompt: String`, `tools: Option<Vec<String>>`, `model: Option<String>`, `output: Option<Value>`).
- **`ConfigLayer::agent_roots` & Discovery (`crates/axo/src/config.rs:410, 446, 481-498`)**:
  - `pub agent_roots: Vec<PathBuf>` on `ConfigLayer` and `EffectiveConfig`.
  - Security gate: Project `agent_roots` are only honored when `ProjectAccess == ProjectAccess::Trust` (`config.rs:484-488`).
- **`SessionOptions.agent` & `SessionOptions.agents` (`crates/axo/src/session/mod.rs:84-88`)**:
  - `pub agent: Option<String>`: Names the root agent definition.
  - `pub agents: Vec<AgentSpec>`: Host programmatic agent definitions loaded alongside filesystem agent roots (`axo.rs:288`).
- **Root Agent as Persona (`crates/axo/src/session/mod.rs:1200-1240, 1269-1288, 1345-1358`)**:
  - Overrides `SYSTEM` prompt body (`prompts.insert("SYSTEM", PromptOverride { text: spec.body })`).
  - Restricts available tools via `tools.retain(|tool| allow.contains(tool.name))`.
  - Disables prompt sections listed as `false` in `spec.sections`.
  - Overrides context files (`ContextFiles::Off` disables all context files).
  - Supplies default model in cascade if session and caller specify none (`axo.rs:295-305`).
- **Fork Inheritance (`crates/axo/src/session/mod.rs:1946-1955`)**:
  - `Session::fork` passes `self.inherited.clone()`, so forks inherit the parent's `Inherited.agent`. Reopening with `Axo::open_session` treats it as a fresh root based on `SessionOptions.agent`.
- **`Session::agents(&self) -> &[AgentDef]` (`crates/axo/src/session/mod.rs:1658-1660`)**:
  - Returns the combined slice of catalog agents and teams loaded for the session.

### 1.3 Harness Name (`AxoBuilder::name`)
- **API (`crates/axo/src/axo.rs:75-80`, `crates/axo/src/schemes/mod.rs:26-38`)**:
  - `AxoBuilder::name(mut self, name: impl Into<String>) -> Result<Self, Error>`
  - Validates `name` against `^[a-z][a-z0-9_-]{0,31}$` and rejects reserved scheme names (`dir`, `local`, `session`, `artifact`, `proc`, `skill`, `agent`, etc.).
  - Replaces the default `"axo"` in:
    1. Docs URL scheme: `<name>://` (materialized docs via `DirScheme`, `session/mod.rs:1247`).
    2. Prompt `docs` section heading and text (`context/prompt.rs:47, 169`).
    3. Bundled `SYSTEM.md` and `SUMMARY.md` prompts (`session/mod.rs:1267`, `compaction/summary.rs:35-43`).

### 1.4 Subagents Settings
- **`SubagentSettings` (`crates/axo/src/config.rs:64-95`)**:
  User-only settings (`USER_ONLY_KEYS`) controlling child execution:
  - `pub max_depth: u32` (default `1`): Depth limit. Root = 0, child = 1.
  - `pub max_concurrent: usize` (default `8`): Process-wide/root semaphore slots for child runs streaming simultaneously. Clamped to `max(1)` (`axo.rs:348-350`, `spawn.rs:305`).
  - `pub max_turns: u32` (default `50`): Turn budget per child run. At `max_turns`, wrap-up message injected; on next turn, non-yield tools blocked.
  - `pub default_model: Option<String>` (default `None`): Fallback selector for children when neither agent nor request specifies a model.
  - `pub models: IndexMap<String, String>` (default empty): Map of agent catalog name to model selector override.
  - `pub context_files: Option<ContextFiles>` (default `Some(ContextFiles::Off)`): Subagents disable parent context files by default.

### 1.5 Child Sessions & Spawning Mechanics
- **Public API (`crates/axo/src/session/spawn.rs:188-193`, `mod.rs:1579-1582`)**:
  - `Session::spawn(&self, request: SpawnRequest) -> Result<ChildHandle, Error>`
  - `Session::spawn_team(&self, request: TeamRequest) -> Result<TeamHandle, Error>`
  - `Session::agent(&self) -> Option<&store::SpawnLink>`: Returns `SpawnLink` for child sessions, `None` for roots/forks.
- **Request & Handle Types (`crates/axo/src/session/spawn.rs:28-74`)**:
  - `SpawnRequest { agent: Option<String>, spec: Option<InlineSpec>, task: PromptInput, model: Option<String>, output: Option<Value> }`
  - `TeamRequest { team: Option<String>, members: Vec<TeamMemberRequest>, charter: Option<String>, tasks: IndexMap<String, PromptInput> }`
  - `TeamMemberRequest { name: String, agent: Option<String>, spec: Option<InlineSpec>, brief: Option<String> }`
  - `ChildHandle { pub job: String, pub session: Arc<Session> }` (where `job` is `agent_N`)
  - `TeamHandle { pub team: String, pub members: Vec<(String, ChildHandle)> }` (where `team` is `team_N`)
- **Spawn Hooks & Policy (`crates/axo/src/session/spawn.rs:93-138`)**:
  - `pub trait SpawnHook: Send + Sync { async fn before_spawn(&self, spawn: &SpawnContext<'_>) -> SpawnDecision; }`
  - `SpawnOrigin`: `Host` or `Tool { call_id: String }`.
  - `SpawnTeam<'a> { name: Option<&'a str>, member: &'a str }`.
  - `SpawnContext<'a>` carries `spec`, `agent`, `model`, `thinking`, `task`, `depth`, `parent_session_id`, `origin`, `team`.
  - `SpawnDecision`: `Allow`, `Block(String)`, or `Replace(AgentSpec)`.
- **Storage, File Layout, & Lazy Creation (`crates/axo/src/store/jsonl.rs:124-148`, `session/spawn.rs:379-391`)**:
  - `Header.spawn: Option<SpawnLink>` (`store/entry.rs:77, 80-99`).
  - `Header.parent_session` is `None` for children (used only for forks, `spawn.rs:379`).
  - Child files are created **lazily** on first submission/entry (`store/jsonl.rs:136-147`).
  - Child files live in the same directory as root sessions: `<session_dir>/<timestamp>_<uuid>.jsonl`.
  - `SessionInfo::spawn` exposes `Option<SpawnLink>` in session listings (`store/entry.rs:572`).
- **Revival & Parking (`crates/axo/src/session/children.rs:30-44, 139-160, 190-205, 324-348`, `axo.rs:198-202`)**:
  - `Axo::session(&self, id: &str) -> Result<Arc<Session>, Error>`: Returns live or revives parked child. Children must be known to an open parent in the process (`axo.rs:199`). Direct call to `Axo::open_session` on a child file fails with `Error::ChildSession` (`axo.rs:176-179`).
  - Idle children park via `SessionTable::park` (`children.rs:345`), dropping their writer lock. Reviving reacquires the lock and recovers unfinished runs.
  - Hosts can call `child_session.attach(from).await` to stream events from any child session identically to a root session (`session/mod.rs:1805`).

### 1.6 Tools, Protocols & Team Coordination
- **`spawn` Built-in Tool (`crates/axo/src/session/spawn.rs:756-960`)**:
  - Offloaded to models when depth < `subagents.max_depth` (`session/mod.rs:1359`).
  - Schema parameters: `agent` (string), `spec` (object), `task` (string), `tasks` (map), `members` (array of objects), `charter` (string), `context` (string), `model` (string), `output` (object schema).
  - Returns immediate tool result with `agent_N` or `team_N (member agent_N, ...)` (`spawn.rs:920-952`).
- **`yield` Built-in Tool & Output Schemas (`crates/axo/src/tools/output.rs:13-138`)**:
  - Injected when child has `AgentSpec.output` (`session/mod.rs:1362-1367`).
  - Wraps non-object schemas in `{ "type": "object", "properties": { "result": schema }, "required": ["result"] }` (`output.rs:25-33`).
  - Model nudged once (`yield_nudge`) if it emits a text-only turn without calling `yield` (`output.rs:62-72`).
  - `MAX_YIELD_FAILURES = 3`: 3 rejected yields fail the run (`output.rs:16, 63`).
  - Calling `yield` terminates the run with `terminate: true` (`output.rs:135`).
  - Exempt from the turn-budget block (`jobs/mod.rs:88-92`).
- **`agent://` Virtual URL Scheme (`crates/axo/src/schemes/agent.rs:1-309`)**:
  - `read agent://agent_N`: Status report (name, model, session id, status, turns, latest text/result).
  - `read agent://agent_N/result`: Raw text result of child run.
  - `read agent://agent_N/result/<json-pointer>`: Extracted JSON field from result (`agent.rs:149-166`).
  - `read agent://agent_N/transcript`: Conversation history formatted via `render_conversation` (`agent.rs:168-175`).
  - `write agent://agent_N`: Queues a follow-up submission (`agent.rs:214-220`).
  - `write agent://agent_N/steer`: Queues a steer submission (`agent.rs:207-213`).
  - `write agent://parent`: Sends mid-run note directly to parent registry (`agent.rs:222-249`).
  - `write agent://<member_name>`: Sends teammate mail (`agent.rs:250-264`).
- **Team Coordination & Mutual-Wait Deadlock Detection (`crates/axo/src/session/team.rs:1-167`)**:
  - Prompt section: Adds `<team>` prompt section containing member name, charter, brief, and roster (`team.rs:133-167`).
  - Mutual wait detection: When a member enters `wait`, `TeamState::deadlocked` checks if all team members are either non-live or waiting/idle with no pending messages and no running admitted jobs (`team.rs:80-112`).
  - If deadlocked, `wait` fails with error: `"Mutual wait: every running member of {team} is waiting and nothing is pending, so no one can reply..."` (`team.rs:125-131`).

### 1.7 Result Delivery, Caps, and Queue Drain
- **Child Run Result Delivery (`crates/axo/src/jobs/child.rs:36-53, 100-205`, `jobs/mod.rs:78-105`)**:
  - Delivery identifiers: First run delivers as `agent_N`; subsequent runs (from follow-ups/steers/mail) deliver as `agent_N/2`, `agent_N/3`, etc. (`jobs/mod.rs:98-105`).
  - Content capping: Capped at 2000 lines / 50 KB (`truncate_head(Limits::default())`, `child.rs:36-53`). Truncated results append pointer: `[Showing lines 1-X of Y. Full result: agent://{job}/result]`.
  - Wrapper tag escaping: Embedded XML wrapper tags (`<system>`, `<conversation>`, `<delegation>`, etc.) are escaped with `escape_wrappers` to prevent prompt injection into parent (`session/delivery.rs:13-14`).
- **`Agent::take_next_input` (`crates/axo-agent/src/agent.rs:260-266`)**:
  - Replaces `take_steering` and `take_follow_ups`.
  - When starting a run from idle, takes steering queue if non-empty, else takes follow-up queue.
- **Fixed Delegation Framing (`crates/axo/prompts/DELEGATION.md`)**:
  - Fixed framing injected as `<delegation>` section for all child sessions (`context/prompt.rs:181-185`). Children have `ask` disabled and omit host prompt sections.

---

## 2. Breaking Changes Hitting Axome's Current Code

| Axome File & Line | What Breaks | Root Cause in axo | Required Axome Replacement |
| :--- | :--- | :--- | :--- |
| `src-crates/axome-core/src/services/axo/live.rs:265-462` | Compile error: non-exhaustive patterns in `match event` | `SessionEvent` added `Jobs { jobs: Vec<JobInfo> }` (`session/mod.rs:218-220`). `live.rs` matches exhaustively without `_ =>`. | Add `SessionEvent::Jobs { .. } => {}` or handle `SessionEvent::Jobs` in `live.rs`. |
| `src-crates/axome-core/src/services/axo/live.rs:296-300` | Compile error: non-exhaustive patterns in `match kind` | `SubmissionKind` added `Mail` (`store/entry.rs:438`). `live.rs` matches `Prompt`, `Steer`, `FollowUp` explicitly. | Add `SubmissionKind::Mail => None` (or map to a new wire queue kind). |
| `src-crates/axome-core/src/services/axo/mod.rs:137-140` | Compile error: `Axo::builder()` takes 2 arguments | Commit `de732d7` updated `Axo::builder(models: Arc<Models>, store: Arc<dyn SessionStore>)` (`axo.rs:132`). | Pass `Arc<Models>` and `Arc<dyn SessionStore>` into `Axo::builder(...)`. |
| `src-crates/axome-core/src/services/axo/mod.rs:152-164` | `SessionOptions` missing Track 2 fields | `SessionOptions` gained `agent: Option<String>`, `agents: Vec<AgentSpec>`, `spawn_hooks: Vec<Arc<dyn SpawnHook>>` (`session/mod.rs:84-106`). | Currently compiles due to `..Default::default()`, but ignores agent personas, programmatic specs, and spawn policy. |
| `src-crates/axome-protocol/src/chat.rs:33-38` | Wire catalog cannot represent model roles | `ChatModelCatalog` only has concrete provider models (`models: Vec<ChatModel>`). Cannot represent `@role` selectors or role thinking defaults. | Extend `ChatModelCatalog` to include `model_roles: IndexMap<String, String>` and expose roles in the frontend picker. |

---

## 3. Silent Behavior and Format Changes

1. **New JSONL Stored Entry Types (`crates/axo/src/store/entry.rs:379-407`)**:
   - `child_start`: Written when a child is spawned. Carries `session_id`, `job_id` (`agent_N`), `child_session_id`, `child_path`, optional `agent`, `model`, and optional `team`.
   - `job_start`: Written when a command is promoted to a background job (`bg_N`).
   - `job_end`: Written when a job (command or child run) finishes.
   - Axome's `live.rs:280-388` falls through to `_ => {}` for these entries. They will not crash Axome, but child spawns and job results are completely invisible in the chat transcript.
2. **`Message` Entry Background Job Receipts (`crates/axo/src/store/entry.rs:223-228`)**:
   - `Entry::Message` now carries `admitted_jobs: Vec<String>` and `delivered_jobs: Vec<String>`. Axome's deserializer ignores them, but they affect transcript reconstruction if Axome expects standard message lines.
3. **`CustomMessage` Delivered Job Results (`crates/axo/src/jobs/mod.rs:431-441`)**:
   - Finished child results are delivered to the parent as `AgentMessage::Custom(CustomMessage { custom_type: "job_result", display: true, ... })`.
   - In Axome's `live.rs:334`, `AgentMessage::Custom(_)` is a no-op. If the frontend only renders LLM user/assistant messages, delivered subagent results will not appear in the UI.
4. **Child Sessions Cannot Be Opened Directly (`crates/axo/src/axo.rs:176-179`)**:
   - `Axo::open_session` fails with `Error::ChildSession` if `header.spawn.is_some()`.
   - If Axome's session list (`list_sessions`) includes child sessions and the user clicks on one in the sidebar, calling `open_session` will fail with an error dialog unless Axome filters them or uses `Axo::session(id)`.
5. **Prompt Text and Framing (`crates/axo/prompts/DELEGATION.md`, `context/prompt.rs:181-185`)**:
   - Child sessions receive non-overridable `<delegation>` framing: "You are a subagent: another agent delegated a task to you, and no user is watching this conversation..."
   - Subagents omit all host prompt sections (`host_sections: Vec::new()`) and disable `ask`.

---

## 4. Responsibility Split: axo vs Host (Axome)

| Responsibility | Owned by `axo` | Owned by Host (`Axome`) |
| :--- | :--- | :--- |
| **Model Resolution** | Expanding `@role`, parsing `:thinking`, cascading defaults (`config.rs:137-200`). | Supplying credentials in `auth.json`/env; providing role picker UI in frontend. |
| **Agent Definitions** | Scanning `agent_roots` markdown files, frontmatter parsing, validation, team roster checks (`context/agents.rs`). | Providing user/project `agent_roots` paths via `ConfigLayer`; authoring agent markdown files. |
| **Root Agent (Persona)** | Overriding `SYSTEM.md`, applying tool allowlists, disabling sections, turning off context files (`session/mod.rs:1200-1358`). | Selecting `SessionOptions.agent` at session creation/open; providing UI to pick agent persona. |
| **Child Execution** | Task preparation, asynchronous background execution, child turn budget, parking/revival (`session/children.rs`). | Regulating concurrency if desired via `subagents.maxConcurrent`; subscribing to events. |
| **Policy Enforcement** | Invoking `SpawnHook` before child creation; checking `max_depth` (`session/spawn.rs:93-138`). | Implementing `SpawnHook` to prompt user for approval or rewrite child specs/models. |
| **Job Registry & Delivery** | Allocating `agent_N`, buffering results, delivering via `agent_N` or `agent_N/2`, capping at 50 KB (`jobs/mod.rs`). | Rendering job status badges and child progress in the chat workbench. |
| **Virtual Schemes** | Serving `agent://` (status, result, JSON pointer, transcript, steer, follow-up, mail) (`schemes/agent.rs`). | Resolving `agent://` links in chat message text when user clicks them. |

---

## 5. Host Observability

### Push Events (via `Session::attach` / `SessionEvent`)
Hosts subscribe by calling `session.attach(from).await` (`session/mod.rs:1805`):
1. **`SessionEvent::Jobs { jobs: Vec<JobInfo> }` (`session/mod.rs:218-220`)**:
   - Pushed whenever any job starts, updates phase, or settles (`delivery.rs:388-399`).
   - `JobInfo` (`jobs/mod.rs:152-157`):
     - `id: String`: e.g. `"bg_1"`, `"agent_1"`.
     - `status: JobStatus`: `Running`, `HelpersRunning`, `Finished`, `Failed`.
     - `kind: JobKind`:
       - `JobKind::Command { command: String, artifact: u64 }`
       - `JobKind::Child { session_id: String, agent: Option<String>, model: String, team: Option<TeamRef> }`
2. **`SessionEvent::Entry(SeqEntry)` (`session/mod.rs:193`)**:
   - Pushed for every durable journal entry: `ChildStart`, `JobStart`, `JobEnd`, `Submission`, `Message`, etc.
3. **Child Session Attach (`session/mod.rs:1805`)**:
   - A host can call `.attach()` on ANY child session handle (`child_handle.session.attach(None)` or `axo.session(id).await?.attach(None)`).
   - Yields the exact same stream of `SessionEvent`s (tokens, tool execution updates, entries) as a root session.

### Polling & Synchronous State
1. **`Session::jobs(&self) -> Vec<JobInfo>` (`session/mod.rs:1663-1665`)**:
   - Synchronous snapshot of all jobs currently in the session registry.
2. **`Session::agent(&self) -> Option<&SpawnLink>` (`session/mod.rs:1580-1582`)**:
   - Returns the child's `SpawnLink` (spec, parent ID, depth, team link).
3. **`Session::agents(&self) -> &[AgentDef]` (`session/mod.rs:1658-1660`)**:
   - Returns the catalog of loaded agents and teams.
4. **`SessionInfo::spawn` in `Axo::list_sessions` (`store/entry.rs:572`)**:
   - Inspectable without opening sessions; allows filtering out child sessions in sidebar listings.

### Control Operations
- `session.spawn(request).await -> Result<ChildHandle, Error>` (`session/spawn.rs:188`).
- `session.spawn_team(request).await -> Result<TeamHandle, Error>` (`session/spawn.rs:189`).
- `child_session.steer(text).await`: Injects steer to child (`session/delivery.rs:322`).
- `child_session.follow_up(text).await`: Injects follow-up to child (`session/delivery.rs:322`).
- `child_session.abort()`: Stops active child run (`session/mod.rs:1725`).
- `proc://agent_N/kill`: Writing to proc scheme aborts child (`schemes/proc.rs:175-180`).

---

## 6. Gaps in axo for Host UI

1. **Root Agent Cannot Be Changed Mid-Session (`session/mod.rs:287, 1269-1288`)**:
   `Inherited.agent` is immutable once a `Session` is started. There is no `Command::Agent` or `session.set_agent(...)` API. To switch persona, the host must close the session and reopen it with a different `SessionOptions.agent`.
2. **Root Agent Persona Is Not Persisted in Header (`store/entry.rs:68-78`)**:
   `Header` stores `spawn: Option<SpawnLink>` for children, but has NO field for root agent name. On reopening a root session with `Axo::open_session`, if the host does not explicitly pass `SessionOptions.agent`, the session reverts to having no root agent.
3. **No Granular Child Progress in Parent Registry (`jobs/mod.rs:241-260`)**:
   `JobInfo` reports only `JobStatus::Running`, `Finished`, or `Failed`. It does not expose child turn count, active tool, or token usage to the parent. The host must attach to the child session directly via `Axo::session(id)` to observe fine-grained activity.
4. **No Pagination on Child Log / Transcript Rendering (`schemes/agent.rs:168-175`)**:
   Reading `agent://agent_N/transcript` re-renders the complete conversation from disk on every read. For long-running subagents, this incurs significant string formatting overhead.

---

## 7. Deep Dive: Implementing `/plan` Mode with an `@plan` Agent

### Switching Root Agent: Per-Conversation, Mid-Session, or Creation Only?
- **At Session Creation**:
  - Fully supported by axo. The host sets `SessionOptions.agent = Some("plan".into())` when calling `Axo::create_session` (`axo.rs:266-294`).
  - axo loads `plan.md` from `agent_roots` or `SessionOptions.agents`, overrides the `SYSTEM` prompt body with the plan agent's body, restricts tools to the plan agent's allowlist (e.g. read-only tools, no edit/bash), and applies context file rules.
- **Mid-Session on an Existing Session**:
  - **Not supported dynamically**: There is NO `set_agent` method on `Session` and NO journal entry type for agent changes. `Inherited` is wrapped in an `Arc<Inherited>` and cannot be mutated (`session/mod.rs:274-297`).
  - **Model vs Agent**: While the **model** can change mid-session via `/model` or `session.set_model` (`session/mod.rs:1696-1721`), the **agent persona** cannot.
  - **Host Workaround for Mid-Session Switch**:
    Because `Header` does not save the root agent, reopening a session via `Axo::open_session` evaluates `SessionOptions.agent` anew. A host can emulate a mid-session switch by:
    1. Unloading the live session from memory.
    2. Calling `Axo::open_session(path, SessionOptions { agent: Some("plan".into()), .. })`.
    3. Re-attaching to the session.
    *Drawback*: Forces a reload of session state and interrupts active streaming.

### Comparison with Axome's Current Architecture
- **Axome Session Creation (`src-crates/axome-core/src/services/axo/mod.rs:150-165, 335-350`)**:
  - `AxoService::options`: Hardcodes `agent: None` via `..Default::default()`.
  - `AxoService::create`: Takes `(folder, model, thinking)`. Does not accept an `agent` parameter.
  - `AxoService::live` (reopen): Calls `open_session(path, options(folder, None, None))`, always passing `agent: None`.
- **Axome Model Picker & Catalog (`src-crates/axome-protocol/src/chat.rs:18-38`, `src/lib/features/chat/models.svelte.ts`)**:
  - `ChatModelCatalog` returns a flat array of concrete `ChatModel`s (`selector: "provider/id"`).
  - No concept of `@role` (e.g. `@plan`, `@fast`, `@code`).
  - No concept of agent definitions or personas.
- **What Axome Needs to Implement `/plan`**:
  1. **Protocol (`chat.rs`)**:
     - Add `agent: Option<String>` to `ChatCreateRequest` (or `chat_create` command arguments).
     - Add `agents: Vec<ChatAgent>` to `ChatModelCatalog` (or a new `ChatAgentCatalog`), exposing definitions from `Session::agents()` or `load_agents`.
     - Expose `modelRoles` in `ChatModelCatalog`.
  2. **Persistence**:
     - Store the chosen `agent` persona in Axome's database or session metadata so that when `AxoService::live` reopens an existing session, it supplies the original `agent` in `SessionOptions.agent`.
  3. **Frontend Workbench**:
     - Add an Agent / Mode picker alongside the Model / Thinking selectors in `ChatSurface.svelte`.
     - Allow selecting `@plan` mode at conversation creation.

---

## 8. Verified Code vs Docs Mismatches

During code verification, the following mismatches were identified between `crates/axo/docs/` and the actual implementation at HEAD:

1. **Child Session Lazy Creation (`crates/axo/docs/sessions-and-compaction.md:10` vs `crates/axo/src/store/jsonl.rs:136`)**:
   - *Doc claims*: "Child and fork files are created immediately because their headers link to a parent."
   - *Code reality*: In commit `6cdaeb4`, child headers stopped setting `parent_session` (`Header::new(..., None, None)` in `spawn.rs:379`). In `jsonl.rs:136`, `JsonlStore::create` only eagerly creates files if `header.parent_session.is_some()`. Consequence: Child session files are created **lazily** on first submission, exactly like root sessions.
2. **Child Session Header `parentSession` Field (`crates/axo/docs/sessions-and-compaction.md:28` vs `crates/axo/src/session/spawn.rs:379`)**:
   - *Doc claims*: "The child's `parentSession` names the parent file and `forkedFrom` is absent."
   - *Code reality*: Child headers leave `parent_session` as `None`. Parent session identity lives exclusively in `Header.spawn.parent_session_id`.
3. **`Agent::take_steering` & `Agent::take_follow_ups` (`crates/axo/docs/` references vs `crates/axo-agent/src/agent.rs:260`)**:
   - *Doc/plan references*: Earlier documentation described separate drain calls for steering and follow-ups.
   - *Code reality*: Commit `6cdaeb4` removed `take_steering` and `take_follow_ups` as a breaking change, consolidating them into `pub fn take_next_input(&self) -> Vec<QueuedMessage>`.
