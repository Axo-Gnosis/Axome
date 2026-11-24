# Research behind Axome's axo adoption

Surveys of axo as Axome sees it. Each was made by a read-only subagent against axo source code, not its docs (which were being rewritten at the time), and is kept verbatim as evidence. Each file names the axo revision it describes; re-check the code before relying on a claim if axo has moved since.

- **Labels:** claims marked [INFERENCE] were not verified directly.
- **Axome side:** reports cite Axome at its HEAD of the day where they mention it. Axome had not changed between the `6cdaeb4` and `843cd73` surveys; later surveys follow Axome's own adoption work.

## axo `deda8e6` (2026-11-22): pin-bump survey from `5c3f418`

Started as an inventory; the last four reports are recommendations answering Toph's review. The topic is [T13](../plan.md#t13-axo-deda8e6-extensions-definitions-service-tiers).

| File | Question |
|---|---|
| [`axo-deda8e6/compile-probe.md`](axo-deda8e6/compile-probe.md) | What breaks when Axome HEAD compiles against `deda8e6` (20 production + 6 test errors by cause), silent wire drift (`details.files`), and what happens to existing auth slots, child journals and tool records on upgrade |
| [`axo-deda8e6/session-api-and-definitions.md`](axo-deda8e6/session-api-and-definitions.md) | `SessionOptions::definitions`, live definitions and layers, registered commands, `axo::ext` dispatch, paging, `Arc<SessionEvent>`/`QueueChanged`, child observation, one-off completions, changed tool trait and errors |
| [`axo-deda8e6/behavior-data-and-config.md`](axo-deda8e6/behavior-data-and-config.md) | Behavior Axome feels without new calls: journal/header compatibility, delivery and mode ordering, file-record envelope and hard-stop records, bounded reads, command capture, prompt layout, new settings keys, error surfaces |
| [`axo-deda8e6/providers-credentials-and-tiers.md`](axo-deda8e6/providers-credentials-and-tiers.md) | Per-origin credential slots, owned registrations and layers, transactional reload/refresh, service tiers end to end (selectors, `/tier`, settings, journal, transport, cost), and whether handoff items S/L/U landed |
| [`axo-deda8e6/extensions.md`](axo-deda8e6/extensions.md) | The Bun extension bridge as an embedding host sees it: `ExtensionOptions`, manager status/events, install/update/rollback, contributions, callable API, lifecycle hook, failure and trust model, and where it meets Axome (including Axome's own unrelated `extension.json` system and Bun packaging) |
| [`axo-deda8e6/service-tier-ux.md`](axo-deda8e6/service-tier-ux.md) | Q66: where tiers are chosen and shown (Codex, Claude Code, OMP, OpenCode, Zed, Cursor compared); chosen, effective and recorded tier; selector, Settings and turn-footer changes |
| [`axo-deda8e6/extension-runtime-inputs.md`](axo-deda8e6/extension-runtime-inputs.md) | Q70: Bun packaging, directories, env, engine-owned manager, the `Lifecycle` hook over tabs and folders, `axome-server`; proposed axo handoff for folder-scoped extensions. Its `auto_install: false` is overridden in T13 |
| [`axo-deda8e6/extension-management-ux.md`](axo-deda8e6/extension-management-ux.md) | Q71: how OMP, pi, Claude Code, OpenCode, Goose, Zed, VS Code and Obsidian manage extensions; the recommended Settings → Agent → Extensions page, install sources, trust, diagnostics |
| [`axo-deda8e6/extension-systems-clash.md`](axo-deda8e6/extension-systems-clash.md) | Q72: every clash between Axome's language/LSP/harness packages and axo's Bun extensions, four resolution options, and the recommended rename to Integrations |

## axo `614089b` (2026-11-20): pin-bump survey from `a0ec20d`

| File | Question |
|---|---|
| [`axo-614089b/compile-probe.md`](axo-614089b/compile-probe.md) | What breaks when Axome compiles against `614089b`: 7 production + 3 test errors by cause, plus frontend wire drift |
| [`axo-614089b/providers-and-login.md`](axo-614089b/providers-and-login.md) | The runtime provider, generic login and request-rewrite seam (`54dccae`, axo-ai parts of `0f4f890`) and Axome's login, provider and web-auth code |
| [`axo-614089b/model-catalog.md`](axo-614089b/model-catalog.md) | models.dev v2 mapping, Codex account model list, catalog cache/refresh/timer, open sessions adopting changed definitions (`614089b`) |
| [`axo-614089b/session-and-write-scopes.md`](axo-614089b/session-and-write-scopes.md) | Layered write scopes, `withdraw` returning `QueuedInput`, delivery fixes, review-detail keys, pipe capture, squash concurrency (`0f4f890`, axo crate) |
| [`axo-614089b/harness-login-and-availability.md`](axo-614089b/harness-login-and-availability.md) | How pi, OMP, OpenCode, Codex, Claude Code and Zed present login (URL, paste, device code, prompts, select, remote), credential provenance, and models that disappear or the account lacks |
| [`axo-614089b/harness-model-catalog.md`](axo-614089b/harness-model-catalog.md) | How the same harnesses load, refresh and push model catalogs, report offline/auth/config states, clamp effort, and show unknown or tiered prices and extended context |
| [`axo-614089b/legacy-journals-and-queue.md`](axo-614089b/legacy-journals-and-queue.md) | The 9 local child journals with legacy `writes`, the blast radius at `614089b`, exactness of a one-layer read; whether custom `QueuedInput` can reach Axome today |
| [`axo-614089b/axome-code-answers.md`](axo-614089b/axome-code-answers.md) | Axome-side facts for the open questions: engine rebuild vs live sessions, model caches and push channels, login path and remote callback, provider cards, grounded search auth, empty/unavailable states, change entries, cost display |
| [`axo-614089b/memory.md`](axo-614089b/memory.md) | 2026-11-21: two live Hindsight bank exports, 46 session questions, measured recall quality/latency and provider cache reads, pinned harness comparison, and the host-owned memory decision |

## axo `b961066` (2026-11-15): Phase 3 batch survey

| File | Question |
|---|---|
| [`axo-b961066/host-capability-handoff.md`](axo-b961066/host-capability-handoff.md) | Did the Phase 3 batch land as requested: fired keywords on `Entry::Submission` (K) and inherited `writes` scopes (W); their exact APIs and semantics, and the review and activity contracts Stage B re-checked at that revision |

## axo `cd12162` (2026-11-13): delta survey

| File | Question |
|---|---|
| [`axo-cd12162/host-capability-handoff.md`](axo-cd12162/host-capability-handoff.md) | What axo added after `97513d6` for Axome: activity signal, typed receipts and host `send`, prompt/context/ledger inspection, labeled command output, effective mode and edited results, retry detail, project skill roots; and which phase uses each |

## axo `843cd73` (2026-11-10): current baseline

| File | Question |
|---|---|
| [`axo-843cd73/host-surface.md`](axo-843cd73/host-surface.md) | What the typed host surface gives: events, snapshot, job rows, live command output, children, personas, catalog, host messages; §8 audits Axome's 35-point host-surface request (IDs A1–F5) |
| [`axo-843cd73/tools.md`](axo-843cd73/tools.md) | The tool model: `kill`, `send`, `call`, `tool://`, deferred host tools, `ToolPolicy`, result `details` per tool, how Axome renders tools today |
| [`axo-843cd73/modes.md`](axo-843cd73/modes.md) | Session modes: definitions, entry and exit, tool limits, model rule, result review, children, how they relate to personas and `@roles` |
| [`axo-843cd73/compile-probe.md`](axo-843cd73/compile-probe.md) | What breaks when Axome compiles against axo HEAD: 38 errors grouped by cause, plus frontend wire-key drift |

## axo `6cdaeb4` (2026-11-09): partly superseded

| File | Question | Still current for |
|---|---|---|
| [`axo-6cdaeb4/config-and-embedding.md`](axo-6cdaeb4/config-and-embedding.md) | Host-owned config, `Axo::builder`, axo-ai changes, error enum, tool trait | Construction, `ConfigSource`, credentials, axo-ai |
| [`axo-6cdaeb4/jobs.md`](axo-6cdaeb4/jobs.md) | Command supervision and background jobs | Supervision modes, promotion and delivery mechanics |
| [`axo-6cdaeb4/agents.md`](axo-6cdaeb4/agents.md) | Selectors, agent definitions, children, spawn, teams | Definition format, spawn and team mechanics, child storage |

Job shapes, events, snapshot, persona persistence and child rows in the `6cdaeb4` reports are superseded by `axo-843cd73/host-surface.md`.
