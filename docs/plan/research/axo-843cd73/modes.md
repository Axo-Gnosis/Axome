# axo 843cd73: session modes

Raw survey report, kept verbatim as evidence. Produced by a read-only subagent on 2026-11-10 against axo `843cd73`, verified against source code (not docs). Covers mode definitions, entry and exit, tool limits, model rule, result review, children, and Axome's command handling.

Citation prefixes are defined inside the report. Claims marked [INFERENCE] were not observed directly.

---

## Summary

Modes are **root-session, branch-derived instruction overlays**, independent of personas. They add request-boundary instructions, call-time tool restrictions, optional entry-time model/thinking selection, and optional schema-validated results with event-driven host review. They do **not** replace the persona or inherently rebuild its system prompt/tool shape. Children inherit a **persisted tool ceiling**, not the mode’s instructions or selected-mode identity. Axome currently has neither mode state/review wiring nor mode-command routing.

**Citation roots:** `axo/` = `/repo/rust/axo`; `Axome/` = `/repo/rust/Axome`. All implementation citations below refer to source at inspected HEAD. Read-only investigation; no builds/tests executed.

## Files / architecture

- `axo/crates/axo/src/context/modes.rs:13-224` — public definitions, frontmatter, discovery, validation.
- `axo/crates/axo/src/config.rs:398-450,483-529` — host-supplied roots and trust filtering.
- `axo/crates/axo/src/axo.rs:294-316,329-402` — catalog and session configuration/model resolution.
- `axo/crates/axo/src/session/modes.rs:17-667` — selection/restoration, notices, host review, deferred `yield` and `enter_mode`.
- `axo/crates/axo/src/session/policy.rs:15-127` — effective scope, host policy adapter, call-time mode gate.
- `axo/crates/axo/src/session/surface.rs:20-189` — deferred tool catalog, request-boundary activation, promotion, announcement.
- `axo/crates/axo/src/session/spawn.rs:159-191,314-438` — child inheritance and persisted ceiling.
- `axo/crates/axo/src/store/entry.rs:89-111,218-278`; `store/project.rs:30-44,101-120,171-175` — durable mode state and branch projection.
- `axo/crates/axo/src/commands.rs:19-80,201-241,309-398`; `session/mod.rs:1990-2028` — static built-ins versus session-aware mode commands.
- `axo/crates/axo/src/compaction/mod.rs:82-93,260-419`; `compaction/elide.rs:112-124` — notice cut points, compaction, shake protection.
- Axome bridge: `src-crates/axome-core/src/services/axo/{mod.rs,live.rs,convert.rs}`; wire contract: `src-crates/axome-protocol/src/chat.rs`; composer: `src/lib/features/chat/elements/prompt-input/{document.ts,PromptInputRoot.svelte}`.

**Connection:** host roots/specs → `Axo::catalog` → root session’s mode catalog → selected mode journal entry → effective mode at next request boundary → model-facing custom notice + execution gate + optional deferred result tool. Spawn snapshots the applicable restrictions into the child header. Host review is a pending registry plus session events/methods, **not a review trait**.

# 1. What axo now provides

### 1.1 Definition format: every field

Files are recursively discovered `*.md` with leading YAML frontmatter. Files lacking a recognized leading frontmatter block are silently skipped; an empty block is valid. BOM/newline normalization and trimmed bodies come from the shared markdown parser. (`axo/crates/axo/src/context/modes.rs:95-197`; `context/markdown.rs:11-23`.)

| YAML field | Exact semantics/default |
|---|---|
| `name` | Optional string; omitted/empty uses file stem. Validated lowercase ASCII letters/digits/hyphens, maximum 64 characters, no leading/trailing or consecutive hyphens. Built-in command names are rejected. |
| `description` | String, default empty; command and model-entry listing text. |
| `argument-hint` | Optional string; host placeholder after `/name`. |
| `disable-model-invocation` | Boolean, default `false`; excludes mode from built-in `enter_mode`, **not** host selection or command catalog. |
| `model` | Optional selector applied on entry; can choose model, thinking, or both. |
| `tools.allow` | Optional `Vec<String>`; absent imposes no positive allowlist, empty list permits no ordinary tool. Exact-name matching. |
| `tools.deny` | `Vec<String>`, default empty; deny wins over allow. |
| `result` | Optional `ModeResult`; absent means only the host ends the mode. |
| `result.schema` | JSON Schema; defaults to `{"type":"string"}`. Invalid schemas reject the definition with a diagnostic. |
| `result.review` | Boolean, default `false`; when true, result execution waits for host review. |
| `result.then` | Optional continuation string appended to successful `yield` output; keeps the run continuing rather than terminating through that tool. |
| Any other top-level key | Captured in `ModeSpec::extra: serde_json::Map<String, Value>` for host policy; no built-in semantics or unknown-key warning. |

