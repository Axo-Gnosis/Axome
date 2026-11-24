# Compile probe and persisted/wire compatibility

- axo revision: `deda8e6770a3cfa81589b276f4f0980e0fa5efcc` (`deda8e6`), base `5c3f418a5548f161aeb0af686b77992c55278811`.
- Axome HEAD: `48f5a03`.
- Date: 2026-11-22.
- Question: What fails to compile on the dependency bump, what silently changes across Axome's serialized boundaries, and what happens to existing persisted data?

Paths prefixed `axo:` refer to `/repo/rust/axo` at the evaluated revision. Other source citations refer to Axome HEAD, not concurrent working-tree changes. This is an inventory, not an adoption design. Runtime consequences below are source-traced, not claims of an executed upgraded application.

## 1. Probe method and observed result

Created `/tmp/axome-probe-deda8e6` from Axome HEAD using `git worktree add ... HEAD --detach`. The tool intercepted this as its worktree-clone operation, reported cross-device FICLONE unavailable and fell back to a plain checkout. Changed only the three axo dependency revisions in that checkout's `Cargo.toml:29-31`; Cargo also updated that checkout's lockfile. The real manifest, lockfile and source files were not edited. The only repository write by this investigation is this report.

Cleanup completed: `git worktree remove --force /tmp/axome-probe-deda8e6` and `git worktree prune` both returned successfully. The diagnostic log remains outside the repository.

Cargo was already on PATH; no Nix shell was needed. Command:

```sh
CARGO_TARGET_DIR=/repo/rust/Axome/target cargo check --workspace --all-targets --message-format short
```

Output was captured to `/tmp/axome-probe-deda8e6.log`; shell elapsed time **50.511 seconds**, exit **101**. Cargo fetched the requested git revision successfully and checked axo itself. No local source patch fallback or compatibility fixes were applied.

**26 distinct emitted source errors: 20 production, 6 additional test-only.** Cargo reports `axome-core` lib with 20 and lib-test with 26: 46 target-level errors, not 46 distinct sites. All 26 diagnostics are enumerated below. No Rust warning diagnostics were emitted; `build failed, waiting for other jobs to finish...` is only Cargo's failure notice. This does not establish cleanliness of downstream targets blocked by the core failure.

### Complete emitted error set, grouped by cause

Unless otherwise noted, Axome sites in this table are under `src-crates/axome-core/src/services/axo/`.

