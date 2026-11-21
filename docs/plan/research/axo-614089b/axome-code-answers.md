# Axome code answers for the axo 614089b adoption — 2026-11-20

Paths under `/repo/rust/axo` refer to axo `614089b`; other paths refer to Axome. These are source findings, not runtime verification.

## B1 — Axome-side login constraints

**Evidence**

- The current host service accepts only `openai-codex`, starts one cancellable login task, returns a URL and login ID, accepts one manual redirect string, and exposes pending/done/failed polling. Starting another login cancels the previous one (`src-crates/axome-core/src/services/axo/login.rs:29-158`).
- The UI opens the URL, offers copy/open controls and a paste field, polls every second, displays errors and a spinner, and cancels on dismissal (`src/lib/features/settings/views/ProvidersView.svelte:117-164,337-366`).
- The path is frontend `backend.axo.loginStart/Submit/Status/Cancel` → server-routed RPC → host operations → `AxoService`; the same server-routed contract supports a local desktop host or a remote host (`src/lib/api/backend.ts:694-705`, `src-crates/axome-protocol/src/rpc.rs:642-652`, `src-crates/axome-core/src/operations/axo.rs:38-55`).
- The callback listener runs **on the machine hosting axo**. Codex redirects the browser to `localhost:1455`, while the listener binds server-side `127.0.0.1:1455`; a browser on another machine therefore does not reach that listener without separate forwarding. The paste path remains available, including when binding fails (`/repo/rust/axo/crates/axo-ai/src/auth/codex.rs:27-28,52-105`; `src/lib/platform/web.ts:22-27`).
- Available primitives include `Dialog`, `Input`/`Textarea` with native input attributes, `Select`, `Button`, `Alert`, `Badge`, `Spinner`, `ProgressBar`, and `ProgressRing` (`src/lib/components/ui/dialog/index.ts:3-13`, `src/lib/components/ui/input/Input.svelte:11-18`, `src/lib/components/ui/select/index.ts:1`, `src/lib/components/ui/progress/index.ts:1-3`).

**Options seen**

- Existing Axome implements URL + one paste prompt + polling/cancel. axo’s generic interaction supports open URL/instructions, device code, secret or non-secret prompts, select and progress; accommodating sequential interactions would require a richer host/UI exchange than the current single-string RPC. Remote users need the manual path unless a separately designed forwarding mechanism exists.

## B2 — Provider cards

**Evidence**

- Backend counts registry models per provider and reports credential source as config, stored API key, OAuth, environment variable, or none (`src-crates/axome-core/src/services/axo/providers.rs:10-44`).
- A card is “configured” when source is not none **or** no key is required. It displays source badges `models.jsonc`, `Stored key`, `Logged in`, `Environment`, or `No key needed`; unconfigured keyed providers show the relevant environment names. The separate `OAuth` badge is driven by the old `provider.oauth` capability (`src/lib/features/settings/views/ProvidersView.svelte:51-90,217-251`).
- OAuth-capable cards show **Log Out** only for an OAuth source, otherwise **Log In**. Key-required providers show **Add Key** or **Update Key**; stored API keys additionally show **Remove**. Removal deletes stored credentials, not config/environment credentials (`ProvidersView.svelte:252-295,310-316`; `providers.rs:47-73`).
- At `614089b`, capability is `ProviderInfo.login`, meaning a `ProviderLogin` exists; login can return an API key, not necessarily OAuth (`/repo/rust/axo/crates/axo-ai/src/models/mod.rs:40-60,369-416`).

**Options seen**

- Preserve separate capability and source indicators, but the existing “OAuth” capability label would misdescribe a generic login returning a key. Capability-driven Login and source-driven Logout/Remove are distinct choices; the current card already treats them separately.

## B3 — Runtime providers and request rewrite

**Evidence**

- No Axome registration of axo runtime providers or `RequestRewrite` implementation was found in `src-crates`; the `.register` calls in the Axome axo service register **sessions**, not providers (`src-crates/axome-core/src/services/axo/mod.rs:430-474,788-801`).
- Web search and docs are host tools with their own HTTP/auth machinery, not runtime model providers (`src-crates/axome-core/src/services/axo/web/mod.rs:31-94`). Grounded-search compatibility is a separate auth/request issue, detailed in B5.
- Axo’s roadmap says the Rust seam is implemented but Bun bridge and extension loader are not; the external Claude-subscription provider is an **acceptance goal for the future bridge**, not a shipped Axome provider (`/repo/rust/axo/plan/plan.md:297,349,371-380`). Axome’s adoption research describes the same as roadmap context (`docs/plan/research/axo-614089b/providers-and-login.md:61,81`).

