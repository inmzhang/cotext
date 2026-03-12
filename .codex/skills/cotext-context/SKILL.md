---
name: cotext-context
description: Read and update structured project context for cotext with cotext. Use when you need the current design, awareness notes, progress, active todos, deferred work, or a reliable workflow for syncing those back after implementation.
---

# Cotext Context

## Goal

Use `cotext` as the canonical context layer for `cotext`. The normal loop is:

1. Load the packet.
2. Narrow it if the task is scoped.
3. Perform the implementation or analysis.
4. Sync back any durable design, note, progress, todo, or deferred changes before handoff.

## Default operating sequence

1. Start with `cotext render --audience codex`.
2. Commands prefer global cotext storage by default and fall back to repo-local storage when no matching global project exists; use `--storage local` to force repo-local management.
3. If the task is about resuming work, "continue", or finding the next item, inspect `cotext list --category todo` and `cotext list --category deferred`.
4. If the task is about one slice of context, narrow with `cotext list --category <category>`, `cotext render --category <category> --audience codex`, or `cotext show <id>`.
5. Do the work.
6. If durable context changed, update the relevant entry with `cotext update` or create a new one with `cotext new`.
7. When a human wants one-screen review/editing, use `cotext tui`.

## Read patterns

- Full implementation packet: `cotext render --audience codex`
- Focused active-work packet: `cotext render --category progress --category todo --audience codex`
- Actionable queue: `cotext list --category todo --status active --status planned`
- Deferred queue: `cotext list --category deferred`
- Single-item inspection: `cotext show <id>`
- Machine-readable listing: `cotext list --format json --category todo`

## Update rules

- Prefer `cotext update <id> ...` when advancing or closing an existing tracked item.
- Use `cotext update <id> --append ...` for short progress/evidence additions.
- Use `cotext update <id> --status done` when a tracked task is complete.
- Use `cotext new <category> <title> ...` when the work introduced a new durable decision, warning, next step, or deferred item.
- If you changed the target repo's `AGENTS.md`, `CLAUDE.md`, or other generated agent guidance, sync cotext before handoff so the packet matches the instructions now on disk.
- Prefer `cotext update` / `cotext new` / `cotext tui` over direct edits to the managed cotext entry markdown on disk unless you are repairing broken metadata or debugging cotext.

## Category guide

- `design`: stable architecture decisions, invariants, tradeoffs, storage changes.
- `note`: warnings, environment quirks, operator guidance, facts later agents should remember.
- `progress`: what landed, how it was validated, and what should happen next.
- `todo`: the next actionable task with a concrete goal and acceptance criteria.
- `deferred`: real future work intentionally postponed.

## Status guide

- `draft`: rough or incomplete design state.
- `active`: currently in force or currently being worked.
- `planned`: accepted next work that has not started.
- `blocked`: valid work waiting on a dependency.
- `done`: completed work retained for history.
- `deferred`: postponed work.
- `archived`: kept for reference but no longer part of active context.

## Command cookbook

```bash
# Load the full packet before coding
cotext render --audience codex

# Resume the active frontier
cotext list --category todo
cotext list --category deferred
cotext render --category progress --category todo --audience codex

# Inspect and update an existing entry
cotext show cli-render-pipeline-and-tui-mvp-landed
cotext update cli-render-pipeline-and-tui-mvp-landed --append "Validation: cargo test"

# Close a completed todo
cotext update add-richer-metadata-editing-in-the-tui --status done

# Capture a new design decision
cotext new design "Canonical agent guidance templates" --section agents/docs --tag agents --tag docs

# Open the single-screen review surface
cotext tui
```

## Good writeback quality

- Record durable facts instead of chatty narration.
- Include concrete evidence such as commands, tests, file paths, or validation results.
- Keep titles stable and make the body rich enough that a later agent can resume without replaying the conversation.
- If code and cotext disagree, sync cotext before finishing.

## Refreshing generated guidance

- Project-local Codex guidance is generated from `cotext agent install codex`.
- The default tracked Codex skill target is `.codex/skills/cotext-context/`.
- Use `--codex-skill-dir <path>` only when you also need a second copy in another Codex skill directory.
