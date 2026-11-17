# Research behind Axome's axo adoption

Surveys of axo as Axome sees it. Each was made by a read-only subagent against axo source code, not its docs (which were being rewritten at the time), and is kept verbatim as evidence. Each file names the axo revision it describes; re-check the code before relying on a claim if axo has moved since.

- **Labels:** claims marked [INFERENCE] were not verified directly.
- **Axome side:** reports cite Axome at its HEAD of the day where they mention it. Axome had not changed between the `6cdaeb4` and `843cd73` surveys; later surveys follow Axome's own adoption work.

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