Sources: `axo/crates/axo/src/context/modes.rs:17-92,174-224`; `context/markdown.rs:51-78`.

The trimmed markdown **body is mode instructions**, inserted into a conversational `<mode name="…">…</mode>` notice, not a system-prompt replacement. `ModeSpec` additionally exposes `body: String` and `path: Option<PathBuf>`; file-loaded definitions keep their path. Its public fields serialize camelCase, whereas YAML explicitly uses the two hyphenated keys above. (`axo/crates/axo/src/context/modes.rs:15-33,174-193`; `session/modes.rs:276-305`.)

There is **no recognized `agent`, `summary`, `thinking`, or bash-sandbox field** in a mode. Such top-level fields merely become `extra`; thinking is selected through `model` syntax. (`axo/crates/axo/src/context/modes.rs:78-92`; `session/modes.rs:199-233`.)

### 1.2 Definition acquisition, trust, and “root-only”

- `ConfigLayer::mode_roots: Vec<PathBuf>` is a **Rust host input**, not a new settings JSON key. `EffectiveConfig::mode_roots` contains trusted project roots followed by user roots. Project roots require `ProjectAccess::Trust`; at `Read`, supplied roots are ignored with a tracing warning. At `Ignore`, or when the user disables `project.files.config`, no project layer is acquired. (`axo/crates/axo/src/config.rs:398-450,483-509`.)
- `SessionOptions::modes: Vec<ModeSpec>` adds programmatic definitions **after all file definitions**, first name wins. (`axo/crates/axo/src/session/mod.rs:97-98`; `context/modes.rs:103-149`.)
- Roots are recursively walked in sorted file-name order, with shared ignore rules, hidden-file skipping, `node_modules` exclusion, and symlink following. Canonical paths are deduplicated; later duplicate names produce diagnostics and do not replace earlier definitions. (`axo/crates/axo/src/context/markdown.rs:26-48`; `context/modes.rs:104-149`.)
- No implicit mode location is acquired by these paths. The host supplies roots/specs. (`axo/crates/axo/src/axo.rs:294-307`; `config.rs:398-450`.)
- “Root-only” means **root sessions**, not “only markdown files immediately under a filesystem root.” Discovery is recursive. Children receive `modes: Vec::new()`, and their `set_mode` calls fail `"Modes belong to root sessions"`. (`axo/crates/axo/src/session/spawn.rs:171-178`; `session/modes.rs:318-323`.)

### 1.3 Catalog

Exact API:

```rust
pub fn catalog(
    &self,
    cwd: &Path,
    project_access: Option<ProjectAccess>,
    agents: &[AgentSpec],
    modes: &[ModeSpec],
) -> Result<Catalog, Error>
```

`Catalog` contains `config: Arc<EffectiveConfig>`, `agents: Vec<AgentDef>`, `modes: Vec<Arc<ModeSpec>>`, `diagnostics: Vec<Diagnostic>`. It resolves config and loads definitions synchronously without starting a session. Definitions’ model selectors are not resolved here; that happens at selection. (`axo/crates/axo/src/axo.rs:48-54,292-316`; `session/modes.rs:199-215`.)

### 1.4 Entering, switching, and exiting

```rust
pub async fn set_mode(&self, name: Option<&str>) -> Result<(), Error>
pub fn mode(&self) -> Option<Arc<ModeSpec>>
pub fn modes(&self) -> &[Arc<ModeSpec>]
```

`Some(name)` selects; `None` ends. Unknown names return `Error::ModeNotFound`; selecting the already-recorded name is a no-op. Selection alone does not submit input/start a run. Instructions and limits become effective at the next request boundary; the getter reports the **selected**, possibly not-yet-effective mode. (`axo/crates/axo/src/session/modes.rs:140-175,318-339`; `session/mod.rs:1995-1998`; `session/surface.rs:89-105`.)

