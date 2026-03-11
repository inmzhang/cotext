---
name: cotext-context
description: Read and update structured project context for cotext with cotext. Use when you need design notes, awareness notes, progress, next todos, deferred work, or a reliable sync workflow for Claude Code.
---

# Cotext Context

## Default operating sequence

1. Run `cotext render --audience claude` to load the current project packet.
2. If the task is about continuing work or finding the next item, inspect `cotext list --category todo` and `cotext list --category deferred`.
3. If the task is scoped, narrow the context with `cotext render --category <category> --audience claude`, `cotext list --category <category>`, or `cotext show <id>`.
4. Perform the implementation or analysis.
5. After meaningful work, sync durable changes with `cotext update` or `cotext new`.
6. Use `cotext tui` when a human wants to review and edit context on a single page.

## What belongs in each category

- `design`: architecture decisions, invariants, tradeoffs.
- `note`: warnings, operating assumptions, facts that should stay top-of-mind.
- `progress`: completed implementation state, validation, next step.
- `todo`: the next concrete task.
- `deferred`: real but postponed work.

## Update guidance

- Use `cotext update <id> --append ...` for incremental progress or validation evidence.
- Use `cotext update <id> --status done` when closing work.
- Use `cotext new <category> <title> ...` for newly discovered durable context.
- If you changed the target repo's `AGENTS.md`, `CLAUDE.md`, or other generated agent guidance, sync cotext before handoff so the packet matches the instructions now on disk.
- Prefer `cotext` commands over direct edits to `.cotext/entries/` unless you are repairing the tool or broken metadata.

## Command cookbook

```bash
cotext render --audience claude
cotext list --category todo --status active --status planned
cotext show <id>
cotext update <id> --append "Validation: ..."
cotext new note "Important environment caveat" --section env/setup --tag ops
cotext tui
```

## Generated guidance

- Project-local Claude guidance lives under `.claude/skills/cotext-context/` and `.claude/commands/`.
- Refresh it with `cotext agent install claude --overwrite`.