**Options seen**

- Adopt the new registry APIs without registering any Axome-owned provider now, or add a concrete Rust provider if a real current requirement is identified. The surveyed Axome code and plans do not establish that runtime registration or a request rewrite is required today.

## B4 — Registry lifetime and live sessions

**Evidence**

- `Engine::build` creates one `Arc<Models>` and passes a clone into `Axo::builder`; Axome’s listing uses that same engine’s `models` (`src-crates/axome-core/src/services/axo/engine.rs:49-91`; `src-crates/axome-core/src/services/axo/mod.rs:680-705`).
- The engine slot is rebuilt when absent or when the models-file metadata stamp or `extendedContext` changes. `invalidate()` only clears the slot; **Reload Models** calls that invalidation, and the next engine access builds anew. Provider key changes, removal and completed login also invalidate (`mod.rs:295-337`; `src-crates/axome-core/src/operations/axo.rs:34-35`; `providers.rs:47-73`; `login.rs:61-80`).
- A `LiveSession` holds its own `Arc<Engine>`; loading an already-held session returns it before obtaining the current engine. Invalidation/rebuild neither closes nor migrates these sessions: they retain the old `Arc<Axo>` and `Arc<Models>`, while new listing/open/create operations can use the new engine (`src-crates/axome-core/src/services/axo/live.rs:115-120`; `mod.rs:400-430,461-474`).
- At `614089b`, `reload(config, extended_context)` rebuilds **in place** using the loaded catalog, keeps registrations and retains the previous state on error; `refresh()` fetches catalog data and rebuilds in place. Sessions sharing that registry adopt changed definitions before subsequent requests; a removed selected model eventually yields `UnknownModel` (`/repo/rust/axo/crates/axo-ai/src/models/mod.rs:72-86,216-255`; `/repo/rust/axo/crates/axo/src/session/mod.rs:1122-1145`).
- **No registry change notification API was found**: the registry exposes snapshots and mutation methods, but no watch, subscribe, event, or generation counter for refresh/reload/login changes. Codex login schedules refresh asynchronously after credential storage (`/repo/rust/axo/crates/axo-ai/src/models/mod.rs:88-140,340-426`).

**Options seen**

- Current Axome rebuild isolates already-open sessions from the new registry. A long-lived shared `Models` plus in-place reload would let those sessions follow changes, but frontend freshness still needs an explicit host mechanism; axo does not supply a registry notification. Config reload and network refresh are different operations.

## B5 — Grounded web search

**Evidence**

- Search has its own provider order: dedicated providers and Parallel first, then active supported grounded provider, configured grounded-model provider, and `openai-codex`, `anthropic`, `openai`, `google` fallback (`src-crates/axome-core/src/services/axo/web/providers.rs:18-20,280-325`).
- Grounded defaults are Codex `gpt-5.1-codex-mini`, Anthropic `claude-haiku-4-5`, OpenAI `gpt-4o-mini`, and Google `gemini-2.5-flash`. It builds provider-specific HTTP requests and response parsing itself (`web/grounded.rs:15-40,62-65,185-195,211-239,372-377`).
- `WebTools` owns credentials and a separate HTTP client, not `Models` (`web/mod.rs:31-66`). Grounded auth reads stored credentials/environment directly and performs its own locked Codex OAuth refresh; unavailable credentials are skipped (`web/auth.rs:127-185`; `web/providers.rs:115-135`).
- Settings › Tools has its own grounded-model selector. `grounded_model_for` accepts only a `provider/id` whose provider matches the chosen backend; it does not resolve roles through the registry (`src/lib/features/settings/views/ToolsView.svelte:290-295`; `web/providers.rs:317-325`).
- At `614089b`, `Models::auth(&Model) -> Result<ModelAuth, Error>` first resolves the model against the **current registry**. `ModelAuth` returns `api_key`, `headers`, and `base_url`; it does not itself return a request rewrite (`/repo/rust/axo/crates/axo-ai/src/models/mod.rs:418-480`; `/repo/rust/axo/crates/axo-ai/src/auth/mod.rs:15-25`).

**Options seen**

- Keep the dedicated grounded auth/HTTP implementation, or provide it access to shared `Models` and look up an actual registered model before calling `auth`. The latter can honor resolved key, headers and base-URL override, but **does not automatically apply provider request rewrite** or supply a grounded-search adapter. A configured/default grounded model absent from the registry needs an explicit policy; `auth` cannot operate on an arbitrary nonexistent model.

