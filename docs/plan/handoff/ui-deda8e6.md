# UI/UX needs from Axome's `deda8e6` adoption

- **For:** the UI/UX implementation pass.
- **The UI work is defined by Toph's mockup:** `/home/toph/Projects/ui.html` (12 boards). It is a mockup. Axome's own design system (`skill://axome-design`) decides components, tokens and styling.
- This document lists the facts each design has to respect, maps the mockup to them, and names what the mockup does not cover.
- **Source:** [`../plan.md`](../plan.md) T13, decisions [P32, P33, P34](../design.md#decisions).
- **Factual background:**
  - [`../research/axo-deda8e6/extensions.md`](../research/axo-deda8e6/extensions.md) (extension manager, states, contributions);
  - [`../research/axo-deda8e6/providers-credentials-and-tiers.md`](../research/axo-deda8e6/providers-credentials-and-tiers.md) (service tiers);
  - [`../research/axo-deda8e6/extension-management-ux.md`](../research/axo-deda8e6/extension-management-ux.md) § "What to borrow, and from whom" (how other harnesses manage extensions; reference only).
- **Fixed constraints:**
  - Axome conventions: `skill://axome-design`, `AGENTS.md` styling rules.
  - D4: every axo setting gets Settings UI, and `settings.jsonc` is only backing storage.
  - D4: Workspace Trust follows VS Code (Restricted Mode banner, `Trust Folder` / `Manage`, persistent badge). D4 is designed but not built.

## Mockup boards

| # | Board | Section below |
|---|---|---|
| 0 | Tier in the composer: supported, remembered-but-unavailable, Standard only | 1 |
| 1 | Tier after the fact: turn footer and Response Details | 1 |
| 2 | Settings → Models: default and roles with tiers | 1 |
| 3 | Settings → Extensions: a normal day | 2 |
| 4 | Settings → Extensions: when something needs attention (incl. Bun missing) | 2 |
| 5 | One extension's own page | 2 |
| 6 | A failed update, with its output | 2 |
| 7 | Install (folder, Git), uninstall, replace source | 2 |
| 8 | Runners | 2 |
| 9 | Extensions in the composer and model picker | 3 |
| 10 | Extensions in the transcript | 3 |
| 11 | File changes: several files per call, changes from a failed call | 5 |

### Not in the mockup, no board needed

- **Subagent tier** (`subagents.serviceTier`: Standard / Inherit): one more row in the existing subagent settings under Tools.
- **Providers page:** an extension can add a model provider (board 9's `acme`), and it shows in Settings → Providers like any other provider. If the provider has its own login, the existing login dialog runs it. Two additions: the row says which extension it comes from, and when it reuses a built-in id (an extension shipping its own `anthropic`), the row shows that it replaces the built-in one (board 5: "Hides the built-in acme provider").
- **§6, Integrations:** renaming labels only.

### Implement to axo's rules, not the mockup's examples

- **Ids:** extensions in the user folder or a project folder have plain ids (`scratch`, `release-notes`). `publisher/name` is only for installed packages.
- **Command clash:** an extension command beats a skill with the same name. It loses only to a built-in command or a mode.

## 1. Service tiers

**Facts**
- Tiers: `standard` (default), `flex`, `fast`, `ultrafast`. Each model declares which non-standard tiers it supports and a price multiplier for each (for example 0.5×, 2×). Many models support none.
- Three distinct values:
  - **chosen:** the tier the session asked for;
  - **effective:** what the current model can actually send. An unsupported choice is remembered but sends standard;
  - **recorded:** the tier that actually served each assistant message.
- The chosen tier applies from the next request. It can change mid-session through `/tier <tier>` or through a selector suffix.
- Selectors (default model, roles, subagent models, the picker) can carry a thinking suffix and a tier suffix, in either order: `provider/model:high:fast`, `@role:flex`, or only `:fast`. Today Axome's selector handling understands only the thinking suffix.
- On model change the tier resets to standard, unless the user setting `serviceTier.onModelChange` is `keep`.
- Subagents start at standard, unless the user setting `subagents.serviceTier` is `inherit`, or the child's own selector names a tier.
- Compaction always runs at standard.
- The cost of a message includes its tier multiplier. The multiplier itself is not stored on the message.
- **Unknown prices (`118a716`):** a model without known prices has no cost, which is different from $0. A turn, request or session total that contains any unknown part is unknown. Today Axome shows these as $0.

**Needs**
- Choosing a tier for a session, and seeing which tiers the current model supports and what each costs relative to standard.
- Showing when the chosen tier is not effective on the current model.
- Editing the tier part of selectors wherever selectors are edited: model picker, roles, default model, subagent model fields.
- Settings for `serviceTier.onModelChange` (`reset` / `keep`) and `subagents.serviceTier` (`standard` / `inherit`).
- Seeing which tier served a response or turn, including turns where requests used different tiers.
- Showing an unknown cost wherever costs appear (Response Details, turn footer, context panel, subagent rows), including totals that are unknown because one part is.

## 2. Extension management

**Facts**
- Extensions are trusted TS/JS packages run by Bun. They can run code as the user, install npm dependencies (lifecycle scripts run) and act on sessions.
- Three package sources:
  - user-authored folders on the host;
  - project folders inside a repo;
  - installed packages, which axo copies into its own store.
- Enabling is a separate step: a **user-only**, **ordered** list of `{ id, group? }`. A project cannot enable its own extensions. Order decides which extension wins a provider id. A shared `group` puts several extensions in one runner process.
- Project extensions additionally need the folder trusted (D4), and they depend on an axo change that is in progress. They ship together with user and installed extensions.
- States an enabled extension can be in, each with an optional reason text:

  | State | Meaning |
  |---|---|
  | Cold | ready, starts on first use |
  | Installing | installing dependencies |
  | Starting | starting |
  | Ready | running |
  | Restarting | restarting after a crash |
  | Disabled | stopped after a load failure, or after 3 crashes in 300 s |
  | Unavailable | setup problem: missing package, missing Bun, bad input |

- Operations axo provides:
  - Retry (re-arm a disabled or unavailable one);
  - Reload (pick up code edits);
  - install from a local folder, and update by installing again (an update from another origin is refused unless replacement is confirmed);
  - rollback to the previous version (one previous copy is kept);
  - uninstall, refused while enabled, optionally deleting the extension's data directory;
  - install dependencies now.
- Install errors are typed: missing Bun, missing `bun.lock`, invalid manifest, SDK version mismatch, origin changed, `bun install` failed (with an output tail), already installing.
- Install sources are a folder on the host and a Git repository (branch or commit). Axome fetches the package; axo installs it from that folder.
- The manager also reports:
  - provider collisions (which extension or host provider a winner hides);
  - diagnostics per extension and per session;
  - memory (PSS) per runner process. A shared runner reports one number for all its members.
- Bun is packaged with Axome, but it can still be missing or broken; extensions are then Unavailable.
- On `axome-server`, all of this happens on the server, so paths are server paths.

**Needs**
- Seeing every extension: source, enabled or not, order, group, state and reason.
- Enabling, disabling, reordering and grouping.
- Installing, updating, rolling back and uninstalling, including the "delete data" choice and the origin-change confirmation.
- Retry, Reload and Install dependencies.
- Showing install errors and output tails, diagnostics and provider collisions.
- Showing runner memory.
- The Bun-unavailable state.
- Project extensions: how a folder's extensions appear relative to its trust state, and that trust alone does not enable them.
- Extension problems that happen outside Settings: a crash-disabled runner, a session diagnostic, a failing provider.

## 3. Extensions inside chat

**Facts**
- Extension tools and slash commands carry an origin (`extension <id>`). Registered commands appear in the command catalog after built-ins and modes. Today the composer shows only built-ins, modes and skills.
- Extensions can add model providers. They show up in the model list and the Providers page like any other, and can hide a host provider with the same id. A **project** extension's provider exists only in that folder: it appears in that folder's model picker and nowhere else. Its login and key live in Settings → Extensions → **This Project**, next to the extension that adds it (P38), and run through the existing login dialog. Settings → Providers stays host-wide.
- Extensions can post custom messages into a session, and keep namespaced custom state.
- Extensions can prompt, steer, follow up, switch model, mode or tier, answer questions and navigate in a session. Prompts, steers, follow-ups and posts record which extension sent them (`Extension { id }`); model, mode and tier changes record no sender.
- A failing extension can block a tool call (fail-closed policy) or fail a model request (`provider_unavailable`).

**Needs**
- Showing where a tool, command or provider comes from.
- Extension slash commands in the composer.
- Showing extension posts in the transcript.
- Showing that a prompt, steer or post came from an extension.
- Showing that a blocked call or failed request came from a failing extension.

## 4. Tabs opened or closed by extensions: deferred

Axome does not install axo's `Lifecycle` hook for now ([P36](../design.md#decisions)). An extension that tries to create, open, fork or close a root chat gets a `no_lifecycle` error, and nothing happens in Axome. No UI work. Subagents an extension spawns inside a chat are unaffected; they don't use this hook.

## 5. File changes in the transcript

**Facts**
- A tool call reports a list of files, each with its access (read or write) and, for writes, a change with a diff and before/after snapshots.
- One call can touch several files.
- A call that failed or was hard-stopped after writing to disk still reports the changes it made.
- Today Axome shows one diff per call, only for successful `edit`/`write` calls. The turn card `N files changed +A −D [Review]` and the sidebar Files panel make the same assumption.
- Old chats lose their inline diffs (P32).

**Needs**
- Diffs for several files in one call.
- Changes from failed or interrupted calls, in the call, the turn card and the Files panel.

## 6. Integrations rename

**Facts.** Axome's own language, grammar, LSP, formatter and terminal-harness packages are renamed from "extensions" to **Integrations** (P34). "Extensions" now means axo packages only.

**Needs.** Every user-facing label, command and message for the old system says Integrations, for example "Reload Extensions" and "Extension not loaded". Nothing should be ambiguous between the two systems.

## 7. Ask documents (axo `9c1b4ad`)

**The UI is defined by Toph's mockup `/home/toph/Projects/ask.html`:**
- waiting in the chat flow;
- answered, as history;
- dismissed, steered, timed out, refused, scrolled away;
- Other and notes.

**Facts**
- One document, read top to bottom: Markdown blocks (model-written, render as Markdown) and question blocks in between. Question kinds:
  - `choice`: single, or `multi`; options with label, description and `recommended`;
  - `text`: `multiline`, `placeholder`, `prefill`;
  - `number`: `integer`, `min`, `max`, `default`.
- Every question can be `optional` (skippable).
- **Other:** every choice accepts a free-text answer. On single choice it replaces the pick; on multi it adds to the picks. The model never adds its own "Other".
- **Notes:** a note is allowed on choice and number answers only, never on text. A note needs an answer; a skipped question can't carry one.
- **Numbers:** axo has **no step**. The mockup's "step 0.05" and "Shift for 10" are UI choices. `integer` means whole numbers; min/max are inclusive.
- **Timeout:** off by default (`ask.timeoutMs`, already in Axome's settings). When it fires, only `recommended` / `prefill` / `default` are submitted. A required question without one is sent as no answer (`null`); the mockup's timed-out board doesn't show that case.
- **Validation:** axo checks every submission against the document. An invalid one is refused with a reason and the questions stay open (the mockup's "refused" board).
- **Steering:** a message typed into the composer while questions are pending answers them as "steered".
- **Several asks at once:** asks can run in parallel, so more than one document can be pending in a chat. The mockup shows one.
- **History:** a settled card is rebuilt from the call's arguments (the document) and its result (`details.answers`, `timedOut`). Old chats' ask cards lose their answers (P32).

## Out of scope

Re-login after the bump (Providers already shows logged-out state) and queue display (no visible change).
