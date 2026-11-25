# Axome <img src="src-tauri/icons/icon.svg" width="36" height="36" align="top" alt="" />

**A Linux desktop workspace for coding-agent CLIs.**

Keep using Claude Code, Codex, OMP, or Antigravity in the terminal. Axome puts a file editor, Git diffs, and local issues alongside them, so you can follow the work and review changes without bouncing between separate apps.

Tabs can belong to different repositories. Switch tabs, and the file tree and Git sidebar follow.

Axome is a passion project in active development, built around a simple idea: keep the agent's own interface, and make the work around it easier.

![Axome in action: switching tabs, editing files, reviewing diffs, and launching agents](docs/readme/tab-driven-workflow.webp)

## Try it

With [Nix](https://nixos.org/) and flakes enabled:

```sh
nix run github:tophc7/Axome
```

Install and authenticate whichever agent CLI you want to use separately. Axome detects supported installed agents; you can also open a regular terminal using your login shell.

<details>
<summary>Install on Debian, Fedora, or Arch</summary>

Packages use your distribution's GTK, WebKitGTK, and graphics drivers.

### Debian

Download the `.deb` for your architecture from [GitHub Releases](https://github.com/TophC7/Axome/releases/latest), then install:

```sh
sudo apt install ./axome_*_$(dpkg --print-architecture).deb
```

Available for Debian 13+ and Ubuntu 24.04+ on `amd64` and `arm64`. To update, install a newer release with the same command.

### Fedora

Download the `.rpm` for your architecture from [GitHub Releases](https://github.com/TophC7/Axome/releases/latest), then install:

```sh
sudo dnf install ./axome-*-1.$(uname -m).rpm
```

Supports Fedora 40+ on `x86_64` and `aarch64`. To update, install a newer release with the same command.

### Arch

Download `axome-bin-<commit>.tar.gz` from [GitHub Releases](https://github.com/TophC7/Axome/releases/latest) into an empty directory, then unpack and install:

Using `yay`:

```sh
tar -xf axome-bin-*.tar.gz
yay -Bi .
```

Using `makepkg`:

```sh
sudo pacman -S --needed base-devel
tar -xf axome-bin-*.tar.gz
makepkg -si
```

Supports `x86_64` and `aarch64`.

</details>

## Why I built Axome

I've always liked VS Code's straightforward workflow. I was much less happy with it's use of Electron, especially on Wayland, and wanted something that felt familiar without relying on it.

As coding agents became part of my daily work, I found I really only needed a file tree, Git diffs, an editor, and a terminal. I wasn't using any of the rest of the "IDE" or its extensions anymore; but VS Code's workflow was still the only that worked for me.

Axome grew out of that: a workspace for the parts I actually use when working with agents. It's evolved into a tab-driven workflow of its own, while keeping enough familiarity that VS Code users should feel at home.

## What you can do

### Keep agents and repositories in reach

Run agent sessions and ordinary terminals in tabs, with each tab tied to its folder. The sidebar follows the selected tab, and a folder stays open until its last tab closes. Open multiple windows and drag tabs between them.

The home dashboard reads local Claude Code, Codex, and OMP histories to show projects with a seven-day activity heatmap, so you can find where you left off.

In native Axo chats, each run of tool calls and thoughts is one work block drawn as a rail of one-line rows (`Read 3 files`, `Ran make test`, …); opening a row shows its output in a single box. A block stays open while it runs and folds to `N tool calls · N thoughts` once the reply continues. Every subagent, team and background command gets one card or row per block that merges its spawn, follow-ups, waits, notes, result and stop; **View Agent** and **Open in Jobs** jump to the sidebar. The end of a turn shows one line: `Stopped.` (naming jobs still running), a **Turn Failed** alert, or a note such as the output token limit.

`Alt+O` cycles through folded work, call summaries, and open thoughts/tools (including nested calls), then folds everything again, resetting individual folds. Opening calls keeps long outputs truncated; use each output's More control to reveal the remaining lines. Thoughts and tool outputs start folded.

Live chats label tools, slash commands, and model providers contributed by extensions. Extension-authored messages and posts keep their attribution when reopened. When a tool policy blocks a call, the transcript keeps it visible outside folded work and shows its recorded reason, or offers **Open Extensions** when the policy extension isn't running. Expand the call to see the original output.

Extension diagnostics appear above the transcript. If an extension's provider becomes unavailable or its runner stops, **Open Extensions** or **View Details** opens Settings on that host and project. **Retry** only retries the extension; it never sends or replays a chat message.

A pending `ask` renders in the transcript; questions asked back to back share one card. Clicking an option, or pressing `Enter` on it, picks it and collapses the question to its answer row (click the row to change it); `Enter` also moves on to the next unanswered question, then to **Send answers**. `Space` selects without collapsing (and ticks options in pick-any questions), `←`/`→` step numbers, typing on a highlighted text question edits it, `Ctrl+Enter` sends, and `Alt+D` dismisses the questions; axo continues with its own judgement. `Alt+A` or **Jump to questions** moves the keyboard to the first open question; typing in the composer instead steers, answering in your own words.

The keyboard moves through the transcript the way the turn dots on the left do: `↑`/`↓` step between turns and, inside open questions or reviews, between their items; tool calls are not stops. `↑` from the start of the composer (or `Alt+↑` from anywhere) enters the transcript, and `Esc` steps back out to the turn, then to the composer. Every chat key, navigation included, is rebindable in **Settings → Keyboard Shortcuts** under **Chat**, **Chat Input** and **Chat Navigation**.

Assistant messages render fenced code blocks in framed cards with language headers, syntax highlighting, word-wrap and copy controls, scrollable height with a full-height toggle when long, and a **Preview / Raw** toggle for `markdown` blocks (up to 3 nested preview levels). Compaction and other host work render as a collapsible horizontal rule at the point they happened (after the footer when they end a finished turn), labelled like `Compacted · before → after`, expanding via click or `Alt+O` into the same scrollable/full-expand markdown card. Memory captures and live maintenance show in the same kind of rule. Tool output lists each recorded file access and shows saved diffs when available, even if the call did not finish. Finished turns with saved changes (including `local://` session files) show a collapsible **N files changed** card with net `+A −D` counts; unfinished calls are marked there and in the Files sidebar. **Review** or clicking a file opens a dedicated **Turn Review** diff stack backed by `axo`'s content-addressed turn snapshots, independent of `git status`. When there is room on the left edge, a vertical turn-navigation dot rail highlights the active turn, previews each turn's prompt, final reply snippet (when present), index, and timestamp on hover, and scrolls so that turn's end rests at the bottom of the view (with top overscroll back to the first turn's end).

While a turn runs, its working line shows three animated dots and a theme-colored wave over the elapsed time. **Stop** sits at the far right of that line with a red icon, outside the input box and without a keyboard hint; reduced-motion settings keep the indicator static. In a child transcript, `Esc` stops that child, not the parent chat.

The Axo chat composer works like a chat app's:

- **Send & Queue Controls:** one button sends when idle and becomes **Steer** during a turn (including settling pending `ask` questions or reviewed `yield` prompts when a steer arrives). Hold `Alt` to switch it to **Follow-up**, queuing your message after the current turn; releasing `Alt` restores Steer. `Enter` uses Send or Steer, `Shift+Enter` inserts a newline, and `Alt+Enter` queues a follow-up while running. Queued messages appear in a collapsible **Queued Messages** card above the composer (in both main chats and the subagent inspector) with per-row **Promote to steer / Interrupt & send now** (`→`), **Edit** (prepends back into the composer above your draft), and **Cancel** actions; messages held when a run stops or fails stay visible until resumed or withdrawn.
- **Commands:** type `/` for the session's **Commands** (built-ins and registered handlers), **Modes** and **Skills**. Built-in arguments are highlighted and checked as you type; arguments with fixed values (modes, models, true/false) list them at the caret. Registered commands take free text without attachments. They preserve internal spacing, quotes and newlines, and keep the draft if the handler fails. Commands that need an idle session are disabled while a turn runs.
- **Modes:** `/<mode> text` enters the mode and sends the rest of the message, chips and attachments included. The **Session** sidebar panel shows the mode and switches or leaves it. Entering or leaving a mode during a turn takes effect at the next model request; until then a bar above the composer names the pending change.
- **Personas:** when you have personas (agents with `runAs: root` or `any`), a new chat offers them as cards, and the **Session** sidebar panel or `/persona <name>` switches between them and **Default** later. A persona replaces the agent's instructions and tool set and may switch the model; harness rules stay in place.
- **Keywords:** enabled keywords are highlighted in the composer as you type, using axo's own matching, so the highlight shows what will fire. A keyword that fired is underlined inside the sent message, with a hover naming it and its definition; a fired keyword whose word can no longer be located shows as a chip under the message instead. Fired keywords are recorded in the session journal, so the marks are still there after reopening the chat.
- **Setup in the footer:** changing persona, mode, model, effort or tier writes nothing into the transcript. Each answer's footer names the persona and mode it ran under next to model and effort, brightening what changed since the previous answer (`reviewer → Default` when it changed partway through). While a persona, mode, model or effort change has not reached the model yet, a bar above the composer says what the next message (or, during a turn, the next step) will use. Between turns, **Cancel** puts back what the last answer ran with; it is not offered while a turn runs, because the bar can then list what the running request already uses.
- **Mentions:** type `@` to mention a project file or folder as a chip; start with `../`, `/`, or `~/` to mention anything else on the chat's machine, and press `Tab` on a folder to list its contents. The paperclip opens the location browser to attach any file or folder there (`Ctrl+Enter` picks the open folder). Files axo's machine can see, including drops, are referenced by path rather than copied. Pastes, and local files dropped on a remote chat, are copied into the session (up to 20 MiB per message). Long pastes collapse into a chip.
- **Chips everywhere:** chips look the same in replayed, reopened, and forked chats.
- **Links:** `artifact://`, `local://`, and `skill://` links open the file from that chat's own session, even when the chat isn't loaded.
- **Keys:** a chip sits in the text like one character: the caret steps over it, `Backspace` deletes it whole, and it selects, copies, and pastes with the text around it. `Esc` closes a list first, then stops the run.

A collapsible right-hand **Session Sidebar** keeps live session activity out of the scrolling transcript:

- **Subagents & Teams:** lists standalone subagents and collapsible team groups, lets you inspect a child's transcript live (Enter steers it while it runs, Alt+Enter queues a follow-up) with **Stop** on its row and **Open in Tab**, and opens from **View Agent** on the job cards in the parent chat. Input the parent agent sends to a child is labelled **From parent agent** (or the sender's name) in the child's transcript; what you type there keeps your label. Nested subagent rows expand under their root session in the sessions sidebar and home history, and history search matches a subagent's model; opening a closed subagent opens a read-only snapshot first, with **Resume** in the footer to bring it live.
- **Background Jobs:** streams live output (capped at a 128 KB tail with an **Open Full Output** `artifact://` link and **Stop** control while running) and reads the saved artifact after reopen, with stderr shown in red and a note when capture was incomplete. Foreground `bash` calls also offer a **Background** button while running; if you stop a turn while a background job is running or its result is waiting, a banner above the composer says so and offers **Resume**.
- **Toolbox:** **Tools** occupies the upper pane; drag the horizontal divider to resize it against the lower definitions pane, like the Git sidebar. Tools show direct, promoted and deferred exposure, current-mode restrictions, and parameters without promoting deferred tools. The lower pane switches between **Skills**, **Agents**, **Modes** and **Keywords**, with a **Diagnostics** tab when definitions failed to load. Its header follows the selected catalog. Entries show their details (model, tool limits, read-only `bash`, write scope, reviewed results, team members, `/command`-only skills, …) and open their definition file.
- **Plan:** shown while a plan mode is selected or a plan review is pending. It renders the `local://` draft live as the agent writes it, and **Edit** opens the draft in an ordinary editor tab; what you save there is what gets kept. See **Plan and research modes** below.
- **Session:** the chat's title (click to rename; clear it to go back to the first message), status, start time, persona, mode and model sit above the context and usage divisions. Under **Context**, one usage meter shows how full the model's context is and when automatic compaction starts. Fixed, bounded **Context** and **Usage** divisions share the native sidebar header styling. **Conversation**, **Instructions**, and **Tools** are compact graph rows, ordered by estimated token size; click to expand into additional aligned rows for prompt sections, loaded instruction files, or declared tool definitions. Right-aligned **?** controls explain the context estimates and session usage. **Usage** always shows cumulative tokens and cost, including subagents, separately from the current window; **Root** and **Subagents** are evenly spaced subtotals whose labels align with **Cost**. The shortened session ID at the right of the Session header copies the full ID when clicked. Use `/compact` to compact manually.
- **Files:** native folder trees for the session's `local://` files and the files the agent edited or read (from axo's file records, not git). Local, Edited and Read have independent folder expansion, directory-chain compaction, and alphabetical folder-first ordering like the main file tree. Local files show their size; edited and read files show current git status, which reflects the working tree rather than proof of what the agent changed. Files outside the project retain an absolute-path root.

While a model request is being retried after a transient provider error, the running reply shows `Retrying n/max · in Ns` with the provider's error. Compaction and other maintenance outside a turn shows as such a rule at the end of the transcript (**Compacting**, **Shaking**, **Summarizing branch**), with the same retry text in a row beneath it while retrying.

When a mode other than a plan mode ends with a reviewed result, the transcript shows it in a card with **Accept**, **Request Changes** (with your feedback, continuing the mode) and **Dismiss**. A review still pending when you close the tab or quit comes back when you reopen the chat, because reopening resumes the waiting run.

Axo chats are configured entirely by Axome; `~/.axo` is never read:

- **Providers:** **Settings → Providers** sets or removes API keys for providers that accept them, runs browser sign-in for providers that offer it, and shows each provider's status and credential source (`models.jsonc`, a stored key, a login, or an environment variable). Custom models and providers live in `~/.config/axome/models.jsonc`, edited as a file; changes apply to open chats without reopening them, and a broken file is reported there while the last good models keep working. **Refresh Models** fetches the latest model catalog. Model lists in open chats and Settings update by themselves after any of these changes; a chat whose model is no longer available asks you to pick another before sending.
- **Subscriptions:** ChatGPT Codex (`openai-codex`) and the Claude subscription (`anthropic-auth`) come from [Axo-Extras](https://git.ryot.foo/toph/Axo-Extras). Under **Settings → Providers**, use **Log In**: Codex completes in your browser; for `anthropic-auth`, authorize in your browser and paste the returned `code#state` or hosted callback URL into the masked prompt. Each lists the models the signed-in account offers, without API pricing. Sign-in, token refresh, and requests run on the selected host; subscription accounts and API keys are separate (`anthropic-auth` and `anthropic` are two providers). Axome does not switch to `anthropic` if the service rejects a model or blocks access. **Log Out** removes the stored subscription credential on that host.
- **Grounded web search** uses the chat credentials for supported backends. Claude subscription is chat-only. The search model is **Settings → Tools → Grounded fallback model** (a `provider/id` or a role), else `@dash`.
- **Browser automation:** the deferred `browser` tool drives a user-installed Chromium-based browser (auto-detected from `PATH`, such as Helium, Chrome, Chromium, Brave, Vivaldi, or Edge, or set via **Settings → Tools → Browser** / `axo.browser`) for interactive page inspection (`@e1` ARIA snapshots), DOM actions, screenshots, and console/network error capture.
- **Model roles:** **Settings → Models** sets the default model and maps `@default`, `@reason`, `@work`, `@dash`, `@conduct`, `@memory` (background memory capture) and your own roles to models. An unset role inherits `@default`; an unset `@default` uses the last model picked in a chat. Model selectors can include effort and service-tier suffixes (`provider/model:xhigh:fast`, `@review:flex`, or `:fast`). `:standard` requests Standard explicitly; leaving the tier absent lets the applicable default or policy choose it. **Default effort** is the fallback when the default selector has no effort suffix.
- **Model options:** the control beside the model picker changes reasoning effort and service tier. Cycle reasoning effort with `Alt+T`; use `/tier fast` (or `standard`, `flex`, `ultrafast`) to change tier from the next request. An unsupported tier falls back to Standard without losing the selected preference. **Settings → Models → Tier after switching model** (`axo.serviceTier.onModelChange`, `reset` by default or `keep`) controls whether a model change without an explicit tier resets it. **Settings → Tools → Subagent tier** (`axo.subagents.serviceTier`, `standard` by default or `inherit`) applies when a subagent selector has no tier suffix. Each answer's footer shows the tier when it is not Standard, and **Response Details** shows each request's recorded model, effort, tier and estimated cost.
- **Agent settings:** the **Agent** pages of Settings (**Providers**, **Models**, **Behavior**, **Tools**, **Project Access**) edit settings under `"axo"` in `~/.config/axome/settings.jsonc`. A folder's `.axome/settings.jsonc` can override permitted keys, but not global-only keys such as `project`, `homeAgentsDir`, `modelRoles`, `serviceTier`, `subagents`, `extendedContext` or `browser`. Changes apply to chats started or reopened afterwards.
- **Axo extensions:** author host packages in `~/.config/axome/extensions/` or project packages in `.axome/extensions/`. Enable them with the global `axo.extensions` list or `axo.projectExtensions` (a map from canonical project folder paths to lists); entries use `{ "id": "scratch" }` and can optionally name a runner `group`. Later entries win competing provider declarations. Extensions run as your account under Bun; enable only code you trust. Axome can also install packages from host folders or HTTPS/SSH Git repositories. Nix packages ship Bun; Tauri-bundler builds fall back to Bun on the host's `PATH`.
  **Settings → Extensions** installs, updates, rolls back, and removes host packages; **This Project** manages that folder's extensions and their provider credentials separately. Enabled entries can be reordered or grouped into shared runners. Changes update open root chats. **Runner Memory** reports PSS per process, once on opening and every two seconds while its list is expanded and visible.
- **Default persona:** `"defaultAgent"` (**Settings → Behavior → Default Persona**) names the persona new chats start with. A name that isn't a root persona logs a warning and the chat starts on **Default**; reopened and forked chats keep their own persona.
- **Plans directory:** `"plansDir"` (**Settings → Behavior → Plans Directory**, default `.axome/plans`) is where accepted plans are saved. A relative path is taken from the chat's folder; an absolute one is used as is. It is read each time a plan is saved, from the folder's `.axome/settings.jsonc` when set there, otherwise from the global settings. `/plan:step` looks for roadmaps there too, using the value read when the chat opened.
- **Bundled subagents:** `scout` (`@dash`, kernel-enforced read-only `bash`) and `task` (`@work`) are built in and overridable with `.md` files in `~/.config/axome/agents/` and `.axome/agents/`. `memory-writer` (`@memory`) runs memory capture and is overridable from `~/.config/axome/agents/` only.
- **Bundled keyword:** writing `orchestrate` in a message has the agent split the request across subagents and integrate their results, and switches the chat to `@conduct`, which stays for later turns. Turn it off with the keyword toggles in **Settings → Behavior**.
- **Bundled modes:** `/research`, `/plan`, `/plan:roadmap` and `/plan:step` (see **Plan and research modes** below), overridable with `.md` files in `~/.config/axome/modes/` and `.axome/modes/`.
- **Your files:** user prompts, agents, modes, keywords and skills live in `~/.config/axome/{prompts,agents,modes,keywords,skills}` and `~/.config/axome/AGENTS.md`; a folder adds `.axome/{prompts,agents,modes,keywords,skills}` (alongside `.agents/skills`). In the editor, their frontmatter offers key and value completion and hover descriptions, and flags invalid values (and, in agent files, unknown keys). Credentials are stored in `~/.local/share/axome/axo/auth.json`, sessions in `~/.local/share/axome/axo/sessions/`, and the model catalog cache in `~/.cache/axome/axo/`.
- **Chat tabs:** the dot at the end of a chat tab means working (pulsing), needs you (yellow), error (red) or finished since you last looked (plain); hovering swaps it for the close button. Long titles fade out.
- **Closing a chat tab ends its session,** asking for confirmation when background jobs or subagents are active and stopping them when confirmed, even when another window or browser is attached (that one shows Reconnect). Closing a secondary desktop window or a web workbench does the same for its chats. Just closing the browser leaves server-side sessions running.

**Plan and research modes.** Four modes are bundled. Only you can enter them (`/name`, optionally followed by a prompt), they run on `@reason`, and they are overridable like any mode:

- **`/research`:** open-ended investigation with no deliverable. The agent keeps notes under `.axome/research/` and scratch work in `local://`; the mode lasts until you exit it.
- **`/plan`, `/plan:roadmap`, `/plan:step`:** the agent drafts one implementation plan, a multi-phase roadmap, or the plan for one phase of an existing roadmap in `local://`, then submits it for review.

When a plan is submitted, a **Plan ready for review** marker in the transcript opens the **Plan** view, which adds an editable **Save as** path, a feedback box and five actions:

- **Request Changes:** sends your feedback; the agent revises the same draft and submits again.
- **Work:** saves the plan, ends the mode, and the same run goes on to implement it.
- **Compact & Work:** saves the plan, ends the run, compacts the conversation, then starts a turn that implements the saved plan. A chat too short to compact skips compaction (the "Nothing to compact" notice still shows) and starts the turn anyway.
- **Work in New Session:** saves the plan and opens a new chat whose first message points at it.
- **Save Only:** saves the plan, ends the mode and leaves the chat idle.

Saving copies the draft (with your edits) to the plans directory and deletes it from `local://`: plans as `<name>.md`, roadmaps as `<name>/roadmap.md`, and step plans beside their roadmap as `<name>/<NN>-<step>.md`. An existing file is never overwritten: the action fails, the review stays pending, and you can change **Save as** and try again. A name that doesn't fit the mode's pattern is refused the same way.

**Write scope and read-only `bash`.** Modes and agents can limit `write` and `edit` with a `writes:` frontmatter list of `local://` and directories (relative to the chat's folder, or absolute); `[]` allows no writes and an absent list leaves them unrestricted. The plan modes allow only `local://`; `/research` adds `.axome/research/`. axo enforces the scope, and subagents (including nested ones) inherit it as a ceiling they can only narrow. `bash: readOnly`, which all four modes set, runs `bash` in a kernel sandbox (Linux Landlock, kernel 6.2+) that blocks writes everywhere except scratch space: `/tmp`, an absolute `TMPDIR` that exists, the session's `local://` folder and `/dev/null`. A project that itself lives under one of those paths is therefore **not** protected from `bash` writes in these modes; the `write`/`edit` scope still applies there. Keep projects you plan or research in outside those paths if that matters.

<details>
<summary>📷 See the dashboard and an agent workspace</summary>

![Project dashboard with seven-day agent activity](docs/readme/home.png)

![An OMP session beside Axome's Git changes and commit graph](docs/readme/git-gragh-diff_omp-session.png)

</details>

### Give agents a long-term memory

Memory is off by default. **Settings → Memory → Enable Memory** (global settings only, `"memory": { "enabled": true }`) gives Axo chats a notebook of plain Markdown files that outlives any one session: decisions and their reasons, facts about a project, and your preferences. The files are the source of truth; edit them in the editor like any other file (**Banks → Open** opens a bank's folder). A derived search index lives in `~/.local/share/axome/memory/index.db` and is rebuilt from the files whenever it is missing or out of date, including after hand edits.

- **Notes:** each note is a Markdown file with optional YAML frontmatter (`title`, `type`, `permalink`, `tags`). A list item `- [category] text` is a fact (an observation), optionally with `#tags`, a trailing `(context)`, and a validity range right after the category: `@2026-11-21`, `@[2026-01-01,2026-07-01)` or `@valid:2026-11`. `- relation_type [[Other Note]]` is a typed link; any other `[[Other Note]]` counts as `links_to`. A link may point into another bank as `[[bank::Other Note]]`. The format follows [Basic Memory](https://github.com/basicmachines-co/basic-memory)'s.
- **What the agent gets:** one tool and one URL scheme. `memory_search` ranks notes by keyword and meaning together, with filters for row type, category, tags, path, date and `valid_at`; each result gives the note's `memory://` path, and the first result from a note also lists up to ten notes it links to and from. `memory://<bank>/<path>` (`memory://main/projA/Tooling.md`) makes a bank an ordinary folder for `read`, `write`, `edit`, `grep`, `find` and `bash`, so memory edits appear in the turn's changed-files card and follow the session's write scope: `/plan` and `/research` cannot change memory. The bundled `scout` subagent gets `memory_search`.
- **Semantic search:** on by default once memory is enabled (`"semantic"`, also global only). Axome downloads `BAAI/bge-small-en-v1.5` (133 MB) once into `~/.local/share/axome/memory/models/` and runs it on the host's CPU; until it is ready, search is keyword-only. **Settings → Memory → Search** shows the download and retries a failed one.
- **Banks:** a bank is a folder of notes. Every project uses the global bank `main` (`~/.local/share/axome/memory/banks/main/`) unless its settings say otherwise. `"banks": { "<name>": { "path": "…" } }` defines more (an absolute path, `~/…`, or a path relative to the project; no path means `~/.local/share/axome/memory/banks/<name>/`), and `"bank"` picks the one a project writes to. A project's `.axome/settings.jsonc` may set `"bank"`, `"banks"` and `"exclude": true` (no memory in that project); a bank inside the repository travels with it through git. Memory belongs to the host that runs the chat; remote hosts keep their own banks.
- **Automatic capture:** after a chat compacts and after you close a chat (`"capture": { "compaction": true, "close": true }`), a background writer reads the new part of the conversation and records durable facts as observations, under `<project>/<Topic>` or `me/<Topic>` pages. The writer's own session is deleted after each capture. The model decides what to keep and whether a fact is new, already known, or replaces an older one; a replaced fact is closed with an end date rather than deleted. It runs on the `@memory` role (**Settings → Models**); unset, `@memory` follows `@default` and spends that model's tokens, so a local model is a good fit. The writer only adds and closes facts; it never reorganizes notes. **Settings → Memory** shows the last capture and any error.

Memory has no Workspace Trust gate yet: any project you open can point memory at another bank or path through its `.axome/settings.jsonc`.

### Edit and review the work

Browse files, edit code, and review changes before committing:

- **Side-by-side or unified Git diffs** with hunk actions, commit and working-tree comparisons, and file or clipboard compare.
- **Full Source Control workflow** with staged and working changes, commit variants (**Commit & Push**, **Commit & Sync**, **Amend**, **Commit All**, **Sign Off**, **Skip Hooks**, **Undo Last Commit**), and branch sync.
- **Merge, rebase, cherry-pick, and conflict resolution** with paused-operation controls (**Continue**, **Skip**, **Abort**), whole-file or per-block resolution in Monaco (**Accept Current**, **Accept Incoming**, **Accept Both**), and conflict navigation.
- **Interactive Git graph and ref management** across **Branches**, **Tags**, **Stashes**, and **Worktrees** tabs—check out, cherry-pick, revert, reset, manage remotes, or **Stash & Checkout** when switching with uncommitted edits.
- **Monaco code editing** with built-in language server (LSP) support (Rust, TypeScript/JavaScript, Svelte, Python, Nix, Fish, HTML, CSS, Tailwind, Biome) and Shiki highlighting.
- **Markdown editing** with a live preview styled to match GitHub as closely as possible.
- **File tree** with filtering, drag-and-drop, status-aware Git actions, and safe deletion/discard to the system trash.
- **Runnable project tasks** defined in `.axome/tasks.jsonc`.

<details>
<summary>📷 See Git diffs, the code editor, Markdown preview, and command palette</summary>

![Axome showing a split commit diff beside the file changes and Git graph](docs/readme/commit-diff.png)

![Code editing in Axome](docs/readme/monaco-file.png)

![Markdown source and live preview in Axome](docs/readme/markdown-split.png)

![Workbench command palette](docs/readme/pallete.png)

</details>

Use `Ctrl+Shift+P` for workbench commands, `Ctrl+P` to find files, or `Ctrl+O` to open a folder or remote host. The command palette also has modes for editor commands (`>`), runnable tasks (`!`), and files (`/`), and exposes the full set of `Git` actions. Shortcuts are customizable.

### Isolate work with Git worktrees

Because tabs in the same folder share its working tree, you can spin up separate Git worktrees when running multiple agents in parallel. Create, open, and remove worktrees from the Git sidebar's **Worktrees** tab, branch menus, or commit history, or use **Migrate Changes to Main** to move uncommitted work back to the primary checkout.

To copy untracked local files (such as `.env`) into new worktrees automatically, configure `git.worktreeCopy` in `.axome/settings.jsonc`:

```jsonc
{
  "git": {
    "worktreeCopy": {
      ".env": true,
      ".local": true,
      ".local/private": false
    }
  }
}
```

Tasks in `.axome/tasks.jsonc` can also run automatically when a worktree is created:

```jsonc
{
  "version": 1,
  "tasks": [
    {
      "id": "setup",
      "label": "Set up worktree",
      "command": "bun install",
      "runOn": "worktreeCreated",
      "confirm": true
    }
  ]
}
```

### Use your project's Nix environment

Axome detects `flake.nix`, `shell.nix`, and `default.nix`. Select and evaluate an environment from the Nix applet at the foot of the sidebar rail; once ready, it is used for new terminals, agent sessions, runnable tasks, and language servers.

Everything Axome launches starts from the environment Axome itself was launched with, minus Axome's own packaging edits (the GTK wrapper and devshell record each variable's prior value as `AXOME_ORIG_<NAME>`, and Axome restores it). That environment is resolved through your login shell (`$SHELL -l`, including home-manager session variables), and the folder's Nix environment is layered on top; session values such as `DISPLAY`, `WAYLAND_DISPLAY`, or `SSH_AUTH_SOCK` always come from the host. After changing your login files, run **Reload Environment** from the command palette; new terminals, sessions, and tasks pick it up without restarting Axome.

The sidebar rail's foot holds the Nix environment, remote servers, notifications and the app menu; the title bar cell above the sidebar shows the current folder.

<details>
<summary>📷 See a terminal session</summary>

![Terminal session with the project environment status visible](docs/readme/terminal.png)

</details>

### Terminal presets

Define shell launch variants under `shell.presets` in `~/.config/axome/settings.jsonc` or a folder's `.axome/settings.jsonc`. Each preset can override the shell `command`, append `args`, add `env`, and type a `startup` line into the shell after it starts. `shell.defaultPreset` picks what **New Terminal** (Ctrl+T) opens; every preset also gets its own New Tab card and palette command.

```jsonc
{
  "shell": {
    "defaultPreset": "dev",
    "presets": {
      "dev": { "args": ["-l"], "env": { "RUST_LOG": "debug" }, "startup": "git status -sb" }
    }
  }
}
```

A task in `.axome/tasks.jsonc` can run inside a preset with `"shell": "dev"`: the preset's `startup` runs first, then the task's `command`. Tasks without `shell` run under your login shell.

### Keep local tasks alongside the code

Track issues with priorities, group them into epics, and see progress in the sidebar. Agents can query, create, and update issues through a local socket API rather than editing the database directly.

<details>
<summary>📷 See the new tab page and issues sidebar</summary>

![New tab page beside the local issues sidebar](docs/readme/new-session.png)

</details>

### Work on remote hosts or in the browser

Run `axome-server` on another Linux machine to open remote folders from the desktop app over QUIC (with mTLS fingerprint pairing), or enable its web listener to use the Axome workbench directly in a browser. Agent sessions and tasks keep running on the server if you disconnect or close the window, and reattach when you return.

The flake exports `packages.<system>.axome-server`, `nixosModules.axome-server`, and `homeManagerModules.axome` for declarative setups, alongside `.deb`, `.rpm`, and tarball server artifacts on GitHub Releases.

Set `"supervision": "processGroup"` (the default) or `"supervision": "cgroup"` in
`~/.config/axome/server.jsonc` to choose command process supervision. Cgroup mode
requires delegated cgroup access; without it, command execution fails with
`Command supervision unavailable:` rather than falling back.

The NixOS module exposes `services.axome-server.supervision`, defaulting to
`"cgroup"`, and enables systemd delegation for that mode. Set it to `"processGroup"`
to disable delegation. Supported cgroup deployments are the NixOS module and
direct binaries with delegated cgroups. The portable tarball's `ld.so` launcher
makes `current_exe()` point to the loader, so cgroup supervision fails closed there;
use `"processGroup"` with that package.

## Worth knowing

- **Active development, Linux-first.** Expect rough edges and changes as the project grows. I do daily-drive it however, so it's stable enough.
- **Sessions in the same folder share its working tree.** Tabs do not create isolated worktrees; create a separate worktree for independent edits.
- **Where Axome keeps things.** The desktop app and `axome-server` share one XDG layout: config in `~/.config/axome`, data in `~/.local/share/axome` (desktop `axome.db` and its webview storage under `webview/`, or `axome-dev.db` and `webview-dev/` under `bun app:dev`, server `server.db`), cache in `~/.cache/axome`. `axome-server` relocates each with `--config-dir`, `--data-dir` and `--cache-dir`, or through `XDG_*_HOME`; the NixOS module sets those variables through its `xdgConfigHome`, `xdgDataHome` and `xdgCacheHome` options. All config files use `camelCase` keys and values; old `snake_case` keys are reported as unknown settings.
- **Your agent is still your agent.** Axome runs the CLI itself; model information and other output inside its terminal come from that CLI.
- **Integrations.** Language support, formatters and terminal harnesses are bundled as integrations. Additional packages use `<data dir>/integrations/<id>/integration.json`; the old package directory is not read. Run **Reload Integrations** from the command palette to reload the active host's catalog.
- **Coming from Sworm?** Global settings, app-owned state (the cached session index, Nix env cache) and server state start fresh under the Axome names; nothing in `~/.config/sworm`, `~/.local/share/dev.sworm` or `~/.cache/sworm` is read or migrated. Sessions that harnesses keep in their own files are still discovered. Only per-project folders carry over: rename each repo's `.sworm/` to `.axome/` (its `issues.db` keeps working as-is), then update `.sworm/` paths and `dev.sworm.*` integration IDs in its settings and tasks. Remote desktop and `axome-server` must both be Axome builds; the wire protocol is `axome/1`.

## Development

Nix provides the development toolchain and Linux desktop dependencies:

```sh
git clone https://github.com/tophc7/Axome.git
cd Axome
nix develop
bun install
bun run app:dev
```

Build and run the Nix package from the checkout:

```sh
nix build
nix run .
```

Axome uses [Tauri v2](https://tauri.app/) and Rust for the desktop runtime, with the system Git CLI and SQLite. The interface is built with Svelte 5, SvelteKit, and Tailwind CSS v4; editing uses Monaco and Shiki, and terminals use xterm.js. Nix defines the development environment and packaging.

Typography is bundled: [Paper Mono](https://paper.design/mono) (OFL) is the default for the interface, editor, and terminal. The interface face enables all of its stylistic sets, including duospace and narrow spaces; code surfaces keep a fixed grid with only ligatures, small arrows, single-story a, and slashed zero. [Symbols Nerd Font Mono](https://www.nerdfonts.com/) supplies icon glyphs as a fallback. Override either family under **Settings → Appearance → Typography**.

## Feedback

If you try Axome, I'd like to hear what works for you and what gets in the way. [Open an issue](https://github.com/tophc7/Axome/issues) with bugs, ideas, or how you use coding agents.

## Credits

- [Emdash](https://github.com/generalaction/emdash) was an early architectural inspiration for Axome's agent-focused workspace.
- [VS Code](https://code.visualstudio.com/) shaped the workflow and UI familiarity I wanted to keep. Axome also uses Microsoft's [Monaco Editor](https://microsoft.github.io/monaco-editor/), the editor behind VS Code.
- Memory is a port of the core of [Basic Memory](https://github.com/basicmachines-co/basic-memory) (AGPL-3.0-or-later): its note format, index, link resolution and hybrid search.

## License

[AGPL-3.0-or-later](./LICENSE)
