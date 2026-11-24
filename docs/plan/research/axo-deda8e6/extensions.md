# Extension bridge: embedding-host inventory

- axo revision: `deda8e6`; comparison base: `5c3f418`.
- Axome HEAD: `48f5a03`.
- Date: 2026-11-22.
- Question: What does axo's new Rust/Bun extension system provide, what remains the embedding host's responsibility, and where does it intersect Axome today?

All `crates/...` and `packages/...` references below are to **axo at `deda8e6`**; `src-crates/...`, `nix/...` and `Cargo.toml` are **Axome at the HEAD above**. This is a source inventory, not an adoption design. No builds or tests were run for this report. The generated SDK is source evidence for wire signatures; implementation references distinguish bridge behavior from SDK commentary.

## 1. Manager and observable host contract

`Extensions::new(axo: Arc<Axo>, options: ExtensionOptions) -> Self` performs no discovery or process startup. It takes a provider layer from `axo.models()`, the supervisor from `axo.supervisor()`, and owns session tracking and runner placements. The host must subsequently await `configure(&[EnabledExtension])`; provider declarations are published before callback execution, and runners start lazily (`crates/axo-extensions/src/lib.rs:212-217,362-400`). There is no current Axome manager consumer: Axome constructs ordinary `SessionOptions` and host tools (`src-crates/axome-core/src/services/axo/mod.rs:412-449`), and its existing extension system is a different schema (section 6).

### Complete `ExtensionOptions` fields

There are **seven fields**, not a supervision field (`crates/axo-extensions/src/lib.rs:76-110`):

| Field | Exact Rust type | Meaning / host input |
|---|---|---|
| `bun` | `Option<PathBuf>` | Absolute executable path. `None` makes extensions unavailable; the bridge does not acquire Bun or search PATH. |
| `roots` | `Vec<PathBuf>` | Ordered authored-package roots; each is searched as `<root>/<id>/extension.json`. Host supplies all roots, including any project root. |
| `data_root` | `PathBuf` | Persistent data root. Authored ID maps to `<root>/<id>`; installed ID maps to `<root>/@installed/<publisher>/<name>`. |
| `runtime_dir` | `PathBuf` | Root for private per-generation runner directories and Unix sockets. |
| `env` | `Arc<EnvMap>` | Entire runner environment, supplemented with bridge `AXO_EXTENSION_*` values. |
| `store` | `Option<PathBuf>` | Managed package store; absent means installed packages unavailable and installation unsupported. |
| `auto_install` | `bool` | Whether missing/stale authored dependencies trigger automatic `bun install`; otherwise host calls `install_dependencies`. |

All supplied paths must be absolute; validation occurs on configure (`lib.rs:1303-1323`). The supervisor is inherited from the supplied `Axo`, not independently configured in these options (`lib.rs:367-373`). Bun validation checks an executable regular file, not a download or version lookup (`process.rs:233-242`).

### Calls and lifecycle semantics

| Method | Return / behavior |
|---|---|
| `configure(&[EnabledExtension])` | `Result<(), BridgeError>`; atomically reconciles ordered declarations, last enabled provider declaration wins. Invalid list/options or shutdown fails the operation without changing configuration; package-specific problems appear in statuses. Unchanged placements retain crash budgets. Code-only edits require reload. |
| `status()` | `Vec<ExtensionStatus>`, one per enabled extension in enabled order. |
| `collisions()` | `Vec<ProviderCollision>`, snapshot from configure/retry/reload, not a live registry subscription. |
| `diagnostics()` | `Vec<ExtensionDiagnostic>`. |
| `subscribe()` | `broadcast::Receiver<ExtensionEvent>`, capacity 256; lag requires rereading snapshots. |
| `retry(id)` | `Result<(), BridgeError>`; re-arms disabled/unavailable placement; load-disabled member retried individually, crash-disabled placement gets fresh budget; dependency failures can retry. |
| `reload(id)` | `Result<(), BridgeError>`; replaces placement cold, including grouped peers; clears this member's failure, preserves peers' load failures. |
| `shutdown()` | Async, idempotent; restores attached sessions' host-only registrations, stops tracking, fails pending calls, kills/reaps runners, removes this manager's providers. Drop fences proxies and tears down without waiting. |

Sources: `lib.rs:47,164-180,388-445,567-598`.