| Upstream cause | Production | Test-only | Exact emitted sites and diagnostics | Upstream contract |
|---|---:|---:|---|---|
| Tool prompt guidelines now use owned-or-borrowed text | 3 | 0 | `web/docs_tool.rs:94:36`, `web/search_tool.rs:98:36`, `src-crates/axome-core/src/services/memory/tools.rs:98:36`: E0053, expected `Cow<'static, str>`, found `&str` | `axo:crates/axo-agent/src/tool.rs:30`, return `&[Cow<'static, str>]` |
| Broadcast events are shared `Arc` values | 3 | 0 | `live.rs:892:55`, `893:58`: E0308, expected `Arc<SessionEvent>`, found `SessionEvent`; `897:45`: E0308, expected `SessionEvent`, found `Arc<SessionEvent>` | `axo:crates/axo/src/session/mod.rs:478-481,3202`, attach/subscribe receivers |
| Model catalog snapshot now contains `Arc<Model>` elements | 1 | 0 | `web/providers.rs:122:42`: E0308, expected `Option<&Model>`, found `Option<&Arc<Model>>` | `axo:crates/axo-ai/src/models/mod.rs:567`, `all() -> Arc<[Arc<Model>]>`; grounded-search preferred/catalog fallback joins incompatible references |
| Programmatic definitions grouped into `SessionOptions.definitions` | 4 | 3 | `mod.rs:427:13` (`agents`), `428:13` (`modes`), `429:13` (`keywords`); `src-crates/axome-core/src/services/memory/writer.rs:251:21` (`agents`); tests `tests.rs:868:13`, `869:13`, `870:13` respectively: E0560, removed fields | `axo:crates/axo/src/session/mod.rs:96-108` |
| Catalog also receives grouped definitions | 1 | 0 | `bundled.rs:82:9`: E0061, method takes 3 arguments, 5 supplied | `axo:crates/axo/src/axo.rs:413-419`, `catalog(cwd, access, &Definitions)` |
| Mode catalog accessor returns an owned vector snapshot | 2 | 0 | `live.rs:250:33`, `777:42`: E0308, expected `&[Arc<ModeSpec>]`, found `Vec<Arc<ModeSpec>>` | `axo:crates/axo/src/session/mod.rs:2672` |
| Session file modification time changes representation | 1 | 0 | `mod.rs:2161:41`: E0609, no field `modified` on `&axo::SessionInfo` | `axo:crates/axo/src/store/entry.rs:754-765`, `modified_at: i64`, Unix milliseconds |
| Registered commands add an enum case | 1 | 0 | `convert.rs:93:21`: E0004, `CommandKind::Registered { .. }` not covered | `axo:crates/axo/src/commands.rs:98-107` |
| Service-tier journal entry adds an enum case | 3 | 0 | `convert.rs:369:15`, `431:33`, `533:22`: E0004, `Entry::ServiceTierChange { .. }` not covered | `axo:crates/axo/src/store/entry.rs:321-325` |
| Service-tier fields in Rust test literals | 0 | 3 | `convert.rs:1216:25`: E0063, `AssistantMessage.service_tier`; `1699:28`: E0063, `LiveSnapshot.service_tier`; `1737:20`: E0063, `Model.service_tiers` missing | `axo:crates/axo-ai/src/types.rs:101`; `axo:crates/axo/src/session/mod.rs:510`; `axo:crates/axo-ai/src/model.rs:75` |
| New session errors | 1 | 0 | `mod.rs:147:11`: E0004, both `Error::Completion { .. }` and `Error::SkillNotFound(_)` not covered (one diagnostic) | `axo:crates/axo/src/error.rs:43-50` |
| **Total** | **20** | **6** | **26** | |

This is the complete **emitted** set, not a promise that addressing it completes compilation. Source inspection additionally finds currently exhaustive matches with no new case: `live.rs:504-832` lacks `SessionEvent::QueueChanged`; its entry match at `live.rs:521-645` lacks `ServiceTierChange`; `mod.rs:2093-2143` likewise lacks `ServiceTierChange`. The compiler did not emit those sites in this run. They are source-level compatibility obligations, not extra observed diagnostics and not silent wildcard handling.

## 2. Silent wire/semantic drift audit

### Tool details: actual untyped frontend break

Axome clones tool-result `details` into its protocol (`convert.rs:652`) and forwards update details (`live.rs:693-709`). Rust carries `Option<serde_json::Value>` (`src-crates/axome-protocol/src/chat.rs:277,338,789`), TypeScript carries `unknown | null` (`src/lib/types/backend.ts:376,398,682`). These nested changes therefore bypass Rust and TypeScript compile-time checks.

- **Read:** axo now emits `details.files: [{ledger, change?}]`, not top-level `details.ledger` (`axo:crates/axo/src/tools/ledger.rs:65-86`; `tools/read.rs:87-111,143-156`). Axome still reads `details.ledger` for normalized paths and proof that a user-limited read's continuation footer is safe to strip (`src/lib/features/chat/elements/tool/readOutput.ts:28-44,95-106`). New results lose the canonical ledger-path choice; ordinary argument-path fallback still exists. User-limited reads with a continuation footer fail the ledger guard and return `null` from the specialized renderer. Top-level truncation metadata is retained upstream, so this is not a blanket loss of all read rendering.
- **Write/edit:** axo's file-tool completion adds mutation records under `details.files` (`axo:crates/axo/src/tools/wire.rs:69-97,129-179`), with `change` nested inside each record. Axome's parser only reads top-level `details.change` (`src/lib/features/chat/elements/tool/payload.ts:81-100`), and `ToolDiff.svelte:25` consumes it. New edit/write results therefore produce no parsed inline file change. The nested `FileChange` fields themselves remain recognizable (`axo:crates/axo/src/tools/changes.rs:25-86`); the breaking difference is their containing list and support for multiple accesses/mutations.
- **Failed/hard-stopped mutations:** records now survive an operation that touched files and then failed or was hard-stopped (`axo:crates/axo/src/tools/wire.rs:85-97,100-126,154-164`; `tools/changes.rs:115-120`). This is a new meaningful payload case, not merely a key rename. Axome's summary header currently omits diffs for failed calls (`src/lib/features/chat/elements/tool/toolHeader.ts:139`).
- **Bash:** inspected details remain compatible: upstream still emits `exit`, `durationMs`, optional `fullOutput`, and promoted receipts with `job` (`axo:crates/axo/src/jobs/command.rs:584-586,799-801`); Axome reads those same fields (`src/lib/features/chat/elements/tool/bashOutput.ts:27-45`). Behavior changes: incomplete/drain-expired output no longer advertises a complete `fullOutput` URL (`axo:crates/axo/src/jobs/command.rs:754-777`), so availability of the existing UI link can change without a schema mismatch.

