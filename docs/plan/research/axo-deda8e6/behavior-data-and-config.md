# Behavior, persisted data, and configuration inventory

- axo: `deda8e6`; base: `5c3f418`.
- Axome HEAD: `48f5a03`.
- Date: 2026-11-22.
- Question: Which changes in this range affect Axome's existing behavior, persisted sessions, tool-result rendering, configuration, and inspectors without requiring it to opt into a new API?

This is a source inventory, not an adoption design. Paths beginning `crates/` refer to axo at the target revision; `src-crates/`, `src/`, and `README.md` refer to Axome HEAD. Source diffs were inspected; no builds, tests, or execution probes were run. Extension-manager internals and provider implementation are outside this report; their settings and persisted-data intersections are included here. Consequences deduced from incompatible producer/consumer shapes are marked `[INFERENCE]`.

## 1. Journal, store, and compatibility

### Paging is an additional read path, not a journal-format migration

`SessionStore::read_page(path, after, limit)` returns the header, entries with physical `seq > after`, and `more`; the default implementation still reads the complete session. Public paging normalization defaults to 256 entries and clamps to 1–1024 (`crates/axo/src/store/mod.rs:27-41,85-98`). The JSONL implementation reads the header, seeks to a cached offset when applicable, and reads through one additional valid entry to determine `more` (`crates/axo/src/store/jsonl.rs:252-318`).

The cache holds at most 32 **in-memory** checkpoints, keyed by device/inode, length, modification time and change time; it only saves a checkpoint when the file stamp is unchanged across the read. A changed/appended/replaced file does not reuse the old stamp's checkpoint. There is no checkpoint sidecar or new checkpoint entry (`crates/axo/src/store/jsonl.rs:20-52,264-315`). Physical blank/malformed lines still count toward sequence numbers, although they are not returned (`crates/axo/src/store/jsonl.rs:113-142`). Existing full `open_read`/`open_write` paths remain full-file reads (`crates/axo/src/store/jsonl.rs:196-249`).

Axome constructs this `JsonlStore` itself (`src-crates/axome-core/src/services/axo/engine.rs:78`). Its history index independently streams JSONL and filters message/submission/session-info lines (`src-crates/axome-core/src/services/axo/mod.rs:2076-2155`); that code does not gain paging merely by updating the dependency. No current Axome paging consumer was found.

### Session list timestamp changes type and field name

`SessionInfo.modified: SystemTime` becomes `modified_at: i64`, serialized as `modifiedAt` in Unix milliseconds (`crates/axo/src/store/entry.rs:754-766`). JSONL listing converts the file's timestamp to milliseconds with a checked integer conversion; sort order retains nanosecond precision through a separate key. Unreadable/invalid sessions are warned about and omitted (`crates/axo/src/store/jsonl.rs:345-379`). This is list metadata, not a renamed field in the JSONL header.

Axome directly uses `info.modified` to produce `HistoryRow.updated_at`; that source contract changes (`src-crates/axome-core/src/services/axo/mod.rs:2166-2170`).

### Old write ceilings can invalidate the header, rather than merely lose a transcript row

`WriteRoot` changes from externally tagged objects such as `{"path":"/repo/plans"}` / `{"url":"local://"}` to `{"type":"path","value":"/repo/plans"}` / `{"type":"url","value":"local://"}` (`crates/axo/src/tools/writes.rs:77-91`). `WriteScope` remains an array of layers. `SpawnLink.writes` in a child header deserializes that type; a present scope must have a layer (`crates/axo/src/store/entry.rs:119-152`). Source tests explicitly reject both old root encodings (`crates/axo/src/store/entry.rs:796-810`).

Consequences are precise:

- A child header with a nonempty old-encoded write root fails `Header` deserialization as `Error::InvalidSessionHeader`. Read, header-read, and writable-open all use this parser; writable-open parses before repairing/truncating any tail (`crates/axo/src/store/jsonl.rs:144-165,196-229,242-267,321-327`).
- Listing skips that session with a warning instead of failing the entire listing (`crates/axo/src/store/jsonl.rs:360-375`).
- Absent/null ceilings and scopes containing empty layers do not contain the changed root encoding. The compatibility issue is not every old session. There is no old-root reader in `WriteRoot`; format version remains **1** (`crates/axo/src/store/entry.rs:20-21`; `crates/axo/src/tools/writes.rs:77-103`).
- Entry-body failures are a different boundary: unknown tags or known tags with undecodable fields become `Entry::Unknown(Value)` and retain their raw JSON; genuinely malformed JSON lines are skipped with a warning. This fallback does not apply to headers (`crates/axo/src/store/entry.rs:286-296,564-568`; `crates/axo/src/store/jsonl.rs:131-156`).

