# AGENTS.md

<!-- COTEXT:START -->
## Cotext Workflow

Use `cotext` as the canonical project context manager for `cotext`.

### Startup

- Read the current packet with `cotext render --audience codex` before substantial work.
- If the task is about "next", "continue", or resuming active work, also inspect `cotext list --category todo` and `cotext list --category deferred`.
- If the task is scoped, narrow with `cotext list --category <category>`, `cotext render --category <category> --audience codex`, or `cotext show <id>`.

### Sync Rules

- Write back meaningful design, progress, note, todo, or deferred changes through `cotext new`, `cotext update`, or `cotext tui`.
- Prefer `cotext` commands over hand-editing `.cotext/entries/` unless you are repairing broken metadata or debugging cotext itself.
- Use `cotext update <id> --append ...` for incremental progress and `cotext update <id> --status done` when closing tracked work.
- If the work introduced a new durable decision or follow-up item, create a new entry instead of overloading an unrelated one.

### Category Guide

- `design`: architecture decisions, invariants, tradeoffs, or storage-model changes.
- `note`: warnings, operational caveats, and facts later agents should stay aware of.
- `progress`: shipped implementation state, evidence, validation, and the next step.
- `todo`: the next actionable task with a concrete goal and acceptance criteria.
- `deferred`: future work that is real but intentionally postponed.

### Generated Assets

- The project-local Codex skill scaffold lives under `.codex/skills/cotext-context/`.
- Refresh Codex guidance with `cotext agent install codex --overwrite`.
- Use `--codex-skill-dir <path>` only when you also need a second copy in another Codex skill directory.

<!-- COTEXT:END -->