**Slash command:** `Session::parse_command(&self, text: &str) -> Option<Result<Command, Error>>` parses built-ins first, then `/mode-name [prompt…]` into `Command::Mode { name: String, prompt: Option<String> }`. `run_command` selects first, then `steer`s optional prompt text. `Command::parse` and `Command::decode` remain built-in-only. There is no new generic `/mode`, `/exit-mode`, `/plan`, or `/agent` built-in. (`axo/crates/axo/src/commands.rs:19-41,201-241,309-330`; `session/mod.rs:2000-2027`.)

**Model tool:** root-only built-in **deferred** `enter_mode`, subject to persona allowlists, collisions with direct/deferred host tools, active-mode gate, and host tool policies. It lists only modes without `disable-model-invocation`. Its exact schema is:

```json
{
  "type": "object",
  "properties": {
    "mode": { "type": "string", "enum": ["<invocable mode names>"] }
  },
  "required": ["mode"],
  "additionalProperties": false
}
```

There is **no `summary` argument** and no prompt argument. Its description begins:

> Switch this session into a mode: instructions, and sometimes tool limits, for one kind of work.

then `Modes:` and `- name: description` lines. Deferred catalog “summary” is derived from the first nonblank description line, sanitized/truncated to 200 characters; it is not a separate mode field. Successful output is:

> Mode "NAME" starts with your next step; its instructions follow.

It is `Replay::Safe`. (`axo/crates/axo/src/session/modes.rs:581-667`; `session/mod.rs:1538-1542`; `session/prompt.rs:144-182`; `tools/deferred.rs:53-66`.)

### 1.5 Model/thinking rule

A mode **switches on entry; it does not pin/enforce the selection throughout the mode**. The selector accepts `provider/id`, `provider/id:thinking`, `@role`, `@role:thinking`, or `:thinking`. Role suffix overrides the role’s suffix; roles cannot recursively name other roles. Exact model lookup precedes suffix interpretation, preserving model IDs containing colons. Thinking is clamped to model support. (`axo/crates/axo/src/config.rs:135-204`; `session/modes.rs:199-235`.)

If entry changes model or thinking, the base is saved as:

```rust
ModeRestore {
    provider: String,
    model_id: String,
    thinking_level: ThinkingLevel,
}
```

Switching mode A → B uses A’s saved base rather than stacking overrides. A new mode with no selector returns to that base. Exiting restores it if that model still exists; otherwise it keeps the current selection. Explicit model/thinking journal entries clear the restore record **without clearing the mode**, so exit does not undo a later user selection. (`axo/crates/axo/src/store/entry.rs:218-225`; `store/project.rs:104-120`; `session/modes.rs:140-235`.)

Reopen uses recorded model/thinking and freshly resolved definitions; it does **not** reapply a changed mode selector merely because the mode name is restored. Missing recorded modes produce a startup diagnostic and run without an effective mode. (`axo/crates/axo/src/axo.rs:379-402`; `session/mod.rs:1570-1589`.)

### 1.6 Finishing through `yield` and host review

An effective mode with `result` dynamically adds deferred `yield`, independent of persona allowlist. Nonobject schemas are wrapped as `{"result": …}` with required `result` and no additional properties; explicit `"type":"object"` schemas are the tool arguments directly. Validation happens before execution/hooks. (`axo/crates/axo/src/session/modes.rs:447-481`; `tools/output.rs:18-40`; `axo-agent/src/run.rs:632-647`.)

There is **no review hook/trait**. The surface is:

```rust
pub fn accept_result(
    &self, call_id: &str, message: Option<String>
) -> Result<(), Error>

pub fn revise_result(
    &self, call_id: &str, feedback: String
) -> Result<(), Error>

pub fn dismiss(&self, call_id: &str) -> Result<(), Error>
```

- `review: true`: registers `PendingResult { call_id, mode, result: Value }`; emits `ModeResult { awaiting_review: true }`; tool waits.
- **Approve:** `accept_result` ends mode. Optional `message` replaces—not appends to—the configured `result.then`.
- **Reject/request revision:** `revise_result` requires nonblank feedback, keeps mode active, and returns successful tool text:  
  `The result was not accepted. Revise it and call yield again.\n\nFEEDBACK`
- **Edit:** no API replaces the typed result value. Host can edit continuation text via `accept_result(..., Some(message))` or ask the model to revise; those are different from accepting an edited result.
- **Dismiss:** resolves with `"The result was dismissed by the user"` and aborts the run.
- Replacing/ending a selected mode supersedes outstanding reviews; stale yields fail if their selected/effective mode identity is no longer active.
- `review: false`: emits `ModeResult { awaiting_review: false }` and accepts immediately.

