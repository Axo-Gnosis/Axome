# axo Turn Diff Snapshots & `write` Diff Metadata

> **Fulfilled in axo `4d6bc34443abf6a6a512098e120469efd8301e39`** (`feat(axo): record file changes with content-addressed snapshots`, 2026-11-16).
> `write` and `edit` now record `details.change: FileChange` alongside `details.ledger` via `SessionTools::record_change`, storing raw bytes under `<session>/snapshots/<sha256>` (capped at `MAX_SNAPSHOT_BYTES = 4 MiB`, read-only, hard-linked for paths referenced on fork). `Axo::{read_snapshot, squash_changes}` and `Session::{read_snapshot, squash_changes}` + `axo::{file_change, squash}` are exposed.
> **Breaking change:** `edit` drops top-level `details.diff`, `details.patch`, and `details.first_changed_line` (now inside `details.change.diff`: `{ additions, deletions, firstChangedLine, patch, patchTruncated }`) and rejects non-UTF-8 files.

- **Target repository:** `/repo/rust/axo`
- **Source discussion:** Chat UI/UX per-turn diff/review work (2026-11-16)
- **Related plan row:** [`../plan.md`](../plan.md#t10-remaining-axo-gaps) ([T10](../plan.md#t10-remaining-axo-gaps))

---

## Goal (Axome host UI)

Render a per-turn `"N files changed +A -D [Review]"` summary card under finished agent turns in chat, and have **Review** open Axome's existing Monaco `DiffStack` viewer (`oldContent` + `newContent` per file) fed **only** from the agent's transcript mutations — independent of `git status` and accurate for historical turns even after later edits.

## Current state in `axo`

- `crates/axo/src/tools/edit.rs` (`EditTool::execute`): reads `bytes`, computes `TextDiff`, and writes `details`:
  `{ diff, patch, first_changed_line, ledger }` (`CONTEXT_LINES = 2`).
- `crates/axo/src/tools/write.rs` (`WriteTool::execute`): already reads `current` when overwriting an existing file for `FileLedger::check_fresh`, then writes `content`, but discards `current` and only records `{ ledger }`.
- `crates/axo/src/session/side_files.rs`: already manages session side directories (`artifacts/`, `local/`) under `<session_dir>` with `copy_for_fork` hard-linking immutable read-only files.

## Why `details.patch` alone isn't enough for Review

1. `DiffStack` / Monaco `DiffEditor` takes complete `oldContent` and `newContent` strings per file (for full syntax highlighting, full-file line numbers, and expanding unchanged regions), not 2-line-context hunks.
2. Multiple `edit`/`write` calls to the same file within one turn cannot be reliably squashed from partial 2-line patches alone, whereas squashing full snapshots is `firstBefore` → `lastAfter` per file in the turn.
3. `WriteTool` currently gives Axome no way to distinguish a new file from an overwrite or compute deletions (`-D`).

## Proposed design to discuss in `axo`

1. **Content-addressed turn/mutation snapshots (under `<session_dir>/snapshots/<sha256>` or similar):**
   - When `EditTool` or `WriteTool` mutates a plain file, persist immutable blobs (keyed by `sha256` from `FileLedger` / raw bytes) for the pre-mutation content (or `null`/absent if newly created) and post-mutation content, subject to size/binary caps.
   - Store only lightweight refs in `ToolOutput.details` (avoiding `.jsonl` bloat), e.g. `{ beforeSha, afterSha }` (plus `created: bool` / `truncated` or `binary` flag if skipped).
   - Hard-link snapshots on session fork in `side_files::copy_for_fork` (same read-only pattern as `ARTIFACTS_DIR`), and clean up with session deletion if applicable.
2. **`WriteTool` diff/numstat parity:**
   - Have `WriteTool` compute `diff` / `patch` (or `additions` / `deletions`) against `current` when overwriting so both inline chat `ToolDiff` cards and collapsed turn summaries have exact counts without reading blobs eagerly.
3. **Host access (`axome-core`):**
   - Expose a clean method/helper on `axo` (session or store level) to read a snapshot blob by `(session, sha256)` so the host does not reach into internal sidecar paths directly.

## Open questions to settle in `axo`

- Exact `details` schema and whether `EditTool`/`WriteTool` should also emit numstat (`additions`/`deletions`) so the host does not need to fetch blobs just to render the `+A -D` summary badge.
- Blob storage format (raw vs compressed, max file size cap, binary handling) and retention across forks/compaction/deletion.
- Whether `local://` virtual writes or only plain workspace files should be snapshotted (and confirming `bash` mutations remain out of scope).