Observable Rust types (these manager types are not automatically an Axome frontend wire protocol):

- `ExtensionStatus { id: String, group: Option<String>, generation: u64, phase: ExtensionPhase, reason: Option<String> }`.
- `ExtensionPhase`: `Cold` (published, no runner), `Installing` (calls wait for dependencies), `Starting`, `Ready`, `Restarting`, `Disabled` (load failure or exhausted crash budget), `Unavailable` (setup/discovery/host input problem).
- `ProviderCollision { provider: String, winner: String, hidden: Hidden }`; `Hidden::Extension(String)` or `Hidden::Host(ProviderOrigin)`.
- `ExtensionDiagnostic { id: String, message: String }`.
- `ExtensionEvent`: `StateChanged(ExtensionStatus)`, `Shadowed { provider, winner, hidden }`, `Diagnostic(ExtensionDiagnostic)`, `SessionDiagnostic { session: String, message: String }`.
- Human-readable status reasons accompany disabled/unavailable states and restarts; these are not a separate typed failure-reason enum.

Sources: `lib.rs:113-180`. Axome has no current consumer of these manager events; its existing live session event processing is `src-crates/axome-core/src/services/axo/live.rs:496-505`.

`BridgeError` variants are `InvalidConfig(ExtensionListError)`, `InvalidOptions(String)`, `ShutDown`, `UnknownExtension(String)`, `InstalledDependencies(String)`, `DependencyInstall(String)` and `Registry(axo_ai::Error)` (`lib.rs:193-210`). `Failure` for remote callbacks is separately `Unavailable(String)`, `Method(String)`, `Invalid(String)`, `Removed(String)`, `ManagerGone(String)` (`remote.rs:15-33`).

### Install, update, rollback, deletion, dependencies and memory

- `install(source: &Path, origin: &str, replace_origin: bool) -> Result<InstalledExtension, InstallError>` takes a **local source folder**, not a marketplace identifier or download URL. The host supplies/acquires that folder and origin. Installation copies without `.git`/`node_modules`, runs frozen-lockfile dependency installation when necessary, switches only on success and retains a previous version. Updating is the same call; changed origin is refused unless explicitly replaced. Installation does **not** enable a package. An enabled package reloads immediately, interrupting its and grouped peers' in-flight calls (`lib.rs:487-503`).
- `rollback(id)` swaps current/previous; a second call rolls forward. `uninstall(id, delete_data)` refuses enabled packages, deletes current/previous, optionally the extension data directory, but does not clean arbitrary files written elsewhere. `installed()` returns sorted records and skips unreadable ones (`lib.rs:506-527`).
- `InstalledExtension { id: String, origin: String, has_previous: bool }`; on-disk `install.json` records `{origin,current,previous}` version-directory names (`store.rs:25-45`).
- `InstallError`: `NoStore`, `InvalidOptions(String)`, `Bun(String)`, `Busy(String)`, `InvalidPackage(String)`, `SdkMismatch {id,declared,provided}`, `MissingLockfile(String)`, `OriginChanged {id,installed,offered}`, `InstallFailed {id,output}`, `Enabled(String)`, `NotInstalled(String)`, `NoPrevious(String)`, `Io(String)`, `ShutDown` (`store.rs:47-87`).
- `install_dependencies(id)` is for enabled **authored** packages, runs Bun even after prior failed installation, then reloads; installed packages use `install` instead (`lib.rs:448-484`). Dependency freshness fingerprints `package.json` plus `bun.lock`, stores `node_modules/.axo-install`, serializes installations and remembers unchanged failures. Install deadline is 600 seconds; failure output tail is 4096 bytes. Lifecycle scripts execute: these are trusted packages (`deps.rs:1-5,23-30,87-105,134-145`).
- `memory() -> Vec<RunnerMemory>` returns `{members: Vec<String>, group: Option<String>, pss_bytes: u64}` for live runner containers; shared process memory is not divided among members. Unreadable samples are omitted, not represented as zero (`lib.rs:182-191,529-564`). This is process memory, unrelated to Axome's semantic-memory service (`src-crates/axome-core/src/services/axo/mod.rs:227-238`).

No current Axome consumer implements these package-management/status/memory operations.

## 2. Host inputs, trust and session ownership

### Enabled configuration and discovery

