# Axome

Axome is a Linux-first desktop workspace for coding agents in local git repositories: native chats built on axo and Axo-Extras, alongside external agent CLIs in terminals.

## The Axo Family

Shared by the AGENTS.md of axo, Axo-Extras, Axome, and Axophyte; change all four together.

- **axo**: the embeddable agent-harness library. It owns harness architecture and the public API every host builds on, designed for any host, never one.
- **Axo-Extras**: optional, opinionated capabilities core should not impose (subscription providers, local models, predictive text). Host-agnostic, built only on axo's public API to axo's standard; each host adopts, ignores, or replaces each crate.
- **Axome**: Toph's desktop host. It proves axo's surface and exposes its gaps; its product choices never define axo or Extras.
- **Axophyte**: Toph's personal persona, workflows, and extensions. It inspires ideas but has no bearing on the engineering of the others.

Dependencies point one way: Axophyte runs on hosts, Axome builds on axo and Extras, Extras builds on axo. A lower layer never names, special-cases, or assumes a higher one.

Where work belongs:

1. **axo**: every host needs it for correctness or efficiency, or hosts cannot build it themselves through the public surface (a missing seam, hook, typed datum, or operation).
2. **Axo-Extras**: a complete, reusable feature some hosts will want but core should not impose. When it needs something axo lacks, add the general seam to axo; never reach into internals.
3. **Axome**: Axome's product: its UI, settings, presentation, bundled modes and agents, and policy choices.
4. **Axophyte**: Toph's own workflow: persona text, prompts, personal workflows such as /cleanup and /decomplect, and personal tools.

"Axome needs X" or "Axophyte does X" is evidence of a need, not the design. Shape the change for every consumer of the layer it lands in, then adopt it upward.

Axophyte ("Axo") may also be the persona you are running as. Its instructions govern how you work, not these repositories' product or architecture.

### What this means in Axome

- Use axo and Extras only through their public APIs. Needing internals, a fork, or a copy of harness logic means an upstream gap: fix it there, host-agnostic, then adopt it.
- Product decisions (UI, settings, bundled definitions, policy defaults) stay in Axome.
- Axophyte is user configuration under ~/.config/axome. Never bundle, depend on, or assume it; bundled defaults stand on their own.

## Stack

- Development environment: Nix-first
- Desktop runtime: Tauri v2
- Frontend: SvelteKit
- Backend/runtime: Rust
- Database: SQLite
- CSS framework: Tailwind CSS v4 (via `@tailwindcss/vite`)
- Package manager: `bun`

## Working Rules

- Nix is the source of truth for development environment and packaging.
- Prefer `nix develop` and `nix build` over ad hoc host setup.
- Use `bun`, not `pnpm` or `npm`.
- Use Bun through the Nix environment; manage Bun dependency reproducibility with `bun2nix`.
- Keep privileged operations in Rust, not in the frontend.
- Do not add `tauri-plugin-shell` for app runtime process spawning.
- Use the system `git` CLI rather than `git2` unless the repo explicitly changes direction.
- Keep the frontend as a SvelteKit SPA with SSR disabled.
- Match the existing module split:
  - Svelte routes/components/stores in `src/`
  - Tauri commands in `src-tauri/src/commands/`
  - desktop logic in `src-tauri/src/` modules (`router/`, `windows/`); business logic in `src-crates/axome-core`
  - serialized models in `src-crates/axome-protocol`
- Frontend rules (Svelte 5, SvelteKit, UI component wrappers, Tailwind styling) live in `src/AGENTS.md`; read it before writing or editing anything under `src/`.
- Keep sessions folder-scoped; tabs drive the workbench, and a folder is open exactly while it has a tab.

## Living Documentation

Plans and the user-facing reference live in dedicated files. Do not duplicate them here, and never let them drift:

- **`README.md`**: user-facing reference for implemented features, configuration (`.axome/settings.jsonc`, `.axome/tasks.jsonc`), and packaging. Update it with every user-visible behavior or config change.
- **`docs/plan/design.md`**: goals, constraints, and settled decisions for Axome's axo integration. Record each decision as it is made; otherwise change it only when a goal changes.
- **`docs/plan/plan.md`**: the live adoption plan: topics, open questions (stable IDs `Q1`…), the mechanical migration checklist, and remaining axo gaps. Update as topics progress or settle.
- **`docs/plan/research/`**: axo surveys kept as evidence, each pinned to an axo revision. When axo moves, add a new `axo-<rev>/` folder rather than editing an old survey.
- **`TODO.md`**: backlog outside the axo adoption.

## Validation

Run Rust checks inside `nix develop` (so `mold`, `sccache`, and `cargo-nextest` are active). Use the narrowest relevant checks that exist today:

- `bun run check`
- `bun run build`
- `cargo check --workspace --all-targets`
- `cargo nextest run -p axome-core` or `cargo nextest run -p axome-server` for backend changes (avoids linking `src-tauri`, which `default-members` selects for bare `cargo test`; pass `--no-pager` to `cargo nextest list` in non-interactive shells)
- `cargo nextest run --workspace` (or `cargo test --workspace`; `nextest` skips doctests)
- `bun run app:build -- --debug`

Do not claim lint/format/test commands exist if they are not defined in the current repo.

## Important Notes

- Terminal/session lifecycle correctness matters more than adding surface features quickly.
- Multiple sessions in one folder share its working tree; be explicit in the UI when behavior is non-isolated.