Axome delegates session discovery/opening to axo, while its own history parser runs only after receiving `SessionInfo`; skipped child headers therefore never reach that parser (`src-crates/axome-core/src/services/axo/mod.rs:2076-2085`). There is no Axome write-ceiling migration in this path.

### Entry and projection changes

The new entry kind is `service_tier_change { serviceTier }`. `thinking_level_change` adds default-false `automatic` and optional `serviceTier`; a suffix-only combined thinking/tier selection can be recorded as one thinking-change entry. `ModeRestore` gains defaulted `serviceTier` (`crates/axo/src/store/entry.rs:275-284,310-328`; `crates/axo/src/session/mod.rs:1385-1419`). Old entries without these additions still deserialize.

Projection tracks the tier. Explicit model/thinking/tier changes clear the saved mode restoration, but an **automatic** registry-driven thinking clamp now preserves it (`crates/axo/src/store/project.rs:112-143`; clamp producer `crates/axo/src/session/mod.rs:1345-1381`). Tier selection is excluded from the transcript revision counter, like other selection-only records (`crates/axo/src/session/journal.rs:266-288`). No new paging, file-mutation, or ask-document entry kind was added: mutations remain tool-result details inside message entries.

Axome has exhaustive entry matches and its own model/thinking projection/rendering paths: `src-crates/axome-core/src/services/axo/live.rs:596-645`, `src-crates/axome-core/src/services/axo/convert.rs:345-373`, and `src-crates/axome-core/src/services/axo/mod.rs:2132-2152`. `[INFERENCE]` These require source-level handling of the new variant; the current model/thinking-only UI does not expose a tier selection. Provider/message tier payloads are covered by the provider survey.

## 2. Delivery, queues, selections, and modes

### A single ownership transition chooses the next work

Previously idle delivery and owner release used separate drain/claim paths. `drive_delivery(releasing)` now selects queued input and job/mail delivery while holding the same activity-state write lock. A releasing owner clears its Stop request there; an idle wake does nothing if another owner is running. If work is ready, ownership stays running through the chained run rather than exposing a temporary idle state (`crates/axo/src/session/delivery.rs:182-225`). Completion of input preparation and job-result notifications use this same path (`crates/axo/src/session/delivery.rs:111-119,710-715`).

Prompt admission only clears auto-pause if Stop has not intervened, under the activity lock (`crates/axo/src/session/delivery.rs:390-401`). This closes a Stop-versus-preparation race; it is not a new host pause policy.

Axome treats `Activity` idle as the admission/unload boundary (`src-crates/axome-core/src/services/axo/live.rs:662-675`). `[INFERENCE]` It can see fewer transient idle transitions and no longer depends on a separate wake recovering work stranded between drain and release. FIFO within queue kinds and steer-before-follow-up are not new product semantics.

### Durable submission and live queue are separate observations

A steer/follow-up is journaled, then enqueued under the live-state lock, then emits the new `SessionEvent::QueueChanged`, and only after releasing `queue_order` drives idle delivery (`crates/axo/src/session/delivery.rs:641-670`). Thus the submission `Entry` event is not itself proof that the live queue insertion has completed. On withdrawal append failure, the removed queued message is restored to its original queue position, a queue-change notification is emitted, and delivery is re-driven (`crates/axo/src/session/delivery.rs:477-513`). Live snapshot reconstruction uses actual queued IDs, preserving their order rather than deriving pending status from journal history alone (`crates/axo/src/session/mod.rs:551-570,3223`).

Events/attach receivers now carry `Arc<SessionEvent>`; `QueueChanged` is a unit variant meaning refresh the live queue snapshot (`crates/axo/src/session/mod.rs:249-265,474-482`). Axome currently constructs/removes queue rows from submission, withdrawal and delivered-message entries, not a queue-change event (`src-crates/axome-core/src/services/axo/live.rs:522-577`). `[INFERENCE]` Its exhaustive event handling and receiver ownership shape need to account for this new event/transport contract, independently of whether it changes its queue presentation.

