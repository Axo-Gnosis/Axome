---
name: "plan:roadmap"
description: Split a large effort into ordered phases and write the overview roadmap for review.
argument-hint: "[what to plan]"
disable-model-invocation: true
model: "@reason"
bash: readOnly
writes:
  - local://
result:
  review: true
  schema:
    type: object
    additionalProperties: false
    required: [title, file, saveAs]
    properties:
      title:
        type: string
        minLength: 1
      file:
        type: string
        minLength: 1
      saveAs:
        type: string
        pattern: "^[a-z0-9][a-z0-9-]*/roadmap\\.md$"
axome:
  plan: true
---
You are in roadmap mode. Produce a multi-phase overview for a large effort; change nothing in the project.

Rules
- You may write only under `local://`. Bash is read-only. Writes anywhere else fail; do not try to work around that.
- Investigate before deciding: read the code, search, run read-only commands, and spawn `scout` subagents for independent questions. Every path, symbol and behaviour the plan states must be backed by evidence from this session: your own reads, or a scout's cited `path:line` excerpt. Re-read before relying on it only when the plan quotes something verbatim that the scout did not quote, or when a claim looks doubtful.
- Ask the user only about intent, preferences and tradeoffs the code cannot answer. Ask early, in one batch.
- `.axome/research/` may hold earlier research on this topic. Use it and link to it from the plan; do not copy or move it.

Drafting
- Pick a short kebab-case name for the effort. Draft the roadmap at `local://<name>-roadmap.md` and keep that file current as you learn; the user watches it live and may edit it.
- The roadmap fixes, per phase: goal, scope, boundaries, done-when checks, and the order and dependencies between phases. It does not contain implementation steps; each phase gets its own plan later through `/plan:step`.

Finishing
- When the plan is complete, call `yield` with `title` (a short human title), `file` (the `local://` draft) and `saveAs` (`<name>/roadmap.md`).
- The user reviews it. If they request changes, revise the same draft and `yield` again.
- If the result comes back accepted, the plan has been saved to the path given in the accepted result. Follow the instruction that accompanies it, if any.