## C4 — Model-list caches and push channels

**Evidence**

- Chat caches a catalog per folder, deduplicates loads, and force-reloads loaded folders after provider actions or scoped settings changes. A cached catalog otherwise remains until explicitly reloaded or its folder is released (`src/lib/features/chat/models.svelte.ts:8-91`; `src/lib/features/settings/views/ProvidersView.svelte:29-39`; `src/lib/features/app-shell/AppShell.svelte:115-124`).
- Settings `axoModels(host)` creates a separate one-shot state per call, used by Models and Tools views; it has no subscription or refresh method (`src/lib/features/settings/state/axo.svelte.ts:7-24`; `src/lib/features/settings/views/ModelsView.svelte:27-30`; `src/lib/features/settings/views/ToolsView.svelte:24-27`). Providers view separately caches its provider payload and refreshes on mount and its own actions.
- Host pushes currently include files, git, settings, recent folders, tasks, Nix, sessions, issues and chat summaries; there is no models-changed event (`src-crates/axome-core/src/events.rs:21-37`). Settings watcher emits `SettingsChanged`; desktop translates host events into frontend events (`src-crates/axome-core/src/services/settings_watcher.rs:206-221`; `src-tauri/src/host_events.rs:44-57`).
- Remote server translates `HostEvent` to `HostEventWire`, broadcasts it, and sends claimed events over the desktop remote event stream or browser control WebSocket. Global settings events are not folder-claim restricted; folder events are (`src-crates/axome-server/src/events.rs:8-23,48-79,95-116`; `src-crates/axome-server/src/web.rs:450-476,616-618`; `src-tauri/src/router/remotes.rs:511-523,595-630`; `src/lib/api/transport.web.ts:58-69,434-441`).

**Options seen**

- Reuse the existing scoped settings-change pathway for changes genuinely tied to settings, or introduce a distinct model-registry event through HostEvent, wire/desktop/web mappings and all cache consumers. The current settings signal cannot by itself observe axo’s background registry refresh, and one-shot settings caches currently do not follow even that signal.

## C5 — Error and empty-state presentation

**Evidence**

- Invalid `models.jsonc` or model load failure causes the engine to retry with default config and retain the original error as `models_error`; Providers displays `models.jsonc: <error>` (`src-crates/axome-core/src/services/axo/engine.rs:47-73`; `src/lib/features/settings/views/ProvidersView.svelte:203-208`).
- `available_models` checks auth once per provider and excludes that provider’s models when auth fails; it does not return the per-provider auth reason (`src-crates/axome-core/src/services/axo/mod.rs:685-705`).
- Providers distinguishes configured/unconfigured credential source and shows environment hints, but readiness is source presence/no-key-required, not credential validation (`ProvidersView.svelte:51-90,217-251`).
- The generic picker’s empty result is `No matches`. An existing chat with no available models disables the model picker; its empty transcript shows “No axo models are available on this host. Add a provider in Settings › Providers.” A catalog RPC error uses a danger alert instead (`src/lib/features/chat/elements/model-picker/ModelPicker.svelte:177-184`; `src/lib/features/workbench/surfaces/chat/ChatSurface.svelte:39-70,210-218,333-344`).
- New chat chooses the resolved default or first available model; if neither exists it fails attachment with the generic “No axo models are available” hint, shown in the closed-connection alert (`src/lib/features/chat/controller.svelte.ts:125-133,155-162`; `ChatSurface.svelte:265-269`).

**Options seen**

- Current UI distinguishes invalid config in Providers and RPC failure in chat, but an empty filtered list conflates absent credentials, absent/incomplete catalog and other auth failures. Distinguishing them requires backend status beyond the current list response.

## C6 — Thinking and model changes

**Evidence**

- Journal model and thinking entries convert into `modelChange` and `thinkingChange`; live handling updates the selected model/thinking and context window, then broadcasts the entry (`src-crates/axome-core/src/services/axo/convert.rs:684-691`; `src-crates/axome-core/src/services/axo/live.rs:596-607,643-655`).
- `transcriptTurns` records these changes in turn settings; changes before the first content establish initial configuration and are not rendered as steps. Later changes become steps, but `AgentBlock` has no rendering branch for either `modelChange` or `thinkingChange` (`src/lib/features/chat/turns.ts:125-155`; `src/lib/features/chat/elements/transcript/AgentBlock.svelte:169-264`).
- The composer visibly shows the **current** model and reasoning effort and sends manual changes as commands. Completed assistant turns show a footer containing the assistant model and recorded thinking level; there is no explicit “changed from X to Y” transcript line (`src/lib/features/workbench/surfaces/chat/ChatSurface.svelte:39-49,110-120,333-355`; `AgentBlock.svelte:363-368`).