### Selection and request-boundary ordering

A model switch publishes model, thinking and tier to the agent in one `set_selection` call after journal append, rather than separate thinking/model mutations (`crates/axo/src/session/mod.rs:1251-1265,1320-1338`). Combined suffix changes similarly append one record before changing both live values (`crates/axo/src/session/mod.rs:1385-1427`). Registry refresh clamps write `automatic: true`, so they no longer cancel mode restoration (`crates/axo/src/session/mod.rs:1370-1381`; `crates/axo/src/store/project.rs:125-137`).

Request-boundary `advance` is now async and takes `selection_order` across effective-mode installation, scope refresh, reservation of mode tools, and promotion changes (`crates/axo/src/session/surface.rs:155-218`; invocation `crates/axo/src/session/mod.rs:1644-1648`). This serializes the boundary against a concurrent selection change; `Mode` remains the selected mode and `EffectiveMode` remains the request-boundary-applied mode.

Mode selection holds that same ordering lock through journal write, live selection update, `Mode` event and pending-result supersession (`crates/axo/src/session/modes.rs:125-200`). Leaving via yield verifies the exact selected `Arc<ModeSpec>`, not just a reused name, under this lock (`crates/axo/src/session/modes.rs:130-146`). Navigation retains the already-selected definition when the destination still names it, and only restores a saved baseline if the live selection still matches the transition it prepared (`crates/axo/src/session/modes.rs:65-122`). These matter when definitions change live; they do not cause a selected mode to apply before the next request.

Axome consumes journal selection separately from mode-result events and uses `session.modes()` to describe review UI (`src-crates/axome-core/src/services/axo/live.rs:596-606,771-777`). Its comment already recognizes that journaling an agent change precedes swapping the live spec (`src-crates/axome-core/src/services/axo/live.rs:621-624`). No total ordering across every independent job/event channel is asserted here; the guarantees above are the specific source-enforced ordering changes.

## 3. File tools, mutation records, and review cards

### The result envelope is a breaking persisted-data change

Read/write/edit no longer emit top-level `details.ledger` and `details.change`. They emit `details.files: FileRecord[]`, each `{ ledger: {path, sha256, access}, change?: FileChange }`. Access is now required in `LedgerRecord`. The extractor returns all changes, including those from an error result; the old singular `file_change` function is replaced by `file_changes` (`crates/axo/src/tools/ledger.rs:59-109`; `crates/axo/src/tools/changes.rs:115-123`). Read constructs the new envelope too (`crates/axo/src/tools/read.rs:87-114`).

`FileChange`, `SnapshotBlob`, `LineDiff` and `SquashedChange` keep their field layouts. However `FileChange.key` is now base64url-no-padding of the canonical native path bytes rather than a lossy path string (`crates/axo/src/tools/mod.rs:269-276`). This separates distinct non-UTF-8 native filenames and still groups aliases of the same target. `[INFERENCE]` A resumed session mixing old and new records can carry different key encodings for the same file; grouping solely by `key` does not join those encodings.

Backward reading is **not** supplied for the old details envelope: `file_records` and `ledger_records` only inspect `details.files`. Ledger restore now considers every result, including error results, and later accesses win (`crates/axo/src/tools/ledger.rs:79-109,185-200`). Old messages still parse as messages because details are JSON, but their old access metadata is ignored by this restore. `[INFERENCE]` Reopening a pre-update journal may require fresh reads before guarded modifications even though that journal contains old successful reads.

Axome's affected consumers are direct:

- `payload.ts:81-100` reads exactly `details.change`; `ToolDiff.svelte:25` and `toolHeader.ts:139` use it (`src/lib/features/chat/elements/tool/`).
- `readOutput.ts:42` reads exactly `details.ledger` (`src/lib/features/chat/elements/tool/readOutput.ts`).
- `turnChanges.ts:37-48` only considers successful `edit`/`write` calls and one mutation per result (`src/lib/features/chat/turnChanges.ts`).
- The review route passes parsed changes to axo squashing and maps snapshot identities (`src-crates/axome-core/src/services/axo/mod.rs:1416-1450`); `TurnDiffView.svelte:39` renders the result through `DiffStack` (`src/lib/features/workbench/surfaces/diff/`).

