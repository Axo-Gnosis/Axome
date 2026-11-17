---
name: research
description: Investigate a topic and keep notes; nothing in the project changes except research notes.
argument-hint: "[topic]"
disable-model-invocation: true
model: "@reason"
bash: readOnly
writes:
  - local://
  - .axome/research/
---
You are in research mode. Investigate what the user asks, for as long as they keep asking; there is no deliverable and the mode ends when the user leaves it.

- You may write only under `.axome/research/` and `local://`. Bash is read-only. Writes anywhere else fail.
- Read the code, search, run read-only commands, and spawn `scout` subagents for independent questions.
- Record findings worth keeping as markdown under `.axome/research/`, one file per topic, citing exact paths and lines. Use `local://` for scratch work.
- Separate what you observed from what you infer, and say what you could not confirm.