Sources: `axo/crates/axo/src/session/modes.rs:341-434,490-563`; `session/mod.rs:2305-2314`.

Successful result output:

```text
Result accepted. Mode "NAME" has ended.

<host message, otherwise configured then, if present>
```

The tool sets `terminate: next.is_none()`. Note that the agent batch terminates only if **every** produced tool result requests termination, not simply because one mode `yield` does. (`axo/crates/axo/src/session/modes.rs:565-577`; `axo-agent/src/run.rs:609-625`.)

**Where the result lands:** typed value goes to host events/pending snapshots; original result arguments remain in the assistant’s tool call. Successful tool output contains acceptance/continuation, **not a typed result in `details` or a new dedicated result journal entry**. This is not child `JobEnd.value` delivery. (`axo/crates/axo/src/session/modes.rs:516-577`; `axo-agent/src/run.rs:952-968`.)

# 2. Breaking changes versus 6cdaeb4 and Axome

The inspected `6cdaeb4..HEAD` diffs add modes, mode roots, `Command::Mode`, `CommandKind::Mode`, session-aware parsing, and a second `commands::catalog` argument. **No additional static built-in commands and no Settings JSON keys were added in this interval.** HEAD still has five built-ins: `compact`, `shake`, `model`, `thinking`, `navigate`; the config delta is roots/host API, not new settings. (`axo/crates/axo/src/commands.rs:201-241,332-365`; `config.rs:24-42,398-450`.)

| Axome today | Break / semantic miss | HEAD counterpart |
|---|---|---|
| `Axome/src-crates/axome-core/src/services/axo/convert.rs:82-94` exhaustively handles `Builtin`/`Skill` | New `CommandKind::Mode` requires handling. | `CommandKind::Mode { argument_hint }`, Alongside policy; `axo/.../commands.rs:60-71,355-364`. |
| `Axome/src-crates/axome-protocol/src/chat.rs:335-350` defines only those two command kinds | No wire representation for mode commands. | Mode metadata is a distinct kind, not a skill. |
| `Axome/src-crates/axome-core/src/services/axo/mod.rs:451-459` uses `Command::decode(name,args)` | Mode names still decode as `"unknown command"`; this API is not extended to dynamic mode JSON decoding. | `Session::parse_command`, explicit `Command::Mode`, or `Session::set_mode`; `axo/.../session/mod.rs:2000-2027`. |
| `Axome/src/lib/features/chat/elements/prompt-input/document.ts:105-108`; `PromptInputRoot.svelte:246-281` | Only built-ins enter command submission; a newly surfaced mode otherwise follows ordinary prompt submission. axo does not automatically execute slash modes inside prompt text. | Host must dispatch session commands; `axo/.../session/mod.rs:2000-2027`; `session/delivery.rs:209-211`. |
| `Axome/.../PromptInputRoot.svelte:164-177` | Any nonbuiltin is labeled/iconed as “Skills”; mode would be misclassified. | Distinct catalog mode kind. |
| `Axome/src-crates/axome-core/src/services/axo/convert.rs:195-226,530-555`; `live.rs:282-375` | Exhaustive `Entry` matches lack `ModeChange`—new entry variant is a compile-time integration change, not merely an unknown line. | `Entry::ModeChange`; `axo/.../store/entry.rs:270-278`. |
| `Axome/src-crates/axome-core/src/services/axo/live.rs:261-466` | No `Mode`, `ModeResult`, `ModeResultResolved` relay handling. | New events, `axo/.../session/mod.rs:237-250`. |
| `Axome/src-crates/axome-protocol/src/chat.rs:303-315`; bridge `live.rs:56-93` | No selected mode or pending result reviews in wire live state. | `LiveSnapshot.mode`, `.pending_results`; `axo/.../session/mod.rs:325-336`. |
| `Axome/src-crates/axome-core/src/services/axo/mod.rs:430-449` | Host exposes ask answer/dismiss, but no mode acceptance/revision methods. Dismiss can now address either kind. | APIs in `axo/.../session/modes.rs:341-355`, `session/mod.rs:2305-2314`. |

