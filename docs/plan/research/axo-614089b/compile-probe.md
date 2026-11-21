# Compile probe — axo a0ec20d..614089b — 2026-11-20

## 1. What changed in axo

Paths prefixed `axo:` are relative to `/repo/rust/axo` at `614089b8a801d26b04e65330abc47924f4973cf1`; other source paths are relative to Axome.

Breaking APIs relevant to this probe:

- `Models::all(&self) -> Arc<[Model]>` replaces the iterator-returning API (`axo:crates/axo-ai/src/models/mod.rs:351-354`). `Models::find(&self, selector: &str) -> Option<Model>` now returns owned models (`:342-349`). The latter produces no diagnostic in Axome's existing `engine.rs:19-22` context-window lookup.
- `ProviderInfo::oauth` becomes `login: bool` (`axo:crates/axo-ai/src/models/mod.rs:43-48`), indicating support for registry-mediated login, not specifically OAuth.
- Free `auth::codex::login` and `refresh` are removed. `Models::login(&self, provider: &str, interaction: &dyn LoginInteraction) -> Result<(), Error>` handles credential storage (`axo:crates/axo-ai/src/models/mod.rs:392-415`). `CodexLogin::new(reqwest::Client)` implements `ProviderLogin`, whose refresh receives a whole `Credential` and returns a `Credential` (`axo:crates/axo-ai/src/auth/codex.rs:40-57,121-140`; `auth/provider.rs:30-36`). Interaction methods are `open_url`, `device_code`, `prompt`, `select`, and `progress` (`auth/provider.rs:41-52`).
- `Session::withdraw(&self, submission_id: &str) -> Result<QueuedInput, Error>` replaces `Result<Vec<PromptPart>, Error>` (`axo:crates/axo/src/session/delivery.rs:495-527`). `QueuedInput` adds `custom: Option<SubmittedCustom>`; custom messages must be resubmitted through `Session::post`, not `steer` (`axo:crates/axo/src/session/mod.rs:397-405`).

Additional serialized changes examined: OAuth credentials add defaulted `headers` and `baseUrl` (`axo:crates/axo-ai/src/auth/mod.rs:36-49`); persisted `SpawnLink.writes` changes from `Option<Vec<String>>` to `Option<WriteScope>` (`axo:crates/axo/src/store/entry.rs:113-136`), serialized as nested layers (`axo:crates/axo/src/tools/writes.rs:32-37`). These are not automatically propagated as frontend fields; see section 2.

## 2. Where Axome touches it today

### Probe method and result

- Axome `HEAD`: `4356018`. Initial `git status --short` was empty; no uncommitted Rust files needed copying.
- Created detached `/tmp/axome-probe-614089b` at HEAD and changed only its three workspace dependency revisions (`Cargo.toml:28-30`) to the full upstream hash above. Cargo fetched the git source successfully; no local patch fallback was needed.
- Command, from the throwaway worktree:

  ```sh
  CARGO_TARGET_DIR=/tmp/axome-probe-target nix develop /repo/rust/Axome -c cargo check --workspace --all-targets --message-format short
  ```

- Exit: **101**. Wall time: **66.34 seconds** (shell timer: 66 seconds), including Nix shell startup, git update, and compilation. Output retained at `/tmp/axome-probe-614089b.log`.
- **10 distinct source diagnostics: 7 production errors, 3 additional test-only errors.** Cargo reports `axome-core` lib failing with 7 errors and lib test with 10 errors: 17 target-level errors, not 17 unique sites. Short-format output printed 13 error-location lines because three iterator diagnostics were repeated with differing `Arc` qualification; other common diagnostics were deduplicated.
- **No Rust warning diagnostics emitted.** The only Cargo warning was `build failed, waiting for other jobs to finish...`; this is not a new source warning. No baseline run was needed to classify emitted source warnings because there were none. This failed probe does not establish warning cleanliness or successful checking of downstream targets blocked by `axome-core`.
- Worktree removed successfully with `git worktree remove --force`; the real Axome manifest and lockfile were not edited. No compatibility fixes were applied or rechecked.

### Exact diagnostic set, grouped by cause

All paths below begin `src-crates/axome-core/src/services/axo/`.

| Root cause | Distinct production errors | Additional test-only errors | Exact sites and diagnostics | Upstream replacement |
|---|---:|---:|---|---|
| Removed Codex free login function | 1 | 0 | `login.rs:61:47` E0425: cannot find function `login` in module `axo_ai::auth::codex` | `Models::login`, `axo:crates/axo-ai/src/models/mod.rs:394-404`; interaction trait `auth/provider.rs:41-52` |
| Removed Codex free refresh function | 1 | 0 | `web/auth.rs:160:52` E0425: cannot find function `refresh` in module `codex` | `CodexLogin` implementation of `ProviderLogin::refresh(Credential)`, `axo:crates/axo-ai/src/auth/codex.rs:121-140` |
| `Models::all()` now returns an `Arc` snapshot, not an iterator | 3 | 0 | `providers.rs:13:22` E0277: `Arc<[axo_ai::Model]>` is not an iterator; `mod.rs:688:22` E0277: same; `mod.rs:696:14` E0599: same | Snapshot API `axo:crates/axo-ai/src/models/mod.rs:351-354` |
| Provider capability field renamed | 1 | 0 | `providers.rs:35:33` E0609: no field `oauth` on type `ProviderInfo` | `ProviderInfo.login`, `axo:crates/axo-ai/src/models/mod.rs:43-48` |
| Withdrawal returns full queued input | 1 | 0 | `mod.rs:1018:35` E0308: expected `&[PromptPart]`, found `&QueuedInput` | `axo:crates/axo/src/session/delivery.rs:497` |
| New required Rust struct field in queued-input test literals | 0 | 3 | `convert.rs:1706:17`, `1713:17`, `1720:17`: each E0063, missing field `custom` in initializer of `QueuedInput` | `axo:crates/axo/src/session/mod.rs:399-405` |
| **Total** | **7** | **3** | | |