`[INFERENCE]` With only compilation fixes, current new-format results would not populate these change cards, and failed-but-mutating calls would remain excluded even after recognizing the new envelope. DiffStack itself is downstream of this normalization, not a direct journal-format reader.

### Hard stops preserve completed mutation evidence

Native tool cancellation still requests cooperative cancellation, then hard-stops after the grace period. The loop now drops the execution future and runs a registered recovery future rather than losing all post-drop results (`crates/axo-agent/src/tool.rs:39-44,85-134`). Built-in edit/write register recovery before committing; commit runs as a separately settled operation, and recovery waits for admitted operations and returns an error `ToolOutput` with `details.files` if records exist (`crates/axo/src/tools/wire.rs:35-40,101-182`).

The filesystem write and change recording are not separated by a cancellation check after the mutation: commit checks cancellation before writing, performs the write and records the change under its retained file lock (`crates/axo/src/tools/writes.rs:433-452`). `finish` merges records into an output even when the caller subsequently errors; non-object custom details are retained under `value`, and the `files` member is owned by the recorder (`crates/axo/src/tools/wire.rs:69-99`). This is an in-process hard-stop guarantee, **not** a guarantee against OS/process death or disk failure after a mutation.

Edit's matching algorithm is not replaced: the diff moves I/O/freshness/scope/recording into shared code while preserving UTF-8 checks, BOM handling, LF normalization, replacement application and original line endings (`crates/axo/src/tools/edit.rs:93-122`; shared preparation `crates/axo/src/tools/writes.rs:323-431`). Write still permits identical-content replay without a prior read and requires freshness before overwriting changed existing content (`crates/axo/src/tools/writes.rs:369-390`). Much of the apparent edit rewrite is regression coverage and shared plumbing, not a new editing language.

### Reads are bounded selectively, not universally

`open_regular` opens nonblocking, rejects directories/non-regular files, and `read_limited` reads at most limit+1 bytes before deciding the limit was exceeded (`crates/axo/src/tools/files.rs:17-45`). File reads acquire the same canonical file lock as mutations and race cancellation even while waiting for it; only a successful conversion/read records freshness (`crates/axo/src/tools/writes.rs:228-307`). Errors include “is not a regular file”, “exceeds the read limit of … bytes”, virtual-resource byte-limit errors, and “Operation aborted”.

Crucially, built-in `read` passes **None** as its byte limit (`crates/axo/src/tools/read.rs:87-88`): its existing output paging/truncation is not a new bound on full-file allocation. The 32 MiB cap is on wire `readFile`/`readFileBytes`, which pass `Some(WIRE_READ_LIMIT)`; UTF-8 wire reads reject invalid UTF-8 before granting freshness (`crates/axo/src/tools/wire.rs:24-25,203-245`). No current Axome consumer of those wire methods.

Snapshot reads now also use the regular-file bounded reader with the existing 4 MiB snapshot ceiling, reject oversize blobs with `InvalidData`, and still verify their hash (`crates/axo/src/tools/snapshots.rs:16-17,115-155`). Snapshot storage checks non-symlink regular-file destinations and cleans up unsuccessful temporary writes; failures remain `Omitted::IoError` rather than pretending a snapshot exists (`crates/axo/src/tools/snapshots.rs:63-112`). Unified inline patches still cap at 64 KiB, but formatting now stops at that bound instead of allocating the whole patch before discarding it; counts/first line and `patchTruncated` semantics remain (`crates/axo/src/tools/changes.rs:22-23,178-206`). Axome's existing omitted/truncated snapshot and diff fields remain relevant (`src/lib/features/chat/elements/tool/payload.ts:35-78,103-110`; `src-crates/axome-core/src/services/axo/mod.rs:1443-1450`).

## 4. Commands, jobs, schemes, compaction, and prompt inspection

### Bash and command output

Bash execution is factored into shared `SessionTools::run_bash`; the first environment notice is tracked per session tools, rather than per built-in BashTool instance. Timeout validation, URL rewriting, dev-shell environment resolution and read-only sandbox preparation remain in the shared path (`crates/axo/src/tools/bash.rs:133-247`). No new bash argument/settings key was introduced by that factoring.

When output drain deadlines expire with pipes still open and no retained helpers, the job now reports `capture_failed` and an “output drain deadline expired” capture error. A truncated result only advertises its full-output artifact when capture succeeded **and** no pipes remain open. Final progress marks capture failure even without a stream index (`crates/axo/src/jobs/command.rs:754-776,948-971`). This prevents a partial log from being presented as complete; it does not remove retained/promoted journal log references.

