# axo models batch for Axome

- **Target repository:** `/repo/rust/axo`, branch `main`. Toph makes these changes; Axome's implementers never modify axo.
- **Inspected revision:** `614089b8a801d26b04e65330abc47924f4973cf1`
- **Status:** fulfilled in `118a716` (2026-11-23): `Models::subscribe`, `Models::logout`, and `Option` costs on models and usage. Adopted in [`../plan.md`](../plan.md) T13.
- **Source decisions:** [`../design.md`](../design.md) ([P22](../design.md#decisions), [P24](../design.md#decisions), [P27](../design.md#decisions)) and [`../plan.md`](../plan.md) ([T11](../plan.md#t11-axo-614089b-providers-login-and-the-live-catalog))
- **Evidence:** [`../research/axo-614089b/`](../research/README.md)

Axome adopts `614089b` without waiting for this batch. Only P22's registry-driven refresh (S) and P24's logout (L) depend on it; until they land, Axome refreshes its lists after its own actions and removes credentials itself.

## Summary of items

| # | Area | Summary | Axome consumer |
|---|---|---|---|
| S | axo-ai registry | A change signal for the model registry | Global `ModelsChanged` host event: composer catalog, Settings model fields, Providers view |
| L | axo-ai auth | `Models::logout(provider)` that also clears account-scoped caches | Provider card **Log out** / **Remove** |
| U | axo-ai model | *Optional.* Unknown prices distinguishable from $0 | Cost displays (not scheduled in Axome) |

---

## S. Registry change signal

**Problem.** The registry now changes on its own: the refresh timer (`crates/axo-ai/src/models/mod.rs:257-275`), the Codex account list fetched after a built-in login (`:394-423`), `reload` (`:223`), `refresh` (`:251`), `register`/`unregister` (`:282`, `:333`). Every path ends in `State::rebuild` (`:129-142`), which replaces `snapshot`, but nothing tells the host. A host that shows model lists can only re-read `all()` after actions it started itself, so a timer refresh or the post-login Codex list stays invisible until something else forces a reload. Codex CLI (`models-manager` watch receiver), OpenCode (`ModelsDev.Event.Refreshed`) and Zed (`selector.watch()`) all expose one ([`../research/axo-614089b/harness-model-catalog.md`](../research/axo-614089b/harness-model-catalog.md) C4).

**Required change.**

- `Models::subscribe(&self) -> tokio::sync::watch::Receiver<u64>` (or an equivalent cheap, cloneable signal): a generation number that increases whenever the published snapshot changes.
- Bumped from the one place the snapshot is replaced, so every mutation path is covered, and only when it actually changes (an unchanged refresh does not wake subscribers).
- No payload beyond the generation; hosts re-read `all()` / `providers()`.

## L. Logout

**Problem.** A host logs out by removing the credential from its `CredentialStore` directly; the registry never hears of it. The discovered Codex account list (`Inputs.codex`, `mod.rs:101-102`, cached at `<cache_dir>/codex-models.json`, `models/codex.rs:29`) survives logout and is not keyed by account, so after logging out (or into a different ChatGPT account) the registry keeps listing the previous account's models until a later refresh succeeds.

**Required change.**

- `Models::logout(&self, provider: &str) -> Result<(), Error>`: removes the stored credential for `provider` under the store's lock, then drops any account-scoped state axo holds for it. For built-in `openai-codex`: forget the discovered list, delete its cache file, rebuild from the offline floor (and bump S).
- Unknown provider: `Error::Config`, like `login`. No stored credential: `Ok(())`.
- Environment and `models.jsonc` keys are untouched; `credential_source` reports them afterwards as before.

## U. Unknown prices (optional)

**Problem.** `Cost` is plain `f64`s (`crates/axo-ai/src/model.rs:37-54`); a model with no feed price (for example a Codex account model missing from the catalog's openai rows) gets 0, which reads as free. OMP renders free / included / unknown separately; Zed keeps cost optional ([`../research/axo-614089b/harness-model-catalog.md`](../research/axo-614089b/harness-model-catalog.md) C7).

**Request.** Any representation that lets a host tell "price unknown" from "price 0" on a model and on a usage cost. Shape is axo's call. Axome does not schedule a consumer yet ([P27](../design.md#decisions)).

## When it lands

From `/repo/rust/axo`:

```
git fetch origin && git status -sb
rg -n "fn subscribe|fn logout" crates/axo-ai/src/models/mod.rs
```

Axome bumps the three axo revs in `Cargo.toml:28-30`, wires S into `ModelsChanged`, and routes provider logout through L. If axo names them differently, Axome uses axo's names.