Axome session options currently supply no modes (`..Default::default()`); no existing chat plan/mode state appears in the inspected integration or protocol. Broad searches for planning-mode identifiers, `/plan`, `@plan`, and entry/API names found no chat implementation; generic “mode” hits were UI/file permissions and `/shake` tests. Thus **“Axome has no implemented planning-mode integration” is [INFERENCE] from searches plus its explicit current wire/options surfaces**, not a claim that no unrelated application feature uses the word plan. (`Axome/src-crates/axome-core/src/services/axo/mod.rs:145-166`; `axome-protocol/src/chat.rs:303-350`; `src/lib/features/chat/turns.ts:23-28,94-97`.)

# 3. Silent behavior, journal, prompt, and result changes

### Durable branch state and navigation/fork

New journal line:

```text
type: "mode_change"
base: id, parentId, timestamp
mode?: string
restore?: { provider, modelId, thinkingLevel }
```

Absent `mode` means no mode. `ModeSpec` itself is not frozen in the journal. Writers are normal mode selection and navigation’s restore correction. Mode state is tracked across the **whole ancestor chain before compaction removes messages**. (`axo/crates/axo/src/store/entry.rs:218-237,270-278`; `store/project.rs:101-120,171-175`; `session/modes.rs:95-101,165-169`.)

`navigate_inner` captures origin mode/restore before moving the branch, then calls `rebase_mode(origin)`. Crossing mode names restores the departed mode’s base, applies target mode’s selector, records current live selection, and may append a fresh `ModeChange` to correct target restore metadata. Same-mode navigation keeps live selection. A target selector failure after the leaf is written logs a warning and uses the base, rather than failing half-moved. (`axo/crates/axo/src/session/maintenance.rs:526-545`; `session/modes.rs:64-112`.)

**Fork differs from navigate:** fork copies the target ancestor path, so selected mode comes from that copied path, but passes the **parent’s current live model/thinking** to `Session::start`; it does not call `rebase_mode`. Startup then records that live selection over the copied branch. Therefore do not assume fork crossing into a mode reapplies that mode’s selector like navigation does. Subsequent startup model/thinking entries can clear the copied restore record. (`axo/crates/axo/src/session/mod.rs:2382-2430,2451-2511,1745-1753`; `store/project.rs:108-120`.)

### `mode_notice`

Exact entry: `CustomMessage`, custom type `"mode_notice"`, `display: false`, `details: {"mode": <name or null>}`, text content. Host `Session::post` may not impersonate this reserved custom type. (`axo/crates/axo/src/session/modes.rs:17,247-313`; `session/delivery.rs:25-35`.)

Model receives its content as a **user message**, including:

```text
[Mode "previous" has ended.\n]
<mode name="NAME">
BODY
</mode>
[Only these tools run while this mode is active: A, B.]
[These tools do not run while this mode is active: X, Y.]
```

Then either:

```text
Finish this mode by calling yield with its result: call {"tool": "yield", "args": …}; read tool://yield for the arguments.
```

or:

```text
This mode stays active until the user ends it.
```

Exit-only text:

```text
Mode "NAME" has ended; its instructions and tool limits no longer apply.
```

Sources: `axo/crates/axo/src/session/modes.rs:276-313`; `session/mod.rs:956-960`.

Reconciliation compares current effective mode **by name** with the latest surviving projected notice. A definition/body change with the same name does not by itself force a replacement notice. Compaction may remove the notice while journal state survives; the next request then reannounces the active mode. `/shake` and automatic elision explicitly leave `mode_notice` and `tool_catalog` intact. `df777d0` makes these notices **compaction cut points but not turn starts**, enabling split-turn treatment when a notice follows a user prompt. (`axo/crates/axo/src/session/modes.rs:247-275`; `session/mod.rs:976-987`; `compaction/mod.rs:82-93,283-288,342-358`; `compaction/elide.rs:112-124`.)

Axome already hides `display:false` custom messages instead of drawing raw engine text. That remains appropriate for transcript text, but does not itself expose mode status. (`Axome/src-crates/axome-core/src/services/axo/convert.rs:413-426`.)

### Tool limits and policy composition

- Root restrictions are **call-time checks**, not removal from the persona’s declared tool shape.
- Mode gate examines the **routed inner name**, so `call {"tool":"write",…}` cannot bypass it.
- Arguments validate first. Child budget hook precedes mode gate; mode gate precedes ordered host `ToolPolicy`s. A blocking gate prevents later policies and actual execution.
- Block result: one text block, `is_error:true`, `details:None`, `terminate:false`, text  
  `NAME is unavailable in mode "MODE".`
