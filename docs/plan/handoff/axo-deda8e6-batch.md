# axo requests from Axome's `deda8e6` adoption

- **Target repository:** `/repo/rust/axo`, branch `main`. Toph makes these changes; Axome's implementers never modify axo.
- **Inspected revision:** `deda8e6770a3cfa81589b276f4f0980e0fa5efcc`
- **Evidence:** [`../research/axo-deda8e6/`](../research/README.md)
- **Status:** all five items landed in `118a716` (2026-11-23), verified by Axome against source. See [`../plan.md`](../plan.md) T13 "Requested fixes".
- **Format:** each item states the facts at `deda8e6` and what Axome needs. The API shape is axo's call.

| # | Area | Summary |
|---|---|---|
| F | axo-extensions, axo-ai registry | Project extensions of one folder reach every other folder's sessions and requests |
| P | session submissions | Input an extension sends is recorded as if the user typed it |
| S | axo-ai registry | No signal when the model list changes (carried over) |
| L | axo-ai auth | No logout that also drops account-scoped model state (carried over) |
| U | axo-ai cost | Unknown prices read as $0 (carried over, optional) |

---

## F. Folder-scoped extensions

**Facts at `deda8e6`**

- One `Extensions` manager has one `ExtensionOptions.roots` list, fixed at `Extensions::new` (`crates/axo-extensions/src/lib.rs:76-110,362-386`). `configure` takes only the enabled list (`lib.rs:394-401`).
- A plain id resolves to the **first** root containing `<root>/<id>/extension.json`; later roots only produce a duplicate diagnostic (`crates/axo-extensions/src/manifest.rs:206-248`). One id therefore maps to one package per manager.
- `compose` gives every session the same manager-wide contribution list and ignores the session's cwd (`crates/axo-extensions/src/session.rs:99-106`). `attach` applies that same list (`session.rs:111-158`), and reconfiguration reapplies it to every attached root (`session.rs:265-318`).
- Providers are published into the engine's **shared** model registry through one provider layer (`lib.rs:367`, `lib.rs:1152`), so any provider an extension declares is visible to every session and every request on that engine.
- Extension data lives at `<data_root>/<id>` (`lib.rs:102-108`), so two folders' authored packages with the same id share one data directory.
- Axome runs one engine and one model registry per host for all open folders (Axome decision P21), with many folders open at once in tabs.

**Consequence for Axome.** If a project root `<folder>/.axome/extensions` were added to the manager, folder A's project extension would apply to folder B's sessions:
- its tools, hooks, commands, sections and definitions;
- its providers, which can replace a provider id for every request on the host.

If A and B each ship an extension with the same id, only one of them runs, for both folders. A second manager per folder separates tools but not providers, because both managers publish into the one registry. A second engine per folder would duplicate the registry, catalog refresh and session ownership that P21 keeps single.

**What Axome needs**

- Project extensions that contribute only to sessions whose folder they belong to, alongside user and installed extensions that apply host-wide.
- Project providers visible only to that folder's sessions and requests, with credentials kept per folder, while the host keeps one model registry.
- Two folders able to ship different packages under the same id at the same time.
- Sessions an extension creates, opens or forks through `Lifecycle` get the target folder's extensions, not the calling extension's folder.
- Forks and children keep the scope of their root.
- Axome decides which folders are trusted (Workspace Trust, Axome decision D4). axo does not need to know about trust or tabs.

## P. Provenance for extension-sent input

**Facts at `deda8e6`**

- Extensions can call `prompt`, `steer`, `followUp` and related session methods through `Session::invoke` with a `Caller { namespace }` (`crates/axo/src/ext.rs:16-20`; SDK `packages/axo-extension-sdk/bindings.ts:4990-5110`).
- The namespace is applied to custom types and posts (`crates/axo/src/session/wire.rs:52-92`), not to submissions.
- `SubmissionProvenance` (`crates/axo/src/coordination.rs:24`) is set only for routed agent `send` (`crates/axo/src/session/coordination.rs:60-131`). Other submissions record `provenance: None` (`crates/axo/src/session/delivery.rs:549,614-624`).

**What Axome needs.** A way to tell, from the journal and live events, that a submission came from extension `<id>` rather than from the user, so the transcript can show who sent it, including after reopen.

## S, L, U. Carried over from [`axo-models-batch.md`](axo-models-batch.md)

None of the three is in `deda8e6` (verified in [`../research/axo-deda8e6/providers-credentials-and-tiers.md`](../research/axo-deda8e6/providers-credentials-and-tiers.md) § Pending S / L / U).

- **S:** the registry still changes on its own: timer refresh, Codex account list after login, `reload`, `refresh`, `register`/`unregister`. `State::rebuild` notifies nobody (`crates/axo-ai/src/models/mod.rs:113-123,222-237`). Axome needs to know when the published model list changed. With extensions this matters more, because extension providers come and go on `configure`, `reload` and runner failure.
- **L:** there is no registry logout. The discovered Codex account list survives credential removal (`models/mod.rs:457-464`). Axome needs logout to remove the credential from its current slot and drop that account's model list. With per-origin slots (`loaded/{id}`, `registered/{owner}/{id}`, `layer/{owner}/{id}`), Axome otherwise has to compute the slot itself through `Models::credential_slot`.
- **U (optional):** `UsageCost` is still plain `f64` (`crates/axo-ai/src/types.rs:295-304`); a missing feed price becomes 0 (`models/rules.rs:330-338,479-482`). Axome needs to tell "price unknown" from "free", on models and on usage.

## When it lands

Axome bumps the three axo revs in `Cargo.toml`, then adopts each item in [`../plan.md`](../plan.md) T13. If axo names things differently, Axome uses axo's names.