The three test-only errors are all in `queue_at_attach_maps_snapshot_queued_in_delivery_order` (`convert.rs:1697-1726`). No missing `Credential` initializer fields, model-literal fields, enum exhaustiveness errors, or errors caused by owned `Models::find` results were emitted. This is the exact observed error set, not a claim that fixing these sites makes every workspace target pass.

### Frontend wire boundary/drift

- **Models:** Axome explicitly maps upstream models into `ChatModel` (`src-crates/axome-core/src/services/axo/mod.rs:693-708`); Rust protocol fields (`src-crates/axome-protocol/src/chat.rs:19-28`) match TypeScript (`src/lib/types/backend.ts:179-187`). The snapshot ownership change does not itself change frontend JSON shape. Upstream `model.rs` had no diff in this revision range.
- **Provider capabilities:** Rust and TypeScript still agree on `oauth: bool` (`src-crates/axome-protocol/src/axo.rs:12-18`; `src/lib/types/backend.ts:192-199`), but its upstream source field no longer exists (`providers.rs:35`). This is **semantic drift**: new `login` means generic login support. It is not an already-existing Rust/TypeScript key mismatch.
- **Login interactions:** Current protocol only returns an initial URL plus pending/done/failed status (`src-crates/axome-protocol/src/axo.rs:40-50`; `src/lib/types/backend.ts:211-216`). The current integration is Codex-only and waits for an authorize URL (`login.rs:39-43,97-118`); the new upstream interaction trait additionally carries device codes, prompts, choices, instructions, and progress (`axo:crates/axo-ai/src/auth/provider.rs:41-79`). No such new fields automatically cross this host-defined wire boundary.
- **Queued/withdrawn input:** `ChatQueued` remains `{submissionId, kind, parts}` in both protocol and frontend (`src-crates/axome-protocol/src/chat.rs:350-354`; `src/lib/types/backend.ts:390-394`). Attach conversion ignores `custom` (`convert.rs:848-864`). Withdrawal still promises only `Vec<ChatPart>` / `Promise<ChatPart[]>` (`mod.rs:1005-1018`; `src/lib/api/backend.ts:254-255`). Therefore the new upstream custom-message metadata is **not represented** in either queue or withdrawal wire payloads; the two frontend/backend declarations remain mutually consistent but narrower than upstream.
- **Entries and credentials:** The changed spawn write ceiling is persisted upstream, not projected by the entry converter, which explicitly constructs `ChatEntry` and excludes `ChildStart` (`convert.rs:530-533,734-743`). Existing thinking-level journal changes already map to `ThinkingChange` (`convert.rs:689-691`). Provider responses expose credential source, not OAuth credential values (`providers.rs:18-37`; protocol `axo.rs:12-35`), so new credential `headers`/`baseUrl` do not produce a frontend shape change through that boundary.

## 3. What needs work in Axome

1. **Snapshot iteration — breaking compatibility.** Update the three `all()` iterator consumers to iterate the returned slice. Provider counting borrows provider names (`providers.rs:12-14`), so the snapshot must outlive those borrowed keys through the subsequent provider loop. Surface: model catalog/provider settings; no JSON shape change is required by the ownership change.
2. **Provider capability mapping — breaking compatibility plus semantic drift.** Replace the removed upstream `oauth` field access with the new login capability contract. Surface: provider settings. Open question: does the host's `oauth` wire name continue to denote only Codex OAuth, or generic provider login? A mere key substitution does not settle that distinction.
3. **Login call migration — breaking compatibility.** Replace the removed free login call with the registry login/interaction contract, retaining credential persistence and cancellation semantics. Surface: provider sign-in; current URL/status-only wire shape does not express all new interaction methods. Open question: which provider interaction capabilities are in scope? No UI design is prescribed here.
4. **Grounded-search token refresh — breaking compatibility.** Replace the removed refresh call with the new `ProviderLogin::refresh` contract, which consumes the complete credential, not just its refresh-token string. Existing refresh occurs inside a credential-store update (`web/auth.rs:143-165`); preserving that locking boundary matters. Surface: web search authentication; no frontend field addition follows from this compile error.
5. **Withdrawal conversion — breaking compatibility.** Consume `QueuedInput` rather than treating the return value as a parts vector. Surface: queued-message editing/resubmission. For ordinary text inputs the parts remain available; custom inputs additionally carry a resubmission contract that the current wire cannot express. Open question: how are custom queued messages represented or admitted to host editing? Discarding `custom` silently would not constitute complete adoption.
6. **Queued-input test literals — test-only compatibility.** Add the new `custom` field to the three ordinary prompt fixtures (their existing data represents non-custom input). No production/frontend shape change is implied by these three diagnostics alone.

No design proposals or source fixes were made. A successful all-targets check remains unverified because this was a diagnostic-only dependency bump.