### Named serialized changes that do not directly leak through

- **`CommandPolicy.whenIdle`: no current frontend key mismatch.** axo changed its own serde casing (`axo:crates/axo/src/commands.rs:80-89`). Axome explicitly maps the Rust variants (`convert.rs:89-91`) into its own snake-case `ChatCommandPolicy` (`src-crates/axome-protocol/src/chat.rs:579-588`). Its TS union remains `when_idle` (`src/lib/types/backend.ts:559`) and the composer compares that exact value (`src/lib/features/chat/elements/prompt-input/PromptInputRoot.svelte:218,237,314,328`). The new registered command *kind* is a compile break, separately counted above.
- **`SessionInfo.modifiedAt`: not raw frontend JSON.** Axome maps it into its own history `updated_at` ISO timestamp (`mod.rs:2157-2161`). The missing Rust field is caught by the probe. axo derives this timestamp from file metadata; there is no persisted `modified` JSON key to rewrite (`axo:crates/axo/src/store/jsonl.rs:345-379`).
- **`WriteRoot` tagged shape:** there is no direct `WriteRoot`/`WriteScope` TS or protocol consumer in the searched Axome surfaces. The real exposure is stored session headers and upstream reopening, covered below; it is not a frontend deserializer change.
- **Structured completion failures:** new `CompletionDetails` carries failure, usage and stop reason (`axo:crates/axo/src/error.rs:5-12,43-45`). Axome's current error mapper converts to string-valued `ApiError` cases and does not model those details (`mod.rs:146-180`); retry handling already ignores fields beyond attempt/delay/count/error via `..` (`live.rs:738-750`). The enum arm absence is a compile failure; the availability of richer terminal/retry failure information is a separate host-projection gap.
- **Service tiers:** new serialized `service_tier_change` entries and `ThinkingLevelChange.automatic/serviceTier` (`axo:crates/axo/src/store/entry.rs:290-325`), assistant-message tier, live selected tier and model tier catalog now exist. Axome's entry conversion and live model/thinking state do not carry a selected tier (`convert.rs:533-743`; `live.rs:596-606`). The new entry variant causes compile errors, while ignoring additive fields via existing mappings would silently omit tier semantics. Axome has no current `serviceTier`/`service_tier` protocol/frontend consumer in the searched surfaces.
- **Queue changes:** `SessionEvent::QueueChanged` is new (`axo:crates/axo/src/session/mod.rs:264-265`), emitted by queue operations (`session/delivery.rs:506,660`). Axome reconstructs queue state from submission/message/withdrawal entries (`live.rs:522-576`) and initial attach (`convert.rs:848-864`). Its exhaustive event match has no queue-refresh arm; this is a source-confirmed additional compatibility gap, not an observed compiler error nor a silently ignored wildcard event.

axo owns the underlying journal/event/tool-detail contracts; Axome owns the chat/history projections and specialized renderers. The inventory does not choose which newly available fields/capabilities Axome should expose.

## 3. Existing persisted-data impact

### Credentials: old auth slots are no longer active provider credentials