`Settings.extensions` is a **user-only** ordered list of `{ id: String, group?: String }`; omitted list defaults empty. Shared `group` means shared runner. IDs are authored `[a-z0-9][a-z0-9._-]*` or installed `publisher/name`, groups use the plain-name grammar, and repeated IDs are invalid (`crates/axo/src/config.rs:47-51,73-160`). Project settings cannot enable extensions: `EffectiveConfig::resolve` excludes `extensions` among user-only keys (`config.rs:659-676`).

The manager does not read settings files: the host passes the resolved enabled list to `configure`. Likewise it does not discover project trust or choose a project root. `ExtensionOptions.roots` are literal host-supplied directories; the first root containing a matching manifest claims the slot even when unreadable, later duplicates produce diagnostics rather than fallback. Installed IDs use the managed store record instead (`crates/axo-extensions/src/manifest.rs:206-247`). Consequently user-only enablement is enforced by axo's config resolution, while admission of project **code roots** remains a host decision. Axome currently passes `ProjectAccess::Trust` for sessions and supplies config through `AxomeConfig`; it has no axo extension root policy (`src-crates/axome-core/src/services/axo/mod.rs:431-449`; `services/axo/config.rs:21-85`).

### Compose, attach, reconfigure

- `compose(SessionOptions) -> SessionOptions` replaces `options.layer` with extension registrations; host `options.definitions` rank after extensions. It starts nothing (`crates/axo-extensions/src/session.rs:99-106`).
- Host creates/opens using its normal session machinery, then `attach(&Arc<Session>).await`. Attach registers live actions and journal entry forwarding and reapplies current composition to cover a configure between compose and attach. Forks attach the same way (`session.rs:108-158`).
- The manager calls `Session::set_layer`, reapplies attached live sessions on changes, retains diagnostics and emits newly observed `SessionDiagnostic`; child sessions with a spawn link are skipped by root-layer application (`session.rs:265-318`). Host is not required to manually duplicate this reapplication loop after attaching.
- `detach(session_id)` stops tracking but leaves registrations intact. Attached sessions are weakly held; the host remains responsible for retaining session ownership (`session.rs:115-118,145-153`).
- Actions can resolve attached sessions or known children through `Axo::session`; child resolution can revive parked children. Subscription semantics are different: resident children/attached sessions only, ending on loss of residency/detach (`session.rs:160-168`; `packages/axo-extension-sdk/index.ts:103-115`).

Axome's integration seam is its session options/lifecycle and event pump (`src-crates/axome-core/src/services/axo/mod.rs:412-459`; `services/axo/live.rs:496-505`); there is no current compose/attach consumer.

### Optional lifecycle hook, not `host.rs` itself

`crates/axo/src/host.rs` supplies serializable helpers on `Axo`; the optional embedding lifecycle trait actually lives in `crates/axo-extensions/src/lifecycle.rs`.

`set_lifecycle(Option<Arc<dyn Lifecycle>>)` installs/clears a hook with **four required trait methods if implemented**:

- `create(&self, caller: &Caller, cwd: PathBuf, request: SessionRequest) -> Result<Arc<Session>, Error>`;
- `open(&self, caller: &Caller, path: PathBuf, request: SessionRequest) -> Result<Arc<Session>, Error>`;
- `fork(&self, caller: &Caller, session: Arc<Session>, entry: EntryId) -> Result<Forked, Error>`;
- `close(&self, caller: &Caller, session_id: String) -> Result<(), Error>`.

`SessionRequest` has optional `model`, `agent`, `thinking`, `project_access`, plus `sections: Vec<(String, Option<String>)>`. `Forked` carries the session and optional editable prompt parts; extension receives `ForkReceipt {session, editable}`. The host constructs sessions with its normal options, retains them and updates its UI; bridge attaches returned create/open/fork sessions and detaches after successful close. These lifecycle calls run to completion despite caller cancellation/runner loss. Without the hook the error is **`no_lifecycle`**, not generic unsupported (`lifecycle.rs:15-74,93-151,173-193`). `Caller.namespace` identifies the extension (`crates/axo/src/ext.rs:16-20`). Child `session.spawn`/`spawnTeam` are separate harness operations, not additional required host lifecycle methods (SDK `bindings.ts:5099-5106`).

### Login, supervision and Unix constraints

