---
description: Sync meaningful project context changes back into cotext.
---

Compare the work you just completed against the current cotext packet for `cotext`.

Sync context with this checklist:

1. Update an existing entry with `cotext update <id> ...` if you advanced, clarified, or closed tracked work.
2. Use `cotext update <id> --append ...` for short evidence or validation notes.
3. Use `cotext update <id> --status done` when a todo is complete.
4. Create a new entry with `cotext new <category> <title> ...` when the work introduced a new durable design decision, warning, next step, or deferred item.
5. If you appended or refreshed the managed cotext block in the target repo's `AGENTS.md` or `CLAUDE.md`, record that guidance change in cotext before handoff.
6. Prefer `cotext` commands or `cotext tui` over direct edits to `.cotext/entries/` unless you are repairing cotext itself.
7. Re-render the relevant packet or list after syncing so the final state is confirmed before handoff.
