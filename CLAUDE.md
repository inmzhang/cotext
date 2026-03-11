# CLAUDE.md

<!-- COTEXT:START -->
## Cotext Workflow

Use `cotext` as the canonical project context manager for `cotext`.

### Startup

- Refresh context with `cotext render --audience claude` before coding.
- If the task is about resuming work, next work, or deferred work, inspect `cotext list --category todo` and `cotext list --category deferred`.
- Narrow the read with `cotext render --category <category> --audience claude` or `cotext show <id>` when only one slice matters.

### Sync Rules

- Use `.claude/commands/cotext.md` to load the authoritative packet inside Claude Code.
- Use `.claude/commands/cotext-sync.md` after meaningful work to sync design, progress, note, todo, or deferred changes.
- If you append or refresh the managed cotext block in the target project's `AGENTS.md` or `CLAUDE.md`, record that guidance change in cotext before handoff.
- Prefer `cotext update` and `cotext new` over manual edits to `.cotext/entries/` unless the tool itself is the thing being repaired.

### Generated Assets

- Project-local skill instructions live under `.claude/skills/cotext-context/`.
- Refresh Claude guidance with `cotext agent install claude --overwrite`.

<!-- COTEXT:END -->

