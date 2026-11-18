# axo Chat: Turn Activity & Ask Panel Redesign

- **Status:** Designed, not started.
- **Target repository:** `/repo/rust/Axome` (frontend only; no `axo` changes required)
- **Design canvas (Toph, private):** https://claude.ai/code/artifact/3808c4c7-631d-4fdc-9069-528823a9e986
- **Source discussion:** Chat transcript legibility after `resume` turns with agents and background jobs (2026-11-17)

This file is self-contained: everything an implementer needs from the canvas is written out below. Board names in brackets (e.g. **[TurnOpen]**) point at the matching canvas board for anyone who can open it.

---

## Problem

A turn with agents and background jobs renders as a flat, same-weight list of lines (`2 tool calls · 3 thoughts`, `Queued follow-up for agent_4`, another fold, `Waited on agent_4`, `Wake agent_4/2 agent_4/3`, NOTE/RESULT cards, `Backgrounded bg_25`, `Wake bg_25 … Exited (0)`, two yellow abort lines). Nothing groups, so the answer and the work blend together.

Root causes in the current code:

1. **`segmentSteps` (`elements/transcript/groupSteps.ts`) only folds `tool | toolGroup | thinking`.** Receipts (`spawn`, `send`, `wait`, `kill`, `yield`, `enter_mode`, backgrounded `bash`) and `jobWake` notices are not folded, so each one breaks the work block into fragments.
2. **One job's lifecycle is spread over unrelated rows.** Spawn, follow-up, wait, wake deliverables (note/result/exit) are separate `ToolReceipt` / `NoticeBlock` / `ChildDeliverableCard` rows, though every one of them carries the job id.
3. **`ToolFrame` draws every call as a filled `bg-surface` rectangle**, so ten calls read as ten boxes of equal weight.
4. **End-of-turn notices duplicate.** `AgentBlock` emits both the assistant `stopReason === 'aborted'` row and the `runEnd.outcome === 'aborted'` row (deduped only on identical text), both in `text-warning`.
5. **A pending `ask` renders inline in the transcript (`ToolBlock` → `AskCard`)**, where it is easy to miss though it blocks the agent.

## Data available (verified in `src/lib/types/backend.ts`, `turns.ts`, `tool/receipt.ts`)