**Options seen**

- Continue reflecting the resulting selection in composer and turn footer, or add an explicit transcript notice for automatic clamps and/or manual changes. Today neither change entry itself is visibly rendered.

## C7 — Cost and context

**Evidence**

- Model-picker rows and field triggers display the model’s context window (`src/lib/features/chat/elements/model-picker/ModelPicker.svelte:115-123,215-224`). The context sidebar displays used/window tokens, percentage, meter and compaction threshold (`src/lib/features/chat/sidebar/ContextPanel.svelte:155-205`).
- Session usage shows total cost and, when available, root/subagent breakdown; each completed response’s details tooltip shows its cost. The compact sidebar label includes cost only when positive (`ContextPanel.svelte:290-313`; `src/lib/features/chat/elements/transcript/AgentBlock.svelte:325-360`; `src/lib/features/chat/sidebar/SessionSidebar.svelte:71-76`).
- `formatCost` formats zero as `$0.00`; it does not distinguish zero price from unknown price (`src/lib/features/chat/format.ts:13-15`).
- **Extended context is exposed** as a toggle in Settings › Behavior › Context (`src/lib/features/settings/views/BehaviorView.svelte:135-140`). The engine includes it in the rebuild stamp (`src-crates/axome-core/src/services/axo/mod.rs:309-311`).

**Options seen**

- Current UI shows effective context sizes and numeric costs but no visible explanation of unknown prices, tier pricing, or why a window is capped. Explanatory labels or richer price provenance would require data beyond the current displayed aggregate.

## U1 — Saved or selected model no longer available

**Evidence**

- `lastExplicitModel` is saved only after a successfully resolved explicit model command. Config fills unset built-in roles from configured `@default`, otherwise that last explicit selector; it does not validate that fallback while filling roles (`src-crates/axome-core/src/services/axo/mod.rs:1485-1515`; `src-crates/axome-core/src/services/axo/config.rs:49-58,114-138`).
- The chat catalog suppresses default resolution errors and exposes the default only if its selector appears in available models. Role resolution errors become null; resolved role models are filtered by available **provider**. New chats fall back to the first available model, or fail if none (`mod.rs:733-779`; `src/lib/features/chat/controller.svelte.ts:125-133`).
- Settings’ field picker preserves a saved unknown concrete selector and marks it `unavailable`; unknown roles show `Unknown role`, while a known unresolved role shows `Not set`. Settings can also type custom selectors, including tool/subagent selectors (`src/lib/features/chat/elements/model-picker/ModelPicker.svelte:64-93,107-127,180-199`; `src/lib/features/settings/views/ToolsView.svelte:115-126,194-224`).
- An open chat’s ghost picker displays an unknown selector as raw text without the field variant’s unavailable warning. Sending remains enabled if the live model string exists, even if it is absent from `available`; with no available models the picker is disabled (`ChatSurface.svelte:39-51`; `ModelPicker.svelte:124-127`).
- On `614089b`, a session retains a removed selected model, but its next request fails `UnknownModel`; a manual model command resolves the selector and can fail earlier (`/repo/rust/axo/crates/axo/src/session/mod.rs:1122-1145,2408-2418`; `/repo/rust/axo/crates/axo-ai/src/models/mod.rs:438-451`). Command failures are shown as notifications (`ChatSurface.svelte:110-120`). A failed run/assistant error becomes a danger **Turn Failed** alert with its reason or assistant error text (`src/lib/features/chat/turns.ts:170-183`; `src/lib/features/chat/elements/transcript/AgentBlock.svelte:299-305`).
- Grounded-search’s per-tool selector is an exception: its own parser only accepts matching `provider/id`, otherwise it falls back to that backend’s hardcoded default; it does not use `Models` selection or resolve `@role` (`web/providers.rs:317-325`; `web/grounded.rs:62,185,223,375`).

**Options seen**

- Existing behavior preserves unknown settings values for repair, but new chats silently choose another available model, whereas an already-open session can retain and attempt its old selection. A future registry-lifetime choice changes whether that old session sees removal at all (B4); the UI can independently choose whether to identify the unavailable selection and explain fallback or request failure.