- `yield` bypasses mode allow/deny but not host policy. `call` itself is routing, not an allowlist bypass.
- Persona allowlists determine available direct/deferred tools first; modes do not grant tools absent from that shape. Root `yield` is dynamically added after persona filtering, and roots with modes retain `call`.
- Persona allowlists retain lifecycle `kill`/`send` subject to presence rules; mode restrictions can block them.

Sources: `axo/crates/axo/src/session/policy.rs:90-110`; `session/mod.rs:1624-1635`; `session/prompt.rs:163-205`; `session/surface.rs:20-38`; `axo-agent/src/run.rs:632-670,1050-1058`; `tools/call.rs:52-55`.

**Not a bash sandbox:** limits constrain tool names. Allowing `bash` while denying `write` does not inspect shell commands; host policies receive `ToolScope.mode`, including `extra`, for any further policy. (`axo/crates/axo/src/context/modes.rs:35-49`; `session/policy.rs:15-40,101-109`.)

### Children and inherited ceiling

Host spawns use **selected mode immediately**; model `spawn` uses **effective mode** from the request boundary. At a root, its tool limits are copied into `SpawnLink::limits`; descendants copy that link’s limits, never derive a broader replacement from current mode. Spawn hooks receive both `mode` and `limits`, but `SpawnDecision::Replace` replaces the agent spec, not the ceiling. (`axo/crates/axo/src/session/spawn.rs:124-128,324-335,361-391,422-431`; `store/entry.rs:105-108`.)

Children have no mode catalog, selection, inherited mode body, or built-in `enter_mode`. Their tool shape is filtered by the persisted ceiling **after persona filtering**, including `kill` and `send`; required structured-output child `yield` is added afterward. Parent mode exit/change does not update existing child ceilings. Revival reuses the saved link/spec. (`axo/crates/axo/src/session/spawn.rs:171-191`; `session/prompt.rs:184-205`; `session/mod.rs:1558-1580`; `session/children.rs:286-303`.)

### Result durability/recovery

Pending reviews and their typed values are in-memory state, not separate durable review records. Ordinary assistant tool calls preserve proposals; settled tool results preserve acceptance/revision text; ending mode writes `ModeChange`. There is no separately journaled approve/reject/edit decision object. (`axo/crates/axo/src/session/modes.rs:359-434,516-577`; `axo-agent/src/run.rs:952-968`.)

Reviewing mode yield is `Replay::Safe`; unreviewed yield is `Replay::Never`. Recovery resolves routed tools and uses that replay policy: unanswered safe calls rerun/register review; unsafe calls are interrupted instead. A crash after mode exit but before its tool result means the old mode’s yield is no longer in the current catalog; do not interpret review replay as a transactional durable approval protocol. (`axo/crates/axo/src/session/modes.rs:483-488,498-513`; `session/resume.rs:192-200`; `recovery.rs:445-470`; `session/surface.rs:20-38`.)

### Prompt caching: precise qualification

Mode instructions/limits do not rebuild `PromptShape` or replace its system sections: selection updates state, request-boundary activation updates scope/yield, and notices append conversational context. This preserves the ordinary system/tool prefix instead of using `set_agent`. (`axo/crates/axo/src/session/modes.rs:140-175`; `session/surface.rs:89-125`; `session/prompt.rs:103-113,163-205`.)

However, **“the declared tool list stays unchanged across all mode changes” is too absolute**: deferred `yield` appears/disappears or changes schema with modes; on models supporting native tool changes, reading tool docs can promote it into declarations, and ending/replacing it prunes promotions. Add-only models specifically do not promote `yield`. A mode model switch can also change provider/model cache reuse. (`axo/crates/axo/src/session/surface.rs:15-17,29-38,59-68,107-124`; `session/modes.rs:447-458,172`.)

# 4. Responsibility split and personas

| axo owns | Host must supply, decide, or render |
|---|---|
| Definition parsing, ordering, diagnostics, trust filtering | Actual mode locations, programmatic definitions, project trust acquisition |
| Selected/effective boundary mechanics and mode journal entries | Mode picker/status, optional slash routing, explicit exit control |
| Call-time gate and inherited child ceiling | Policies beyond tool-name restriction, including interpretation of `extra` |
| Entry model/thinking switch and conditional restore | Selector/role configuration and presentation of mode-driven model changes |
| Typed result validation, pending wait, review events/methods | Actual review UI, approve/revise/dismiss/continuation choices |
| Reconciliation of non-display model notices | Whether/how to show human-readable mode transitions and history |

