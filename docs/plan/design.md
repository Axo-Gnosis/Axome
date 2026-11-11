# Axome agent design: goals & settled decisions

This document holds the goals of Axome's agent integration and every decision we have settled. It changes only when a goal changes or a decision is made; open questions live in [`plan.md`](plan.md).

- **Active discussion:** [`plan.md`](plan.md)
- **Evidence:** [`research/`](research/README.md) (axo surveys)
- **Project rules:** [`.omp/AGENTS.md`](../../.omp/AGENTS.md) (stack, module split, Svelte and styling rules)

## Context

Axome embeds axo (`/repo/rust/axo`) as a Rust library: the agent loop, sessions, tools, jobs, child sessions, personas and modes all live there. Axome owns everything a person sees and decides: storage locations, configuration, policy, and the whole interface.

| | Revision |
|---|---|
| Axome pin | `26066e2` |
| Adoption target | `843cd73` (last 4 commits not yet on origin) |

## Goals

1. **Adopt axo fully, as a clean cutover.** Toph owns every consumer; no shims, aliases or compatibility layers.
2. **Build Axome's own experience on axo's API, not a mirror of axo's model-facing interface.** The model sees background jobs as `proc://` text and tool results; the user gets a real surface to see what runs, follow it, and manage it.
3. **Make subagents and teams legible and workable.** The user can see what was spawned, by whom, what each child is doing, and act on it.
4. **Treat personas, modes and model roles as host-designed experiences.** axo supplies the mechanisms; Axome decides which ones exist and how they feel (for example a `/plan` flow).
5. **Lifecycle correctness before surface features.** Sessions now own running commands and children; nothing in Axome may silently kill or orphan them.

## Constraints

- Privileged work stays in Rust; the frontend talks through typed invoke and channel wrappers in `src/lib`. Business logic in `src-crates/axome-core`, serialized models in `src-crates/axome-protocol`.
- axo stays host-agnostic. Anything Axome-specific (approval prompts, UI state, Axome's own registries) lives in Axome; capabilities every host needs are requested from axo.

## Decisions

Recorded as they are made. Each row links the topic in [`plan.md`](plan.md) that produced it.

| # | Date | Decision | Topic |
|---|---|---|---|
| | | *None yet.* | |