Provider login callbacks use the host's existing `LoginInteraction`: URL + instructions, device code, prompt, selection and progress. Axo stores returned credentials and refreshes through its provider machinery; the bridge routes UI questions rather than providing a GUI (`proxy.rs:104-118,141-217`; SDK `index.ts:61-85`). Axome already supplies all five interactions and starts login through `Models::login` (`src-crates/axome-core/src/services/axo/login.rs:122-168,178-205`).

Runners and installs share `Axo`'s `ProcessSupervisor`; Axome already maps `ProcessGroup` or `Cgroup { program: current_exe, args: [SUPERVISOR_ROUTE], cgroup_root: None }` into its engine construction (`src-crates/axome-core/src/services/axo/mod.rs:314-329`). There is no extension-specific cgroup option. `process.rs` uses Unix filesystem metadata/permissions and `UnixListener`; runner sockets have a 107-byte limit and private generation directories mode 0700 (`crates/axo-extensions/src/process.rs:6,83-96,245-253`). This bridge is Unix-dependent, not a platform-neutral subprocess API. Axome's current Nix desktop/server packages target Linux (`nix/pkgs/axome.nix:100-104`; `nix/pkgs/axome-server.nix:53-57`).

## 3. Contributions and host-visible surfaces

Manifest parsing denies unknown fields. `extension.json` has `id`, optional authored/required installed `sdk`, `entry`, and default-empty/false `providers`, `tools`, `schemes`, `commands`, `sections`, `definitions`, `toolPolicy`, `spawnHook`, `events` (`manifest.rs:149-175,251-291`). SDK compatibility is same major above zero, same minor for `0.y`, exact for `0.0`; current bridge SDK is `0.0.0` (`lib.rs:49-51`; `manifest.rs:270-278,388-390`).

| Contribution | Declared shape and runtime effect | Host-visible intersection |
|---|---|---|
| Providers | `{id,models,env,requiresKey=true,login=false,rewrite=false}`. Static model metadata enters registry; optional callbacks provide login+refresh and request rewrite. Rewrite replaces request, while axo's standard adapter still sends/parses it. | Model/provider catalogs, existing login UI, collision snapshots and provider-origin metadata. No independent extension UI renderer supplied. (`manifest.rs:40-72`; `proxy.rs:104-137`; SDK `index.ts:89-94`.) |
| Tools | `{name,description,parameters,exposure,replay,executionMode,guidelines}`; parameters must be an object JSON schema. Direct/deferred exposure, replay classification and execution mode are part of registration. Handler receives invocation/session/tool context and can emit partial updates. | `Session::tools`, normal tool-execution events/results; `HostTool.origin = "extension <id>"`. Axome already installs web/browser/memory host tools. (`manifest.rs:96-111`; `session.rs:180-199`; SDK `index.ts:117-132`; Axome `services/axo/mod.rs:422-445`.) |
| Slash commands | `{name,description,argumentHint?}`; handler receives trimmed argument string and session handle. | `Session::commands`, command descriptions/hints and `RegisteredCommand.origin = "extension <id>"`. (`manifest.rs:123-130`; `session.rs:222-233`; SDK `index.ts:172-173`.) |
| Tool policy | `toolPolicy: ["before", "after"]` with exactly matching handlers. Before blocks/allows; after can override result fields or terminate. | Ordinary blocked calls/errors and tool result changes, not a new dedicated frontend hook. (`manifest.rs:88-94`; `session/bindings.rs:280-381`.) |
| Spawn hook | `spawnHook: true`; handler allows, blocks, or substitutes an `AgentSpec`. | Child creation/refusal through existing session machinery. (`session/bindings.rs:391-428`.) |
| Schemes | `{name,promptDoc,writeReplay}`; resolve maps to virtual resource or file-backed path; optional virtual read/write. | Scheme prompt docs, file/tool results, collision diagnostics; later extension scheme wins. Axome already supplies web and memory schemes. (`manifest.rs:113-121`; SDK `index.ts:157-170`; `session.rs:66-82,214-221`; Axome `services/axo/mod.rs:452-459`.) |
| Prompt sections | `{name,file}`; contents read statically from package-contained files, not runner callbacks. | Session prompt inspection and diagnostics; later same-name extension section wins. (`manifest.rs:132-137,293-310`; `session.rs:84-93,234-237`.) |
| Definitions | Package-relative directories `agents`, `modes`, `keywords`, **skills**; agent loading also includes **teams**. | Effective session catalogs/commands and definition file paths. Definition loaders emit diagnostics. Composition reverses enabled order because definitions keep first names; code notes agent-vs-team precedence limitation. Host definitions rank afterward. (`manifest.rs:139-147,311-355`; `session.rs:99-105,240-248`.) |
| Journal observation | `events: true` binds `onEntry`. | Stored `SeqEntry` shape and sequence numbers; best-effort append-order delivery, no replay guarantee. Lag, unavailable runners or handler failures can lose events. Explicit subscriptions additionally carry typed session events/lagged/closed. (`session.rs:328-369`; SDK `index.ts:98-115,207-213`.) |
| Journal state / posts | Not separate manifest declarations: callable `appendCustom`, `custom`, `post`. Types are namespaced `<extension-id>:<customType>`; state is branch scoped. | Durable custom journal entries and optional display/message delivery. Axome currently handles generic custom entries separately from `tool_catalog`, with no extension-specific rendering contract. (`crates/axo/src/session/wire.rs:52-80`; SDK `bindings.ts:5088-5098`; Axome `services/axo/live.rs:587-644`.) |