Sources: `axo/crates/axo/src/context/modes.rs:17-92`; `config.rs:398-450`; `session/modes.rs:140-175,247-355,516-577`; `session/policy.rs:15-40`; `session/spawn.rs:324-391`.

**Mode vs persona:** a mode has no built-in agent selector. `set_agent` journals `AgentChange`, may apply its model selector, reshapes tools/context/system prompt, rebuilds prompt, and rescopes; `set_mode` independently overlays instructions/limits. Mode and agent can share a name because their catalogs are separate; no automatic relationship exists. (`axo/crates/axo/src/context/modes.rs:17-33,78-92`; `session/mod.rs:2117-2195`; `session/modes.rs:140-175`.)

Thus `/plan` is a mode command **if a mode named `plan` exists**. `@plan` in a model selector is a **model role**, not an agent reference. A persona named `plan` is selected through the agent API; selecting `/plan` does not select it. Having both definitions is possible but not automatically coupled. (`axo/crates/axo/src/session/mod.rs:2000-2027,2120-2141`; `config.rs:135-166`.)

# 5. Host observability and control

| Surface | Contents / push versus query / durability |
|---|---|
| `Session::modes()` | Loaded root definitions; query, empty for children. |
| `Session::mode()` | Selected `Arc<ModeSpec>`, including pending boundary change; query. |
| `SessionEvent::Mode { mode: Option<String> }` | Push selected-name changes, including navigation rebase. **Not** an effective-boundary event. |
| `LiveSnapshot::mode` | Selected name at attach. |
| `SessionEvent::ModeResult { call_id, mode, result: Value, awaiting_review }` | Push proposal/result, including unreviewed acceptance. |
| `LiveSnapshot::pending_results: Vec<PendingResult>` | Attach snapshot of outstanding reviews. |
| `SessionEvent::ModeResultResolved { call_id }` | Push removal; does not encode accepted/revised/dismissed outcome. |
| `ToolScope { session_id, depth, agent, mode }` | Effective mode/spec to ordered host tool policy callbacks. |
| `SpawnContext::mode`, `limits` | Selected parent mode for host spawn; effective for tool spawn, plus ceiling. |
| `Projection::{mode, mode_restore}` | Reconstructable branch state from journal. |
| `SpawnLink::limits` | Reconstructable descendant ceiling from child header. |
| `set_mode`, `accept_result`, `revise_result`, `dismiss` | Host control methods; no separate effective-mode getter/review trait. |

Sources: `axo/crates/axo/src/session/mod.rs:237-250,325-336,1995-1998`; `session/modes.rs:335-355,359-417`; `session/policy.rs:15-24`; `session/spawn.rs:124-128,324-335`; `store/project.rs:40-44`; `store/entry.rs:105-108`.

Attach supplies missed durable entries, snapshot, and live broadcast receiver; it can recover pending review display after receiver lag without polling. Effective mode remains separately internal. (`axo/crates/axo/src/session/mod.rs:303-337,2347-2360`; `session/modes.rs:31-45`.)

# 6. Concrete discussion questions for Axome