Axome stores the shared credential file at `dirs.data/axo/auth.json` (`mod.rs:224`) and passes it to `Models::load` (`engine.rs:49-54`). axo still uses a JSON object and the same file/lock/atomic-replacement mechanism (`axo:crates/axo-ai/src/auth/store.rs:1-2,60-89,119-149`); **the keys' meaning changed**, not the whole-file JSON encoding.

Loaded providers now use `loaded/{id}` (`axo:crates/axo-ai/src/models/mod.rs:994,1038`); registered/layer providers use origin-owned slots (`models/mod.rs:310`). Authentication/status queries read `info.slot`, with no fallback to legacy provider-id keys (`axo:crates/axo-ai/src/auth/resolve.rs:121-134`; `auth/store.rs:121-124`). Existing `openai-codex`, `anthropic`, etc. stored slots therefore remain physically in the file but are not found for loaded-provider authentication. Existing OAuth users must sign in again under the new origin identity; configured/environment credentials remain separate fallback sources.

**Continuing silent host mismatch:** Axome's API-key setter and remove endpoint still use bare `provider` as the key (`providers.rs:63-85`). Consequently, even newly saved provider API keys go to inactive old slots, and removal does not remove a credential stored at `loaded/{id}`. axo offers the current origin key through `Models::credential_slot` and its login already writes the origin slot (`axo:crates/axo-ai/src/models/mod.rs:597-600,615-626`). This is not just migration of pre-upgrade files.

Host-owned search-service credentials are different: Axome's search auth still intentionally reads bare service ids directly (`web/auth.rs:126-129`), so these do not automatically become invalid because the axo model registry changed. No blanket rewrite of every auth.json key is implied by the source.

### Journals with inherited write ceilings

Base WriteRoot JSON was externally tagged, e.g. `{"path":"/work"}` or `{"url":"local://"}`; current `WriteRoot` is `{"type":"path","value":"/work"}` / `{"type":"url","value":"local://"}` (`axo:crates/axo/src/tools/writes.rs:76-102`). `WriteScope` remains an array of array layers. The child `SpawnLink.writes` field is persisted in the session header and decoded strictly through `Option<WriteScope>` (`axo:crates/axo/src/store/entry.rs:75-90,118-151`), with no old-root compatibility parser. Present empty outer `[]` ceilings also fail explicitly; `[[]]` remains a nonempty layer list.

Axome stores these journals in `dirs.data/axo/sessions` (`engine.rs:33,76-79`). **Source-traced upgrade consequence:** an old child header containing nonempty old-shaped write roots fails header decoding, fails direct reads/reopening, and is omitted from `JsonlStore::list` with a warning (`axo:crates/axo/src/store/jsonl.rs:144-165,345-379`). This is not evidence that all old root sessions fail: absent/null scopes and scope layers with no roots do not exercise the changed root representation. Axome's current session-list consumers accept upstream `SessionInfo` (`mod.rs:125,973,1794`); the skipped-file behavior occurs inside axo before they receive those records.

### Old tool records remain readable JSON but lose upstream derived state

Old journals retain `details.ledger` / `details.change`. Current axo `file_records`/`ledger_records` only read `details.files` (`axo:crates/axo/src/tools/ledger.rs:79-102`), and `FileLedger::restore` rebuilds freshness from that list (`:185-199`). Thus legacy accesses are not restored into the new ledger: a resumed session can require a fresh read before editing even though its old transcript contains a read. Old mutation records also disappear from upstream `file_changes` and hence its derived change views (`axo:crates/axo/src/tools/changes.rs:115-120`); Axome delegates squash operations to axo (`mod.rs:1409-1425`). Historical Axome inline rendering still understands the old top-level records; new records have the opposite renderer mismatch described above. The stored messages themselves are not erased by these extractors.

### Session listings, caches and side files