Call-ID output lookup now chooses the newest matching command; background promotion visits **all** foreground commands for that call rather than stopping at the first. Output lookup can also target a job, and `CommandOutput` exposes its raw path (`crates/axo/src/jobs/mod.rs:1574-1682`; `crates/axo/src/jobs/command.rs:216-220`). These primarily matter for a host/extension tool running multiple commands in one invocation. Axome already uses call-ID output and exposes capture failure (`src-crates/axome-core/src/services/axo/live.rs:1093,1120-1188`), and delegates background promotion (`src-crates/axome-core/src/services/axo/mod.rs:1249-1255`).

`ProcessSupervisor::output_stderr_tail` drains stdout without retaining it and retains only the requested stderr tail; ordinary `output` still retains both complete streams (`crates/axo/src/exec/host.rs:210-255,510-541`). `Container::memory_batch` reports PSS from `/proc` for requested containers (`crates/axo/src/exec/host.rs:789-855`). No current Axome consumer of these new helpers; they do not change ordinary bash output limits or establish a new user setting.

### Ask is not ask documents

The `ask.rs` delta adds wire derives, restricts internal helper visibility, and wraps question events in `Arc`. Question/option/answer fields and the pending/resolved lifecycle are unchanged (`crates/axo/src/tools/ask.rs:22-60,70-164`). There is no ask-document flow in this implementation delta. Axome's question list/update handling continues to consume the same question payloads (`src-crates/axome-core/src/services/axo/live.rs:718-730`), subject to the event transport change.

### Schemes and artifacts

Artifact allocation is now fallible and reserves suffix `streams` / `*.streams` for command-output index files (`crates/axo/src/schemes/artifact.rs:64-80`). This is an allocator contract, not a rename of existing artifacts. Axome's ordinary transcript artifact links use its session scheme-resolution path (`src-crates/axome-core/src/services/axo/mod.rs:1390-1399`); no current Axome direct artifact-allocation consumer was found.

Scheme clones now share a live host-handler set. Replacing it builds/validates a complete list before swapping, and a virtual read retains the handler that resolved it rather than looking it up again mid-operation (`crates/axo/src/schemes/mod.rs:125-203,310-339`; `crates/axo/src/tools/writes.rs:233-246`). `skill://` resolves against live definitions and continues longest-name matching (`crates/axo/src/schemes/skill.rs:12-66`). Bundled docs add `extension-api.md` (`crates/axo/src/schemes/dir.rs:26-29`). These do not automatically install extension handlers in Axome; host registration remains required.

### Compaction consumes the new records; prompt layout is not wholesale replaced

Compaction read/modified file lists and skill-read protection now iterate all ledger records in `details.files`, including records on error results (`crates/axo/src/compaction/mod.rs:203-224`; `crates/axo/src/compaction/elide.rs:77-90`). `[INFERENCE]` Old top-level ledger details no longer contribute to those derived file lists or skill-read protection. The diff does not introduce a new compaction threshold/summary setting. Existing summary retry events now use `Arc` (`crates/axo/src/compaction/summary.rs:439-448`). Tier treatment of summary requests is in the provider survey.

Prompt sections keep their section-map representation and existing inspector separation between `latest_built` and `recorded` (`crates/axo/src/session/prompt.rs:604-652`). Scheme docs move from immutable sources into mutable prompt shape; skill listings read the live definitions. Existing section-building order/wrappers are retained rather than replaced (`crates/axo/src/context/prompt.rs:73-82,123-165`). Live changes can therefore change the `schemes`, skill and tool content of subsequent builds; no new mandatory top-level system section is introduced by this factoring.

Tool shaping now records host origins/diagnostics, and duplicate deferred tools use later-registration replacement rather than keeping the first. Direct-vs-deferred collisions still prefer direct tools, and reserved `call`/mode `yield` handling remains explicit (`crates/axo/src/session/prompt.rs:150-222,269-286`). Axome's Context endpoint still chooses the recorded prompt first and only falls back to the latest build when none is recorded; startup/nested inventories remain separate (`src-crates/axome-core/src/services/axo/mod.rs:1274-1305`). The Context panel loads that endpoint (`src/lib/features/chat/sidebar/ContextPanel.svelte:40-60`). `[INFERENCE]` A staged live prompt change need not immediately appear there because that endpoint intentionally prefers the recorded version.