1. **Which named modes, if any, should Axome expose, and who supplies their roots/programmatic definitions?** No built-in planning mode exists merely from upgrading. (`axo/crates/axo/src/context/modes.rs:95-149`; Axome options `.../services/axo/mod.rs:145-166`.)
2. **Should a desktop “Plan” action select a mode, a persona, or explicitly both—and should `@plan` instead mean a configured model role?** Those are independent mechanisms. (`axo/.../session/mod.rs:2000-2027,2120-2195`; `config.rs:135-166`.)
3. **How should the UI distinguish selected mode from the restrictions governing an already-in-flight tool batch?** Current getter/event/snapshot describe selected state only. (`axo/.../session/modes.rs:335-339`; `session/surface.rs:89-105`.)
4. **How should dynamic mode commands be dispatched and classified in the composer, rather than sent as skill/plain prompt text?** (`Axome/.../document.ts:105-108`; `PromptInputRoot.svelte:164-177,246-281`; backend `mod.rs:451-459`.)
5. **What constitutes the review UI: approve, request revision, dismiss, and/or editing continuation text? Is editing the actual result required?** Current API cannot accept a replacement typed value. (`axo/.../session/modes.rs:341-355,516-577`.)
6. **Should acceptance stop, continue with `result.then`, or use host-supplied continuation text, and how is an explicitly empty continuation treated?** Implementation uses `Option` presence, not nonblank content, to control termination. (`axo/.../session/modes.rs:565-577`.)
7. **Which restrictions are intended tool restrictions versus semantic constraints such as “read-only shell,” and what host policy should interpret mode `extra`?** (`axo/.../context/modes.rs:29-30,35-49`; `session/policy.rs:15-40`.)
8. **How should existing children with older persisted ceilings be represented after the parent leaves or changes mode?** Their limits do not follow later parent selection. (`axo/.../session/spawn.rs:329-335`; `store/entry.rs:105-108`.)
9. **Should fork retain the parent’s current model/thinking even when its copied mode belongs to another point, as source currently does, or is navigation-style mode selection expected?** (`axo/.../session/mod.rs:2382-2383,2499-2511`; `session/modes.rs:64-101`.)
10. **How should missing or edited mode definitions on reopen be presented, given names—not frozen definitions—are recorded, and same-name notices are reconciled by name only?** (`axo/.../session/mod.rs:1570-1589`; `session/modes.rs:247-275`.)
11. **Does Axome need a durable approval/audit object beyond reconstructed proposals and text tool results?** Current pending reviews are memory state and resolved events contain only a call ID. (`axo/.../session/modes.rs:359-417,516-577`.)
12. **Should same-name mode/skill collisions hide the skill only in the picker, or should the host make their differing prompt semantics visible?** Catalog hides skills, but ordinary prompt skill expansion is separate. (`axo/.../commands.rs:344-364,381-398`; `input.rs:282-285`.)

# 7. Remaining gaps / mismatches

- **No public effective-mode snapshot/getter or “now effective” event.** A precise in-flight-versus-next-step indicator cannot come directly from the selected mode surfaces. Effective mode is internal; tool policies can observe it only during calls. (`axo/crates/axo/src/session/modes.rs:31-45,335-339`; `session/policy.rs:23-24`; `session/mod.rs:237-250,325-336`.)
- **No typed-result edit/replace operation, structured review outcome event, reviewer identity, or dedicated durable review-decision record.** (`axo/crates/axo/src/session/modes.rs:341-434,516-577`.)
- **No current root-mode result accessor/history index analogous to a child’s typed final result.** Root mode values are emitted/pending and remain in proposal arguments; successful result details do not carry them. [INFERENCE] A host wanting a persistent “approved plan” panel must distinguish proposal arguments from acceptance text itself or own extra metadata. (`axo/crates/axo/src/session/modes.rs:516-577`; `axo-agent/src/run.rs:952-968`.)
- **No live mode-definition reload API in these surfaces.** Definitions are loaded at catalog/session resolution; selection resolves only within that inherited catalog. (`axo/crates/axo/src/axo.rs:294-307,349-358`; `session/modes.rs:318-331`.)
- **No mode-level shell sandbox or automatic persona coupling.** Tool-name limits and `extra` are the provided primitives. (`axo/crates/axo/src/context/modes.rs:17-49`; `session/policy.rs:101-109`.)
- **No built-in dynamic JSON command decoder.** `Command::decode` remains static; desktop structured commands must account for modes separately. (`axo/crates/axo/src/commands.rs:309-318`; `session/mod.rs:2000-2027`.)
- **Source comment mismatch:** `session/delivery.rs:209-210` still tells typed-command hosts to use `Command::parse`; that excludes modes, whereas `Session::parse_command` is the mode-aware parser. This is an inline-source documentation mismatch, not a behavior uncertainty. (`axo/crates/axo/src/session/mod.rs:2000-2016`.)
- **Caching statement needs qualification, not blanket adoption:** ordinary mode overlays preserve the prefix, but native deferred-tool promotion/schema removal and explicit mode model selection can change declarations/cache behavior. The rewritten guide itself describes promotion capability rules; source confirms these exceptions. (`axo/crates/axo/docs/prompts-and-context.md:60`; implementation `session/surface.rs:15-17,59-68,107-124`.)
- **No additional confirmed rewritten-guide/source discrepancy found in the examined mode sections.** Their claims about name-only notice reconciliation, review memory state, inherited ceilings, and navigation restoration match the cited implementations; fork’s distinct model behavior remains an important implementation detail rather than assuming navigation semantics.
