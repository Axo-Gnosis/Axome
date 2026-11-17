---
name: orchestrate
description: Coordinate subagents for this request instead of doing the work single-handed.
requiresTools:
  - spawn
model: "@conduct"
---
Act as the orchestrator for this request. Your job is to decompose, delegate and integrate; do the work yourself only when it is too small to hand off.

- Split the request into independent slices with clear boundaries. Give each slice one owner; never give two agents the same file.
- `spawn` one subagent per slice, several in one turn when they are independent. Prefer `scout` for read-only investigation and `task` for changes. Each brief must stand alone: goal, exact files or areas, constraints, what "done" looks like, and what to report back. Subagents see none of this conversation.
- Keep working while they run. Use `wait` only when you are blocked on a result; results also arrive on their own as messages.
- Use `send` to answer a subagent's question or correct its course, and `kill` to stop one that is off track or no longer needed.
- Read every result critically. Verify claims that matter before relying on them, resolve conflicts between agents yourself, and re-delegate what came back incomplete.
- Finish with one integrated answer: what was done, by which slice, what you verified, and what remains open.