## 5. Configuration and discovery

### Complete settings-key delta in `config.rs`

No old settings key is removed or renamed. Added keys and their defaults are:

| Key | Default / accepted values | Layer rule | Source |
| --- | --- | --- | --- |
| `serviceTier.onModelChange` | `reset`; also `keep` | User-only | `crates/axo/src/config.rs:196-211,670-679` |
| `subagents.serviceTier` | `standard`; also `inherit` | Whole `subagents` block remains user-only | `crates/axo/src/config.rs:179-192,232-240,670-679` |
| `extensions` | `[]`; ordered objects `{id, group?}` | User-only | `crates/axo/src/config.rs:47-52,73-85,670-679` |

`extensions[].id` permits one lower-case ASCII name or `publisher/name`; each part follows `[a-z0-9][a-z0-9._-]*`. Optional `group` is one such name. Invalid ID/group or duplicate ID rejects deserialization; the list is not silently deduplicated (`crates/axo/src/config.rs:88-163`). Core parses this list but does not itself discover/load a Bun package from it. Package roots, executable, installation and runtime locations are not settings keys in core.

Project validation runs before stripping user-only keys, so an invalid project `extensions` value can error even though a valid one would be ignored. `serviceTier` and `extensions` join `project`, `homeAgentsDir`, `modelRoles`, `subagents` in that stripping list (`crates/axo/src/config.rs:657-679`; project validation/filtering in `crates/axo/src/config.rs:700-728`). Existing defaults for queues, context, ask, bash, retry and compaction are unchanged in this range.

Existing model selectors gain one thinking suffix and one tier suffix in either order, including suffix-only and role selections; exact model IDs are still tried first. This changes accepted **values**, not names of `defaultModel`, `modelRoles`, or definition `model` fields (`crates/axo/src/config.rs:285-382`). Model-config addition is `serviceTiers` under model definitions/overrides: optional map of tier to optional multiplier, `null` removes, positive finite values set, and standard cannot be overridden (`crates/axo-ai/src/models/config.rs:77-94,159-178`). There is no newly removed/renamed models-config key in that source delta; broader provider registry/credentials are covered separately.

### Axome is not a transparent settings pass-through

Axome reads `~/.config/axome/settings.jsonc` and the folder `.axome/settings.jsonc`, validates its own layer schema, extracts only the nested `axo` object, and supplies `ConfigLayer`/`UserConfig`. It also supplies skill/agent/mode/keyword roots, prompt overrides and user AGENTS.md (`src-crates/axome-core/src/services/axo/config.rs:22-85`). axo does not find these files itself. `ConfigSource` sampling documentation is now precise: session create/open can fail at store work before sampling user configuration, whereas effective resolution samples once before any project callback (`crates/axo/src/config.rs:623-635`).

Axome's `AxoSettings` and subagent settings deny unknown fields and lack the new keys (`src-crates/axome-protocol/src/axo_settings.rs:10-31,216-238`). The runtime/editor JSON schema is generated from its own `EffectiveSettings`; folder restrictions have their own explicit global-only pointer list (`src-crates/axome-core/src/services/settings_resolution.rs:373-410`). `[INFERENCE]` Adding new axo settings to a user file is not sufficient to make them flow through this adapter; the current Axome schema does not recognize them.

The existing Settings Models and Tools views expose thinking/role and subagent controls, respectively, but not these new settings (`src/lib/features/settings/views/ModelsView.svelte:75-78`; `src/lib/features/settings/views/ToolsView.svelte:174-195`). README's settings contract enumerates the previous user-only keys and existing Agent pages (`README.md:127`). This is a host schema/UI/docs inventory point, not a claim that axo supplies corresponding UI.

### Discovery remains host-rooted; files and code now merge separately

Files are loaded once into `FileDefinitions`; later live registration merges do not reread them. Agent/mode/keyword/skill file definitions precede code definitions, with first-name-wins collisions; teams are validated after file/code merge (`crates/axo/src/context/definitions.rs:23-32,63-129`; `crates/axo/src/context/agents.rs:273-336`). Skills retain project `.agents/skills`, explicit roots, then optional home roots discovery order and the same required-description/name-warning behavior (`crates/axo/src/context/skills.rs:32-49,51-101,171-194`). Agent `contextFiles: true` can deserialize as retaining inherited context-file settings (`crates/axo/src/context/agents.rs:49-60,115-125`).