- Session listings are built from headers plus filesystem metadata, not a stored index requiring a `modified`→`modifiedAt` migration (`axo:crates/axo/src/store/jsonl.rs:331-379`). Sorting still uses nanosecond file metadata even though the exposed timestamp is milliseconds. The host compile break and invalid old write-scope headers are separate concerns.
- Axome supplies `dirs.cache/axo` to both registry and harness (`engine.rs:33-34,49-50,81`). Model cache filenames remain `models.dev.v2.json` and `codex-models.json` (`axo:crates/axo-ai/src/models/catalog.rs:15`; `models/codex.rs:31`). The Codex cache gains a defaulted `tiers` list (`models/codex.rs:17-29`), so old cached rows decode with no tiers rather than becoming corrupt. The models.dev parser adds multiplier metadata; its cache filename is unchanged (`models/catalog.rs:115-120`).
- Snapshot blobs retain the session-side `snapshots` directory and content-addressed filenames (`axo:crates/axo/src/tools/snapshots.rs:21,41,54`). Axome reads them through the upstream snapshot APIs (`mod.rs:1463-1466`). No snapshot filename migration was found; loss of legacy change extraction is distinct from physical blob availability.
- Journal paging checkpoints are bounded in-memory state (`axo:crates/axo/src/store/jsonl.rs:20-53,252-318`), not a new persisted sidecar requiring migration. This inventory found no model-cache or journal-location rename in the evaluated diff.

## Inventory points

1. **Breaking:** 20 production and 6 test-only emitted diagnostics cover guideline text, shared event/model snapshots, grouped definitions/catalog calls, owned modes, timestamps, new command/entry/error cases, and required tier fields; exact sites and upstream causes are in section 1.
2. **Breaking, not emitted by this probe:** exhaustive live event/entry and history matches additionally lack QueueChanged/ServiceTierChange (`live.rs:504-832`; `mod.rs:2093-2143`; axo `session/mod.rs:264-265`, `store/entry.rs:321-325`).
3. **Behavior change / silent wire break:** `details.files` replaces top-level ledger/change records, defeating existing read-path/footer and inline diff parsers (`payload.ts:81-100`, `readOutput.ts:28-44,95-106`; axo `tools/ledger.rs:65-102`).
4. **New capability:** multiple file accesses and failed/hard-stopped mutations now carry durable records; Axome forwards arbitrary details but has no corresponding list-based parser (`live.rs:693-709`; axo `tools/wire.rs:69-126,154-164`).
5. **Data migration / silent behavior break:** old auth keys are no longer registry slots; Axome's setter/remover also still write/remove the old names (`providers.rs:63-85`; axo `models/mod.rs:597-600,994`; `auth/resolve.rs:129`). Host-owned search-service keys remain a separate consumer (`web/auth.rs:126-129`).
6. **Data migration:** old nonempty WriteRoot encodings in child headers fail decoding and those sessions are skipped by listing (`engine.rs:76-79`; axo `tools/writes.rs:76-102`, `store/entry.rs:118-151`, `store/jsonl.rs:375`).
7. **Data migration / behavior change:** old tool details no longer restore upstream freshness or derived changes, although stored transcript JSON remains (`mod.rs:1409-1425`; axo `tools/ledger.rs:79-102,185-199`, `tools/changes.rs:115-120`).
8. **New capability / projection gap:** service-tier selection, per-model tiers, automatic thinking adjustments and structured completion details are not represented by current Axome chat projections (`convert.rs:533-743`; `mod.rs:146-180`; axo `store/entry.rs:312-325`, `error.rs:5-12`).
9. **No direct wire impact:** axo's `whenIdle` casing does not change Axome's independently serialized `when_idle` protocol; SessionInfo is likewise projected rather than forwarded raw (`convert.rs:89-91`; protocol `chat.rs:579-588`; `mod.rs:2157-2161`).
10. **Behavior change, compatible fields:** bash incomplete-output handling can remove the full-output link without changing the fields Axome parses (`bashOutput.ts:27-45`; axo `jobs/command.rs:754-777,799-801`).
11. **No cache filename migration found:** existing model caches and snapshot paths remain; Codex tier fields default for old caches, and journal checkpoints are memory-only (`engine.rs:33-34`; axo `models/codex.rs:17-31`, `models/catalog.rs:15`, `tools/snapshots.rs:21-54`, `store/jsonl.rs:20-53`).
