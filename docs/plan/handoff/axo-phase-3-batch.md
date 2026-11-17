# axo Phase 3 Batch for Axome

> **Fulfilled in axo `b9610666ea6f04a55be3a44b1d6d0f7f15531c06`** ("feat: add write scopes and record submission keywords", 2026-11-15; parent `cd121629`). K and W landed under the requested names and types: `Entry::Submission.keywords` (`crates/axo/src/store/entry.rs:425-427`); `ModeSpec.writes`, `AgentSpec.writes`, `ToolScope.writes`, `SpawnLink.writes` (`crates/axo/src/context/modes.rs:29-31`, `crates/axo/src/context/agents.rs:64-68`, `crates/axo/src/session/policy.rs:30-32`, `crates/axo/src/store/entry.rs:112-115`), enforced in `write`/`edit` (`crates/axo/src/tools/writes.rs:86-131`). Verified APIs and semantics: [`../research/axo-b961066/host-capability-handoff.md`](../research/axo-b961066/host-capability-handoff.md). Stage B is implemented and verified. The request and the evidence below are kept as written.

- **Target repository:** `/repo/rust/axo`, branch `main`. Toph makes these changes; Axome's implementers never modify axo.
- **Inspected revision:** `cd121629365ff5f8fba5003146155b50168cb923`
- **Source decisions:** [`../design.md`](../design.md) ([P17](../design.md#decisions), corrections to `D8` and `D10`) and [`../plan.md`](../plan.md) ([T10](../plan.md#t10-remaining-axo-gaps))
- **Gate:** Phase 3 Stage B of [`.axome/plans/AXO_ADOPTION_ROADMAP_AXOME_ADOPT_PLAN.md`](../../../.axome/plans/AXO_ADOPTION_ROADMAP_AXOME_ADOPT_PLAN.md#phase-3--agent-experience)

Phase 3 Stage A (personas, modes in the composer, generic reviews, keyword highlighting, frontmatter editing, routed-input labels, standalone maintenance line) runs on the current pin and is implemented and verified (record in the roadmap's Phase 3 status). Phase 3 is not complete: Stage B (the sent-message keyword chip, `plansDir`, the bundled `/research` and `/plan*` modes, saving and accepting plans, the Plan view) needs the two items below, is not started, and is blocked until axo `main` has both.

## Gate state when written

Observed from `/repo/rust/axo` after `git fetch origin`:

- `git status -sb`: `main...origin/main`, clean.
- `HEAD` == `origin/main` == `cd121629365ff5f8fba5003146155b50168cb923`, the revision Axome already pins (`Cargo.toml:28-30`). Nothing new has landed.
- No `keywords` field on `Entry::Submission` (`crates/axo/src/store/entry.rs:413-431` has `submission_id`, `kind`, `parts`, `request_id`, `sender`, `provenance`, `custom`).
- No `writes` on `ModeSpec`, `AgentSpec` or `ToolScope` (`crates/axo/src/context/modes.rs`, `crates/axo/src/context/agents.rs`, `crates/axo/src/session/policy.rs:18-28`).

Both items are absent, so Stage B is not started.

## Summary of Items

| # | Area | Summary | Axome consumer |
|---|---|---|---|
| K | Journal | `Entry::Submission.keywords`: the keyword names that fired for this submission | Keyword chip on sent messages (Stage B1) |
| W | Tool policy | Frontmatter `writes: [...]` on modes and agents, enforced in `write`/`edit`, inherited by spawns as a narrowing ceiling | `writes` in the bundled mode files and the Toolbox mode details (Stage B3) |

---

## K. Fired keywords

**Problem.** Keyword injection appends a hidden `<keyword name="…">` text block to the submission (`crates/axo/src/input.rs:304-307`) and records nothing typed. `Session::keyword_matches(text)` (`crates/axo/src/session/mod.rs:2256`) reports what *would* match now, under the current toggles, not what fired when the message was sent. A host that wants to show which keywords a sent message triggered, including after reopen, can only re-run the matcher (wrong once toggles or definitions change) or parse the injected text.

**Required change.**

- `Entry::Submission` gains:
  ```rust
  /// Names of the keywords that fired for this submission, deduplicated, in first-occurrence order.
  #[serde(default, skip_serializing_if = "Vec::is_empty")]
  keywords: Vec<String>,
  ```
- Filled when the submission is recorded, from the same matching that decides injection. Model-only keywords (a `model` selector and an empty body, the shape axo's own test parses at `crates/axo/src/context/keywords.rs:481-485`) are included even though `input.rs:302` injects no notice text for them.
- Old journals read as an empty list (`serde(default)`).

## W. Write scope

**Problem.** Plan and research modes must not write project files, and that must hold for every subagent they spawn. axo restricts tools by name only, and bash by `BashAccess`. An Axome `ToolPolicy` keyed on the mode cannot do this: policies do apply in children, but the mode identity does not follow a spawn. Children are created with `modes: Vec::new()` (`crates/axo/src/session/spawn.rs:201`), so `ToolScope.mode` is `None` in every subagent. The only restrictions that follow a spawn are the ones axo owns and stores on the link: `SpawnLink.limits` and `SpawnLink.bash` (`crates/axo/src/store/entry.rs:90-111`, `crates/axo/src/session/spawn.rs:149-152`).

**Required change.**

- Frontmatter `writes: [<entry>…]` on modes and agents.
  - An entry is `local://` (the session's local scheme) or a directory path, relative to the session cwd or absolute.
  - Absent means unrestricted. `[]` means no writes.
- `write` and `edit` to a target outside every entry fail with a tool error. Resolution uses axo's own path and scheme resolution, so `local://` and relative paths mean what they mean to those tools.
- The effective scope is the intersection of the mode's, the agent's and the inherited ceiling. It is stored on `SpawnLink` and inherited by children and nested spawns as a ceiling that can only narrow, exactly like `SpawnLink.bash`.
- Exposed as `ModeSpec.writes`, `AgentSpec.writes` and `ToolScope.writes`.

## What Axome does meanwhile, and what it will not do

Stage A is complete and verified on `cd12162`. Stage B waits. No stand-in is built for either item:

- **No `ToolPolicy` for write scoping.** It keys on `ToolScope.mode`, which is `None` in every child, so a `/plan` agent could escape it with a single `spawn`. A scope that leaks through delegation is worse than none, because the mode's instructions and UI would promise a guarantee that does not hold.
- **No `spawn` denial in plan modes.** Blocking `spawn` would make the policy hold only by removing `scout` delegation, which the plan and research modes rely on for investigation. It changes the workflow to hide an engine gap, and every later spawn-capable tool would need the same patch.
- **No parsing of `<keyword name="…">` text.** The block is model-facing prompt text with no stable contract; matching on it couples the transcript to axo's prompt wording and breaks silently when that changes, and a model-only keyword injects no block at all, so it could never be found. Re-running `keyword_matches` on old messages is equally wrong once toggles or definitions change.

## When it lands

From `/repo/rust/axo`:

```
git fetch origin && git status -sb          # main == origin/main
rg -n "keywords" crates/axo/src/store/entry.rs
rg -n "writes" crates/axo/src/context/modes.rs crates/axo/src/context/agents.rs crates/axo/src/session/policy.rs
```

With both present, Stage B starts by bumping the three axo revs in Axome's `Cargo.toml:28-30` to `git -C /repo/rust/axo rev-parse origin/main` and fixing any compile errors with plain explicit handling. The field names above are the request; if axo names them differently, Axome uses axo's names (consumers: the keyword chip, the bundled mode frontmatter, the Toolbox `writes` line). If W's entry syntax differs, only the `writes` lists in the four bundled mode files change. If axo declines W, the plan and research modes do not ship.