The host supplies roots, programmatic definitions and any later registration updates; core owns loading/merging/diagnostics. Axome already supplies roots and bundled definitions, so this touches its config adapter (`src-crates/axome-core/src/services/axo/config.rs:25-32`). The source-breaking `SessionOptions::definitions` API and live-definition controls are inventoried in the session API survey; they are not new settings JSON keys. No new implicit agent/mode/keyword filesystem search root was added in this delta.

## 6. Error surfaces

Compared with the base, `axo::Error` adds `SkillNotFound(String)` and `Completion { message, details: Option<Box<CompletionDetails>> }`. Completion details hold optional provider failure, usage, and stop reason; display remains “Completion failed: …” (`crates/axo/src/error.rs:5-13,42-51`). `ErrorCode` supplies a snake_case machine-readable category for each error, including these additions (`crates/axo/src/error.rs:113-190`). The final commit's tuple-to-struct breaking note concerns an intermediate completion API introduced during this range; the base-to-target inventory sees a newly available completion variant.

Axome currently exhaustively maps axo errors to `ApiError` categories and predominantly formats their display text (`src-crates/axome-core/src/services/axo/mod.rs:147-180`). `[INFERENCE]` New variants require match coverage; simply displaying `to_string()` would not preserve the optional billed usage/failure/stop details. There is no existing Axome completion-details presentation in that mapping. File-tool failures remain tool result text/details rather than new `axo::Error` enum variants, and invalid old header scopes surface through the existing `InvalidSessionHeader` variant.

## Inventory points

