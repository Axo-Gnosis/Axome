# Axome

Axome is a Linux-first desktop app for running coding-agent CLIs inside a local git repository.


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
