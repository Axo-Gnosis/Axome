# axo `b961066`: Phase 3 batch (K, W)

Survey of axo commit `b9610666ea6f04a55be3a44b1d6d0f7f15531c06` ("feat: add write scopes and record submission keywords", 2026-11-15) against its parent `cd121629`, read from source. Paths are under `/repo/rust/axo/crates/axo/src/` unless they start with `tests/` or `docs/` (both under `crates/axo/`). It answers: did the Phase 3 batch requested in [`../../handoff/axo-phase-3-batch.md`](../../handoff/axo-phase-3-batch.md) land as asked, under which names, and with which semantics Stage B must use.

Gate observation in `/repo/rust/axo` after `git fetch origin`: `HEAD` == `origin/main` == `b9610666ea6f04a55be3a44b1d6d0f7f15531c06`; its parent is `cd121629365ff5f8fba5003146155b50168cb923`, the Stage A pin. Both items are present under the requested names and types. Upstream tests cited below are evidence of intended behaviour; none were run for this survey.

## 1. K. Fired keywords

- `Entry::Submission` gains `keywords: Vec<String>` with `#[serde(default, skip_serializing_if = "Vec::is_empty")]` (`store/entry.rs:425-427`). Journals written before `b961066` read as an empty list; an empty list is not written.
- The recorded names are the matched keyword specs, so the journal and the injected text come from one match (`session/delivery.rs:750-772`).
- Matching (`session/delivery.rs:254-301`): only `Text` parts count, in order, never a skill's expansion; the first part's leading `/command` token is skipped; names are deduplicated in text order. Children, keywords switched off, disabled names and keywords whose `requiresTools` are not all allowed in the current mode never fire.
- Model-only keywords (a `model` and no body) are recorded although they inject no text (upstream test `tests/session.rs:1213` `model_only_keywords_are_recorded_without_a_block`).
- Recovery re-renders requeued messages from the **recorded** names, not a re-match: a name whose definition is gone is skipped, and a keyword's model is not applied again (`session/resume.rs:56-108`; upstream test `tests/session.rs:1243`).

**Axome:** Stage B1 copies `Entry::Submission.keywords` into the sent-message chip. Rust code that builds `Entry::Submission` without `..` must add the field.

## 2. W. Write scope

| API | Type | Serialization / notes |
|---|---|---|
| `ModeSpec.writes` | `Option<Vec<String>>` | `ModeSpec` is `Serialize` only; `None` serializes as `null` (`context/modes.rs:15-31`). Frontmatter key `writes` (`context/modes.rs:112-113`) |
| `AgentSpec.writes` | `Option<Vec<String>>` | `None` omitted (`context/agents.rs:64-68`); `writes` is a known agent key, so it raises no unknown-key diagnostic (`context/agents.rs:381-382`) |
| `ToolScope.writes` | `Option<Vec<String>>` | The normalized effective scope; `ToolScope` is `Debug, Clone`, not a wire type (`session/policy.rs:18-33`) |
| `SpawnLink.writes` | `Option<Vec<String>>` | `serde(default)`, `None` omitted; persisted in the child header (`store/entry.rs:112-115`, `session/spawn.rs:457-469`) |
| `SpawnContext.writes` | `Option<&[String]>` | The parent's effective scope handed to spawn hooks (`session/spawn.rs:153-154`) |
| `SessionTools::write_scope()` | `Option<Vec<String>>` | Current effective scope (`tools/writes.rs:77-84`) |

**Definition.** An entry is a registered scheme URL prefix (`local://`, `local://notes`) or a directory path, relative to the session cwd or absolute. Absent is unrestricted; `[]` permits nothing. An explicit YAML null (`writes must be a list, not null`) and a blank entry (`writes entries must not be empty`) are definition errors, so the definition does not load (`context/markdown.rs:12-27`; `context/modes.rs:388-421`; `context/agents.rs:468-471`). An unknown scheme normalizes away and permits nothing (`tools/writes.rs:14-30`; `docs/prompts-and-context.md:492-499`).

**Effective scope.** The intersection of the agent (root persona, or the child's own spec), the effective mode and the inherited ceiling (`session/policy.rs:51-68,173-186`). Of each overlapping pair the narrower entry is kept (`tools/writes.rs:56-74`). Containment is per path component, so `/a` does not contain `/ab` (`tools/writes.rs:32-54`). A URL entry and a path entry never contain each other, even for the same directory: composition can narrow more than expected, never widen. `rescope` recomputes it on agent and mode changes and installs it in both `ToolScope` and the session's tool state (`session/policy.rs:188-214`).

**Inheritance.** A spawn records the parent's effective scope under the mode applied to that spawn (`session/spawn.rs:365-366,399-400,428-429,454-466`) on `SpawnLink.writes`; every descendant intersects against that persisted ceiling and cannot widen it, and it outlives the parent leaving the mode (upstream test `tests/spawn.rs:560` `bash_and_write_ceilings_outlive_the_parent_mode`). `local://` stays symbolic: in a child it means the child's own local directory, not the parent's (`tools/writes.rs:1-5`; `store/entry.rs:112-113`).

**Enforcement.** `write` and `edit` call `check_write` on the located target before mutating (`tools/write.rs:86-87`, `tools/edit.rs:109-110`). File targets compare real paths, so a symlink inside an allowed directory cannot escape it; the check is point-in-time, not protection against a concurrent symlink swap. Virtual targets pass only under a matching URL entry and never with a `..` segment. A refusal is a tool error: `<target> is outside the write scope (<entries>)`, or `(no writes allowed)` for `[]` (`tools/writes.rs:86-131`; upstream test `tests/modes.rs:1474` `mode_write_scope_confines_write_and_edit_by_real_path`).

**Not covered by W.** `writes` does not restrict `bash`; read-only bash (`bash: readOnly`, Landlock) is separate and the bundled modes need both (`docs/prompts-and-context.md:482-494`).

**Axome:** Stage B3's bundled modes declare `writes` with the requested syntax (`local://`, `.axome/research/`) unchanged; `ChatModeInfo.writes` reads `ModeSpec.writes`. `ModeSpec.extra` still flattens unknown keys (`context/modes.rs:115-116`), so `axome: { plan: true }` is unaffected.

## 3. Stage B contracts re-checked at `b961066`

Unchanged in this commit, re-read because Stage B4 depends on them:

- `Session::accept_result(&self, call_id: &str, options: AcceptResultOptions) -> Result<(), Error>` is synchronous. A `replacement` is validated against the pending result's schema before settlement; an invalid one returns `Error::InvalidInput` and leaves the review pending. `AcceptResultOptions { replacement: Option<Value>, message: Option<String> }`; `PendingResult { call_id, mode, result, schema }` (`session/modes.rs:345-391`). B4's rollback of the saved file on an acceptance error is therefore required.
- `Session::revise_result(&self, call_id: &str, feedback: String)` rejects blank feedback with `InvalidInput` (`session/modes.rs:368-372`).
- `Session::set_mode(&self, Option<&str>)` is root-only (`InvalidInput("Modes belong to root sessions")`), takes effect at the next request boundary, and selecting a mode supersedes pending reviews (`session/modes.rs:318-344`). A reviewed `yield` is `Replay::Safe` (`session/modes.rs:520-524`).
- `Activity { running, maintenance }` (`session/delivery.rs:50-54`). Standalone `compact` reserves activity and returns `Error::Busy` when a run or maintenance owns it (`session/maintenance.rs:55-66,603-610`); observing idle does not reserve it (`session/mod.rs:2585-2588`). B4's wait for `running == false && maintenance == None` plus its bounded `Busy` retry stays as planned.