The SDK default export only binds declarations. Missing/extra handler names, providers lacking required login/refresh/rewrite, policy hook mismatches or events/spawn mismatches reject activation (SDK contract `index.ts:20-30`; bridge activation treats handler failure as a member-local load rejection in `placement.rs:493-508`). Origins are concretely present on tools/commands and providers; this does not imply every definition/section has a uniform extension-owner UI field.

## 4. Callable API, generation and deliberately withheld methods

`#[expose]` creates JSON dispatch metadata; every public method in an exposed block must opt in using `#[ext]` or state `#[ext(skip = "reason")]`. Non-wire types require wrappers or explicit withholding (`crates/axo-macros/src/lib.rs:1-18,105-132,437-539`). `ext::objects()` collects session/tools/host/models plus withheld type metadata; bridge advertises only non-skipped methods, augmenting host with lifecycle methods (`crates/axo/src/ext.rs:198-313`; `crates/axo-extensions/src/session.rs:33-54`). `Session::invoke(method, args: Vec<Value>, caller: &Caller)` dispatches positional JSON; SDK bindings provide named typed TS methods and optional trailing `AbortSignal` (`ext.rs:316-327`; SDK `index.ts:1-3`). The generated `extension-api.md` documents this surface, rather than being a separate implementation authority.

The generated SDK callable inventory is broad, not merely provider callbacks:

- **Session state/catalogs**: path, spawn link, agent/model/thinking/service tier/effective tier/config/name; skills/agents/modes/keywords/tools/commands/diagnostics; keyword matching, jobs, command job IDs, context/tree usage, running/live state, entries, context files, file ledger, inspect prompt, skill rendering (`bindings.ts:4888-4995,5038-5051,5131-5138,5177-5193`).
- **Session actions**: set name/model/agent/thinking/tier/mode; run/parse commands; abort/resume wakes; prompt/promptWith, steer/followUp/withdraw; answer/dismiss; custom state/posts; spawn/spawnTeam/send; background/kill jobs; compact/shake/navigate; accept/revise results; reload execution environment; squash changes (`bindings.ts:4923-4930,4950-4963,4990-5037,5055-5110,5140-5176,5195-5210`). Thus extensions can mutate sessions beyond their own tools, subject to harness runtime contracts, not solely a contribution manifest.
- **Paged/history operations**: session entries, command/job raw output or stream-labelled chunks, snapshots as base64; host readSession, snapshots, session listing, squash/project/reconstruct jobs (`bindings.ts:5111-5138,5355-5377,5414-5429`).
- **Host helpers**: file-based catalog (not the host's programmatic definition layer), command/name validation and parsing, agent/mode/keyword parsing, git/project/home-agents paths, argument parsing/substitution (`crates/axo/src/host.rs:38-198`; `bindings.ts:5364-5449`).
- **Models**: find/all/providers/credential-source metadata, token estimates, complete; SDK `models.stream` adds streamed `AssistantEvent` plus closed marker. No credential values in provider metadata (`bindings.ts:5454-5486`; SDK `index.ts:179-186`).
- **Tool-call context only**: cwd, guarded text/base64 reads and writes, supervised command execution, URL/path helpers, scheme documentation, ledger/snapshot/context attachment, artifact allocation/lookup, steering checks, invocation/session IDs and effective bash/write scopes (`bindings.ts:5214-5341`). `tools.checkWrite` is point-in-time symlink-aware scope validation, not containment against concurrent symlink mutation (`bindings.ts:5331-5338`).

Withheld categories are deliberate: raw constructors/builders/dispatchers; live registries/process handles/receivers/readers; host-owned registration setters (`setTools`, `setDefinitions`, `setLayer`, policies, hooks, sections, schemes, commands); raw lifecycle ownership; entire journals/byte arrays replaced with paging/base64; raw custom-state access replaced with caller-namespaced wrappers; derivative value helpers computable in TS. See generated withheld annotations `bindings.ts:4931-4932,4954-4955,4975,5002,5013-5020,5052-5054,5204-5205,5344-5354`, and source `ext.rs:218-269`. Root create/open/fork/close remain host-cooperative; child spawns do not require that lifecycle hook.

These APIs introduce no current Axome direct TS-extension consumer. Their actions intersect the existing Axome session projection, tools and login integration described above; they do not expose a replacement browser-to-Axome network API.

## 5. Failure, process and security model

`remote.rs` is the **internal Rust-to-runner callback path**, not Axome remote-server transport. Calls bind to one current generation and are never retried into a replacement. `proxy.rs` constructs winning providers, routes login/refresh/rewrite and fences retired providers (`remote.rs:35-69`; `proxy.rs:23-45,59-100`). Axome's remote mode is separately `axome-server`/`axome_remote` (`src-crates/axome-server/src/main.rs:4-6,85-109`).

- **Trusted code, not sandboxed**: runner `ExecSpec` explicitly has `sandbox: None`; dependency lifecycle scripts also run unsandboxed. Extensions can use Bun directly, so guarded SDK file/command methods are cooperative safety mechanisms, not confinement of package code (`process.rs:109-118`; `deps.rs:87-105`). Host root admission and user enablement therefore remain material boundaries.
- **Transport isolation**: per-generation private directory, Unix socket, random nonce and generation handshake; runner console output is drained/discarded rather than a host log stream (`process.rs:74-80,98-150,245-253`). Protocol frame cap is 64 MiB, queued bytes twice that, bounded control/cancel/callback slots (`peer.rs:36-49`).
- **Liveness**: handshake 15s; activation 15s; heartbeat every 2s, pong deadline 10s. Refresh/rewrite and state-holding policy/spawn calls use 60s including cold start; login explicitly has no fixed call deadline, allowing human interaction (`peer.rs:47-49`; `placement.rs:33,487-508`; `remote.rs:10-13`; `proxy.rs:106-118,134-136`; `session/bindings.rs:303-308,359-364,411-414`). These are not a blanket timeout on all extension tools or host session operations.
- **Restart/disable**: setup failure is unavailable; member import/activation/dependency rejection disables that member, not all group peers; generation failures restart immediately after first and second failure, disable the placement on third within rolling 300s. Reload/retry behavior and preserved failures are explicit above (`placement.rs:372-410,427-447,513-549`; `placement/budget.rs:8-28`).
- **Fail closed**: unavailable before-policy blocks; unavailable after-policy errors rather than silently preserving a successful result; unavailable spawn hook blocks. A genuinely removed extension on a still-live manager is the exception (allow/no override), whereas stale hooks after manager loss remain failure paths (`session/bindings.rs:314-331,366-380,415-426`; `remote.rs:27-32`).
- **Providers**: retired/interrupted/unavailable callbacks become `ProviderUnavailable`, explicitly not ordinary retryable requests; author/invalid callback failures become authentication/method errors (`proxy.rs:59-100`).
- **Extension-to-host errors**: `{code,message,details?}`; bridge codes include `no_lifecycle`, `no_subscription`, `unsupported`, `stalled`, `unavailable`, `invalid`, `too_large`, `interaction`, alongside invocation/domain errors. Completion errors may carry structured completion details (`peer.rs:51-96`; SDK `index.ts:41-45`).

The host can surface manager status/reasons, collision/diagnostic snapshots, install errors and ordinary session/provider failures. There is no built-in Axome notification, settings page, log viewer or restart UI in this crate.

## 6. Axome's current overlap and runtime packaging

1. **Different existing extension ecosystem.** Axome already loads `extension.json`, but its required schema is `{id,label,version,api,requires?,install_hint?,contributes?}` with languages, grammars, LSP servers, formatters and terminal harnesses. It denies unknown fields and is not the axo manifest (`src-crates/axome-protocol/src/extensions.rs:5-40`). Its catalog/service lives in `src-crates/axome-core/src/services/extensions.rs:279-341,359-500`; host sets installed directory to `dirs.data/extensions` (`src-crates/axome-core/src/host.rs:79`). Sharing the name “extension” does not make the formats or lifecycle compatible. Existing operations expose catalog/grammars/reload, not Bun activation or managed axo packages (`src-crates/axome-core/src/operations/extensions.rs:5-13`).
2. **Settings source.** Axome feeds only the `axo` section of validated settings to the harness. Global `ConfigSource::user` comes from `dirs.config`; project config comes from `cwd/.axome`; files are `settings.jsonc` (`services/axo/config.rs:21-85`; `services/settings.rs:464`). An axo setting existing upstream does not itself configure a bridge: no manager currently consumes `Settings.extensions`. Axome session construction currently trusts project access (`services/axo/mod.rs:439`).
3. **Host tools, schemes and definitions.** Web, browser and memory tools are assembled together with bundled agents/modes/keywords, web/memory URL schemes and folder execution environment (`services/axo/mod.rs:412-459`). New extension registrations intersect those same tool, command, definition and scheme catalogs rather than Axome's LSP/formatter catalog. This is also a direct breaking API touchpoint because current options still initialize `agents`, `modes`, `keywords`, superseded by `definitions` upstream.
4. **Login and provider catalog.** Existing `Interaction` covers all extension login prompts; provider eligibility already comes from `models.providers()`. No extension-specific login widget is supplied by the bridge (`services/axo/login.rs:122-205`).
5. **Live UI state.** Existing event reducer reacts to normal tool messages and `tool_catalog`, and receives generic custom/custom-message entries; extension custom types are namespaced and have no extension-specific UI contract here (`services/axo/live.rs:570-644`). Existing lifecycle ownership would be the host side of optional create/open/fork/close; extension manager never owns Axome's tabs or database records.
6. **Supervision.** Axome already supports process groups and cgroups with its own supervisor route (`services/axo/mod.rs:314-329`; `src-crates/axome-server/src/main.rs:49-53`). New extension manager reuses the same Axo supervisor, not Axome's remote network transport.
7. **Bun at runtime.** Bun is explicitly a development-shell package (`nix/devshell.nix:19`) and frontend build input (`nix/pkgs/frontend.nix:32-48`). Desktop runtime wrapper configures GTK/environment restoration and Playwright driver, not Bun; server launcher configures Playwright and web assets, not Bun (`nix/pkgs/axome.nix:84-98`; `nix/pkgs/axome-server.nix:60-67`). **The packaging inspected does not provision an absolute Bun executable for the application.** [INFERENCE] A user's external environment may still contain Bun; that is not a packaged guarantee and does not satisfy the new manager's absolute-path input by itself. No package build/runtime probe was performed.
8. **Remote mode.** Server accepts independent config/data/cache locations and constructs the host on the server, with its own shutdown and supervisor route (`src-crates/axome-server/src/main.rs:13-25,49-59,69-109`). The extension bridge operates where its supplied Axo/supervisor/filesystem live; it does not transfer packages or spawn Bun on the remote GUI client. Host-side roots/data/Bun availability are therefore also remote-server inventory points, not only desktop concerns.

## Inventory points

1. **New capability — extension manager:** axo supplies lazy provider/session registration and supervised runners; Axome has no current manager consumer, so ownership and exposing its state remain open (`crates/axo-extensions/src/lib.rs:362-400`; Axome `services/axo/mod.rs:412-449`, paths prefixed `src-crates/axome-core/src/`).
2. **New capability — runtime inputs:** host supplies seven options, including Bun/environment/roots/data/runtime/store/auto-install; Axome packaging supplies build-time Bun but no runtime Bun path contract (`lib.rs:79-97`; Axome `nix/pkgs/frontend.nix:32-48`, `nix/pkgs/axome.nix:84-98`).
3. **New capability / security boundary — user enablement:** upstream validates user-only `{id,group?}` lists; host chooses code roots and project trust admission; Axome currently passes trusted project access and has no extension root policy (`crates/axo/src/config.rs:78-160,670-676`; `manifest.rs:206-247`; Axome `services/axo/mod.rs:439`).
4. **New capability — management UI data:** seven phases, reasons, collisions, diagnostics, generation and group metadata exist; Axome has no current consumer or wire representation for those manager events (`lib.rs:113-180`; Axome `operations/extensions.rs:5-13`).
5. **New capability — package installation:** axo manages local source installs, origin-checked updates, rollback and optional data deletion; acquisition, enabling and host UI are not supplied. Axome's existing extension directory/schema is a separate ecosystem (`lib.rs:487-527`; `store.rs:25-87`; Axome `src-crates/axome-protocol/src/extensions.rs:14-40`, `src-crates/axome-core/src/host.rs:79`).
6. **Behavior change / new capability — dependency execution:** authored auto-install or explicit install executes trusted lifecycle scripts with a 600s bound; Axome has no current dependency-install control for axo packages (`deps.rs:23-27,87-105`; `lib.rs:448-484`).
7. **New capability — providers:** extension models/login/rewrite enter the same registry; Axome's existing login interactions overlap directly, while collision/provenance presentation is a new host-visible surface (`manifest.rs:43-55`; `proxy.rs:104-137`; Axome `services/axo/login.rs:122-205`).
8. **New capability — tools/commands/hooks/schemes:** manifest registrations enter ordinary session catalogs/results, with tool/command extension origins; Axome already owns web/browser/memory tools and schemes in that same space (`session.rs:180-233`; Axome `services/axo/mod.rs:422-459`).
9. **Breaking / new capability — definition composition:** `Definitions` includes agents, teams, modes, keywords and skills; Axome currently initializes removed options fields, and extension layers introduce an additional catalog source (`manifest.rs:311-355`; `session.rs:240-248`; Axome `services/axo/mod.rs:434-436`).
10. **New capability — attachment and reconfiguration:** host composes/attaches; axo reapplies root layers, reports diagnostics and restores host registrations on shutdown. Axome currently has no manager attach/detach lifecycle (`session.rs:99-158,265-318`; `lib.rs:567-573`).
11. **New capability — root lifecycle requests:** create/open/fork/close need an optional host hook with `&Caller`; no hook yields `no_lifecycle`, while child spawn already belongs to axo. Axome's session/UI ownership is not handled by the bridge (`lifecycle.rs:37-74,173-193`; SDK `bindings.ts:5099-5106`).
12. **New capability / behavior change — autonomous session actions:** extensions can prompt, steer, alter selections/modes, answer questions and navigate through generated dispatch. Axome's event projection is the overlapping consumer, not a new extension-only session store (`bindings.ts:4990-5176`; Axome `services/axo/live.rs:570-644`).
13. **New capability — durable custom state and posts:** namespaced journal data/messages are available; Axome has generic custom-entry handling but no extension-specific rendering contract (`crates/axo/src/session/wire.rs:52-80`; Axome `services/axo/live.rs:626-644`).
14. **Behavior change — event reliability:** journal listeners are best effort and subscriptions can lag/close; host snapshots and journal paging remain distinct from notifications (`session.rs:328-369`; SDK `index.ts:98-115,207-213`).
15. **Behavior change / security boundary — failure policy:** runner failures share a group crash budget; policies/spawn hooks fail closed, interrupted provider calls do not retry into replacements. Axome has no current extension failure/retry UI consumer (`placement.rs:513-549`; `placement/budget.rs:8-28`; `session/bindings.rs:314-380,415-426`; `proxy.rs:59-100`).
16. **New capability — process accounting and platform:** per-container PSS is available and bridge is Unix-dependent; Axome already supplies Linux process-group/cgroup supervision, but extension memory reporting has no consumer (`lib.rs:529-564`; `process.rs:83-96`; Axome `services/axo/mod.rs:314-329`).
17. **No direct impact on remote protocol — internal proxy:** `remote.rs`/`proxy.rs` mean runner RPC/provider adapters, not axome-server transport; desktop and server still each need their own host runtime inputs if extensions are enabled (`remote.rs:35-69`; `proxy.rs:23-45`; Axome `src-crates/axome-server/src/main.rs:69-109`).