- Steps keep transcript order; `Step.entryId` → `ChatEntry.timestamp` gives timing (steps don't carry it yet).
- `toolResult.durationMs`; `bash` details carry `durationMs`, `exit`, `job`.
- Receipts carry job refs: `spawnReceipt(details).job | team`, `sendReceipt(details) { to, kind: note|steer|followUp|mail }`, `waitReceipt(details) { reason, jobs[] }`, `killReceipt(details) { id, stopping }`.
- `ChatNotice.jobWake { jobs[], deliverables[] }`; `ChatDeliverable.child { jobId, agent, team, note, partial, isError, text, value }` and `.command { jobId, command, exit, isError }`. Refs like `agent_4/2` are the Nth delivery; strip `/\d+$` (as `JobRef` does).
- `live.jobs: ChatJob[]` gives status (`queued|running|helpersRunning|finished|failed|interrupted`), `startedAt`, and for commands `endedAt` and `exit`; children have `progress { turns, maxTurns, tools[] }`. Use `jobTone()` / `pickJob()` from `features/chat/jobs.ts`.
- Thinking has **no** duration. Show the thought's first line, as `ThinkingBlock` already does.
- `live.questions: ChatQuestions[]`; each `ChatQuestion { id, question, header, options[{ label, description, preview }], multi, recommended }`; answers are `ChatAnswer { id, selected[], custom }`. `ask` details `outcome` can be `steered`.

---

## Target design

### 1. Turn structure

- A turn is: user bubble → one or more **work blocks** separated by the assistant's visible messages → end line (if not `done`) → changes card → footer. Unchanged: the changes card and footer.
- **Only a visible assistant message ends a work block** (as today). Receipts and job notices now fold into the work block too. A pending question stays visible even when its block is folded (keep the existing `asking` rule; with section 5 it shows as a single waiting row).
- **Fold behavior:** a work block is open while it is running. When a message starts streaming, the block before it folds to its summary line, the message streams below it, and the `WorkingIndicator` + Stop control sit under the message. When tools start again, a new block opens. When the turn ends, every block folds. **[MidA → MidB → MidC → Turn]**
- **Work block header (folded):** chevron, `Worked 1m 12s` in `text-fg`, counts `10 tools · 2 thoughts` in `text-muted`, then one chip per job touched in the block: `StatusDot size="sm"` + mono `text-xs` id (`agent_7`, `bg_31 exit 1`, `bg_30 stopped`). Per-block duration = first to last entry timestamp in the block; while running, keep the existing `WorkingIndicator` header + Stop.

### 2. The rail (replaces `ToolFrame` boxes) **[Steps, TurnOpen]**

- The open work block is a vertical rail: a 1px `border-edge` line, 7px from the block's left edge, with content padded 24px.
- **Every step is one row**, min-height 24px (`row-height-compact`), `gap-2`, `text-sm text-muted`, no background, no hover transition (it's a swept list):
  - **Node** on the rail: 15×16 box, `bg-ground` (so the line breaks behind it), 12px Hugeicon in `text-muted`; `text-accent` while running, `text-danger` on failure. Reuse the icon map in `ToolFrame` (`TerminalIcon`, `FileCodeIcon`, `FileDiffIcon`, `FilePlusCornerIcon`, `Search01Icon`, `FolderOpenIcon`, `MessageSquareIcon`, `Globe02Icon`, `Link01Icon`, `BookOpen01Icon`, `PackageIcon`).
  - **Verb** in `text-fg` (`text-accent` running, `text-danger-bright` failed), **target** in mono `text-muted` truncated, **aside** pushed right in mono `text-xs text-muted` (duration, counts, `NetLineChange`, exit, job id, actions such as **Background**).
- Rows per tool (verb · target · aside):

| tool | row | opened |
| --- | --- | --- |
| `read` | `Read` · `path:start-end` | code excerpt (existing `ReadOutput`, 40-line preview) |
| `edit` | `Edited` · `path:line` · `+a −d` | diff (existing `ToolDiff`) |
| `write` | `Wrote` · `path` · `N lines · 0.1s` | content / diff (existing `ToolDiff`) |
| `bash` | `Ran` · first line of command · duration | `$ command` + copy, output tail (existing `BashCommand` + `BashOutput`, 12 lines, `… N earlier lines`) |
| `bash` running | `Running` · command · elapsed + **Background** | `Running · No output yet` / live tail |
| `bash` failed | node/verb danger · `exit N` in danger | box with `border-danger-border`, output in `text-danger-bright` |
| `grep` | `Searched` · `"pattern" in path` · `N matches · 0.2s` | result lines with clickable `file:line` (existing `SearchOutput`) |
| `find` | `Found` · `"glob" in path` · `N files` | existing `SearchOutput` |
| `search_docs` | `Searched docs` · `"query" in source` · `N sections` | crumb + `path:start-end` list (existing `DocsSearchOutput`) |
| `web_search` | `Searched the web` · `"query"` · `N results · provider` | existing `WebSearchOutput` |
| `web_fetch` | `Fetched` · url display · `saved · 0.8s` | existing `WebFetchOutput` |
| `ask` (closed) | `Asked` · question · chosen answer / `dismissed` / `answered by your message` | question + options with the chosen one tinted `bg-accent-bg`, `Answered by you` |
| unknown tool | tool name in mono · compact JSON args · duration | `ToolFields` grid + `ToolOutput` |
| thinking | pip node (4px dot) · first line in mono `text-xs italic text-muted` | full thought, same style |

- **Opened output** sits under the row in a box: `rounded-md border border-edge bg-surface font-mono text-xs`; command part on surface, output part `border-t border-edge bg-ground`. Only the output is boxed, never the row. Keep the existing `collapsed | preview | full` views and the "More" button semantics.
- **Same tool in a row (`ToolGroup`, "nested calls"):** one row `Read 3 files` · `a, b, c` (or `Edited 3 files` · `+57 −2` total). Opened: an indented sub-list (`ml-1 pl-3.5 border-l border-edge`), one row per call with no node and the target in `text-fg`; each sub-row opens its own output inside the sub-list. A failed nested call shows `Failed` in danger on its sub-row.

### 3. Job cards and job rows **[Expanded, TurnOpen, Blocks]**

New `groupSteps` output: a `job` step per job id per work block, positioned at the job's first event in that block, that absorbs every receipt and `jobWake` deliverable for that id in the block.

- **Agent (child job) → card** on the rail (node = agent icon, aligned to the card header): `rounded-lg border border-edge bg-surface`.
  - Header (32px, `px-2.5`): `StatusDot` (tone from `jobTone`), mono `text-bright` id, task title (from the `spawn` args `task`, truncated, `text-muted`), state word in its tone (`Running` / `Done` / `Failed` / `Stopped`) + mono duration, chevron.
  - Body (`border-t`, `px-3 py-2.5`, rows `grid-cols-[64px_1fr]`, kind label mono `text-xs text-muted`): `spawned` (task; or `In the previous reply` / `Above, before the message` when spawned in an earlier block or turn), `follow-up` / `steer` / `mail` (from `send`), `waited` + duration (single-job `wait`), `note` (non-final child deliverable), `now` + spinner (live, from `progress`).
  - Result section (`border-t`): `section-label` "Result" + text in `text-bright`. Errors use `border-danger-border` on the card and `text-danger-bright` text.
  - Footer (28px, `border-t`): `N of M turns · read, edit, bash` from `progress`, **View Agent** (`onOpenJob(id)`).
  - Folded card: header + one line (live status or result summary).
  - A job spanning a message or a turn appears once per block; earlier cards say it continues below.
- **Command job → row:** `Ran in background` · command · `StatusDot` + `exit 0 · 38s · bg_25`. Opened: full command + copy, then `Output is kept with the job.` + **Open in Jobs** (output is not in the transcript).
- `kill` → row `Stopped` · `bg_30 · npm run dev` · `stopped`, merged into that job when it is in the block.
- A `wait` on several jobs stays its own row: `Waited for agent_7 and bg_31` · duration.

### 4. End of turn

- One line, not two: dedupe `stopReason` and `runEnd` by meaning, not text.
- `aborted`: 8px rounded square in `bg-muted` + `Stopped.` (`text-sm text-muted`, not warning). Append `agent_4 is still running.` when `live.jobs` has active jobs from this turn.
- `failed`: danger `Alert` with `runEnd.reason` or the assistant `error`.
- `length` / `interrupted`: keep current copy, muted.

### 5. Ask panel (in-flight) **[AskSingle, AskMulti, AskPreview, AskText, AskSteps, AskMin]**

While `live.questions` is non-empty, the question is answered in a **palette docked above the composer**, not in the transcript. The transcript shows only `Waiting for your answer` · question · **Answer below** (accent row on the rail).

- **Placement:** in `ChatSurface`, between `QueuedMessages` and `PromptInput` (`mb-2`), or absolutely positioned `bottom-full` inside `PromptInputRoot` like `PromptSuggestions`. Build it from `paletteSurfaceVariants`, `commandItemVariants`, `PaletteFooter`, `Kbd`, `Badge`, `Checkbox`, `Input`, `Textarea`, `Tabs`. Move `AskCard`'s keyboard handling into it; delete the `AskCard` branch from `ToolBlock`.
- **Header (36px, `border-b`):** `StatusDot tone="waiting"`, `axo needs an answer` (or `axo has 3 questions`) in `text-bright font-medium`, `· asked by axo` mono muted; right side: question tabs (multi-question calls) and a minimize `IconButton`.
- **Body (`p-3`, `gap-2.5`):** `header` in mono `text-xs text-muted` (`Layout`, or `Checks · pick any` for multi, `Keys · 2 of 3` in steps); question in `text-md font-semibold text-bright`; options as items with a 20px `Kbd`-style number, label `text-base`, description `text-sm text-muted` truncated on the same line, `Badge variant="accent"` "Recommended" on `recommended`. Highlighted item = `commandItemVariants` selected style (accent/15, `text-bright`, key in accent).
- **Footer:** `PaletteFooter` hints, right side `Esc dismiss`.
- **Variants:**
  - **Single** (`multi: false`): `↑↓ navigate · 1–N pick · ↵ answer`. Last item is always **Other**; choosing it turns the row into an inline `Input` (maps to `custom`).
  - **Multi** (`multi: true`): `Checkbox` per item, `Space toggle`, `N selected` + accent **Answer** button.
  - **Previews** (any option has `preview`): two columns (`1fr / 1.15fr`); right pane `bg-surface border-l` shows the highlighted option's preview in mono and follows arrow keys.
  - **Free text** (`options` empty): labelled `Textarea`, `↵ answer · Shift+↵ new line`.
  - **Several questions** (one `ask` call, `questions.length > 1`): tabs in the header with a check on answered ones, `←→ question · ↵ next`, answered questions collapse to a line (`Layout  Overlay the game  Change`). Submit once, with all answers.
- **Minimize:** collapses to a 36px bar above the composer: waiting dot, `axo is waiting on your answer`, truncated question, shortcut, accent **Answer**. The session tab/sidebar `StatusDot` shows `waiting` while a question is open.
- **Typing in the composer while a question is open steers instead:** show the composer's existing context strip (`context.title` = `Steer`, text `Sending this answers in your own words and closes the question`). axo already resolves the ask with `outcome: "steered"`.
- **History:** after resolution the `ask` row uses the closed form from the table above. **[AskHistory]**

---

## Implementation order

Each phase ships on its own. Keep Svelte 5 runes and the `.omp/AGENTS.md` rules: UI primitives from `$lib/components/ui/*`, Tailwind tokens only, no hover transition on swept rows, extract a primitive before markup repeats a third time.

1. **Data.** In `turns.ts` carry `timestamp` on steps (from `entryId`). In `groupSteps.ts`: fold receipts + `jobWake` notices into work segments; add the `job` step (merge spawn/send/wait/kill/backgrounded bash/deliverables by stripped ref, per segment); dedupe end-of-turn notices. Extend `groupSteps.test.ts` / `turns.test.ts` with a turn shaped like: tools → `spawn agent_7` → text → `send followUp agent_7` → `wait [agent_7]` → `jobWake agent_7/2 (note)` → `jobWake agent_7/3 (result)` → backgrounded `bash` + `jobWake bg_31 (exit 1)` → `kill bg_30` → text.
   *Done when:* that turn yields exactly two work segments with one `agent_7` job step in each, no stray receipt/notice segments, and one end notice.
2. **Rail.** Rewrite `ToolFrame` as a row, `WorkBlock` as the rail and folded summary (duration + job chips), `ToolGroup` as the nested sub-list, `ThinkingBlock` with the pip node.
   *Done when:* no tool row has a background; every tool in the table renders its row and opened output.
3. **Job cards.** New `JobCard.svelte` (child) and `JobRow.svelte` (command) replacing transcript use of `ToolReceipt` (job receipts), `NoticeBlock` (`jobWake`), `Deliverables`, `ChildDeliverableCard`. Keep `ToolReceipt` for `yield` / `enter_mode`.
4. **Ask panel.** New `AskPanel.svelte` (+ minimized bar) in `ChatSurface`; waiting row and closed row in the transcript; composer steer context.
5. **Docs.** Update `README.md` for the user-visible chat changes; record the settled decisions in `docs/plan/design.md`.

**Validation:** `bun run check`, `bun run build`, the existing `*.test.ts` suites touched in phase 1, then a manual pass in `bun run app:dev` with a session that spawns an agent, backgrounds a command, sends a message mid-turn, and asks a question.

## Open questions

- Should `mail` notices fold into the work block (row `Mail from agent_2`, opens to the text) or stay visible like a message? Not drawn yet.
- `yield` / plan review, `enter_mode`, `turnBudget` / `yieldNudge` / `host` notices, compaction and persona/mode change rows are not redesigned yet. Render them as plain rail rows (info or warning icon) until they are.
- Free-text questions assume axo can send a question with an empty `options` list; confirm in `crates/axo/src/tools/ask.rs`.
- Two concurrent `ask` calls (`live.questions.length > 1`): queue them in the panel one after another.
- The minimized-panel shortcut (`Alt+A` in the design) must not collide with existing keymaps (`chatKeymap.ts`).