1. **Breaking:** `SessionInfo.modified` becomes millisecond `modified_at`/`modifiedAt`; Axome history timestamp conversion directly consumes the old member (`crates/axo/src/store/entry.rs:754-766`; `src-crates/axome-core/src/services/axo/mod.rs:2169-2170`).
2. **Data migration:** old nonempty `SpawnLink.writes` root encodings invalidate child headers on open/read and cause listing omission; no old-root fallback/version bump (`crates/axo/src/tools/writes.rs:77-103`; `crates/axo/src/store/jsonl.rs:144-156,375`; Axome history consumer `src-crates/axome-core/src/services/axo/mod.rs:2076`).
3. **New capability:** bounded-entry journal pages and 32 in-memory paging checkpoints do not change existing full-read behavior; no current Axome paging consumer (`crates/axo/src/store/mod.rs:27-41,85-98`; `crates/axo/src/store/jsonl.rs:252-318`; `src-crates/axome-core/src/services/axo/engine.rs:78`).
4. **Breaking / behavior change:** tier entry plus automatic/thinking metadata expand journal matches; automatic clamps preserve mode restoration (`crates/axo/src/store/entry.rs:310-328`; `crates/axo/src/store/project.rs:125-142`; `src-crates/axome-core/src/services/axo/live.rs:596-645`).
5. **Behavior change:** unified ownership-lock delivery avoids stranded queued work/transient idle; Axome uses idle to unload/admit work (`crates/axo/src/session/delivery.rs:182-225`; `src-crates/axome-core/src/services/axo/live.rs:662-675`).
6. **Breaking / behavior change:** `Arc<SessionEvent>` and `QueueChanged` distinguish durable submission from live queue insertion/restoration; Axome currently reconstructs queues from entries (`crates/axo/src/session/mod.rs:249-265,474-482`; `crates/axo/src/session/delivery.rs:477-513,641-670`; `src-crates/axome-core/src/services/axo/live.rs:522-577`).
7. **Behavior change:** atomic live selection and serialized mode/request-boundary transitions protect restoration and pending review identity; Axome consumes selected/effective mode and review state (`crates/axo/src/session/mod.rs:1320-1338`; `crates/axo/src/session/modes.rs:130-200`; `crates/axo/src/session/surface.rs:155-218`; `src-crates/axome-core/src/services/axo/live.rs:771-777`).
8. **Breaking / data migration:** tool details move to `files[]`, singular extraction disappears, and old ledger metadata is not restored; Axome read/diff parsers still expect old fields (`crates/axo/src/tools/ledger.rs:59-109,185-200`; `src/lib/features/chat/elements/tool/payload.ts:81-100`; `src/lib/features/chat/elements/tool/readOutput.ts:42`).
9. **Behavior change:** completed mutations survive hard-stop recovery as error outputs; Axome's successful-edit/write-only turn aggregation excludes them (`crates/axo/src/tools/wire.rs:101-182`; `src/lib/features/chat/turnChanges.ts:37-48`).
10. **Data migration / behavior change:** canonical file keys become base64 native bytes; mixed old/new key encodings can split per-file aggregation (`crates/axo/src/tools/mod.rs:269-276`; `src/lib/features/chat/turnChanges.ts:50-83`).
11. **Behavior change / new capability:** regular-file checks and cancellable locked reads affect built-in read; 32 MiB bounds apply to wire reads, not built-in full-file allocation (`crates/axo/src/tools/files.rs:17-45`; `crates/axo/src/tools/read.rs:87-88`; `crates/axo/src/tools/wire.rs:24-25,203-245`). Axome renders resulting read text/details (`src/lib/features/chat/elements/tool/readOutput.ts:42`).
12. **Behavior change:** bounded snapshot reads/storage failure handling and bounded inline patch generation retain existing omitted/truncated layouts used by Axome review (`crates/axo/src/tools/snapshots.rs:63-155`; `crates/axo/src/tools/changes.rs:178-206`; `src/lib/features/chat/elements/tool/payload.ts:35-78`).
13. **Behavior change:** partial command captures are explicitly failed and not advertised as complete artifacts; newest call log/all-command promotion semantics affect Axome's existing output/background APIs (`crates/axo/src/jobs/command.rs:754-776,948-971`; `crates/axo/src/jobs/mod.rs:1574-1682`; `src-crates/axome-core/src/services/axo/live.rs:1093-1188`; `src-crates/axome-core/src/services/axo/mod.rs:1249-1255`).
14. **New capability / no current impact:** stderr-tail capture, PSS batch measurement and reserved artifact suffix validation have no current direct Axome consumers (`crates/axo/src/exec/host.rs:210-255,789-855`; `crates/axo/src/schemes/artifact.rs:64-80`).
15. **No payload change:** ask documents are not implemented by this delta; existing question/answer fields remain, with only event ownership/wire derives changed (`crates/axo/src/tools/ask.rs:22-60,70-164`; `src-crates/axome-core/src/services/axo/live.rs:718-730`).
16. **Behavior change:** compaction's file inventory/protection now reads multi-record details, not legacy ledgers; existing Axome sessions retain old JSON details but do not recover that derived metadata (`crates/axo/src/compaction/mod.rs:203-224`; `crates/axo/src/compaction/elide.rs:77-90`; `crates/axo/src/tools/ledger.rs:79-109`).
17. **New capability / behavior change:** live scheme/definition/tool content can change prompt builds without replacing section layout; duplicate deferred tools now use later-registration precedence; Axome Context still favors recorded sections (`crates/axo/src/context/prompt.rs:123-165`; `crates/axo/src/session/prompt.rs:150-222,604-652`; `src-crates/axome-core/src/services/axo/mod.rs:1274-1305`).
18. **New capability / host schema impact:** user-only `extensions`, `serviceTier.onModelChange`, `subagents.serviceTier` and model `serviceTiers` are added, with no settings-key removals; Axome's strict schema, folder scope list, Settings views and README describe the old surface (`crates/axo/src/config.rs:47-85,179-240,670-679`; `crates/axo-ai/src/models/config.rs:77-94,159-178`; `src-crates/axome-protocol/src/axo_settings.rs:10-31,216-238`; `src-crates/axome-core/src/services/settings_resolution.rs:373-410`; `README.md:127`).
19. **Behavior change / new capability:** file definitions remain startup snapshots but merge separately with live code definitions; discovery roots remain host-supplied and unchanged (`crates/axo/src/context/definitions.rs:23-32,63-129`; `src-crates/axome-core/src/services/axo/config.rs:25-32`).
20. **Breaking / new capability:** completion and missing-skill errors plus machine-readable codes expand host error handling; Axome's current mapper has neither variant nor a completion-details carrier (`crates/axo/src/error.rs:5-13,42-51,113-190`; `src-crates/axome-core/src/services/axo/mod.rs:147-180`).
