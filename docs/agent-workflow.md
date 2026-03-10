# Agent Workflow

This document explains how code agents should use `cotext` in practice and how the `cotext` repository maintains its generated agent guidance.

`cotext` is meant to be the canonical context layer for a repository. A good agent loop is:

1. Load the packet.
2. Narrow the view if the task is scoped.
3. Perform the implementation or analysis.
4. Sync any durable design, note, progress, todo, or deferred changes back into `cotext`.

## Core Commands

Use the live CLI instead of guessing the interface.

```bash
cotext render --audience codex
cotext render --audience claude
cotext list --category todo
cotext list --category deferred
cotext show <id>
cotext update <id> --append "Validation: cargo test"
cotext update <id> --status done
cotext new design "Canonical agent guidance templates" --section agents/docs --tag agents --tag docs
cotext tui
```

## Session Startup

For substantial work, agents should begin with the full packet:

```bash
cotext render --audience codex
```

or:

```bash
cotext render --audience claude
```

Then narrow if needed:

- Resume active work:

  ```bash
  cotext list --category todo
  cotext list --category deferred
  cotext render --category progress --category todo --audience codex
  ```

- Inspect one category only:

  ```bash
  cotext list --category progress
  cotext render --category progress --audience codex
  ```

- Inspect one item only:

  ```bash
  cotext show <id>
  ```

## How To Choose the Right Entry Type

- `design`
  Use for architecture decisions, invariants, data-model choices, storage-model changes, and tradeoffs that should remain true after the current task is over.
- `note`
  Use for environment warnings, operational caveats, agent reminders, or facts that later implementers should keep in mind while making changes.
- `progress`
  Use for what already landed, how it was validated, what files/commands prove it, and what the next concrete step should be.
- `todo`
  Use for the next actionable task. A good todo describes the goal, what counts as done, and any blockers or dependencies.
- `deferred`
  Use for future work that is real but intentionally postponed.

## How To Choose the Right Status

- `draft`
  Early or incomplete design state.
- `active`
  In-force context or work currently being executed.
- `planned`
  Accepted next work that has not started yet.
- `blocked`
  Valid work waiting on another task, dependency, or decision.
- `done`
  Completed work retained for history.
- `deferred`
  Postponed work.
- `archived`
  Historical context no longer relevant to the active packet.

## Sync Rules

Use `cotext` as the write path when durable context changes.

- Update an existing entry when you are advancing or closing already tracked work.
- Create a new entry when the work introduced a new durable decision, warning, next step, or deferred item.
- Prefer `cotext update`, `cotext new`, or `cotext tui` over direct edits to `.cotext/entries/` unless you are repairing broken metadata or debugging `cotext` itself.
- Before final handoff, make sure the packet reflects the real state of the code.

Useful write patterns:

```bash
# Add short validation evidence
cotext update cli-render-pipeline-and-tui-mvp-landed --append "Validation: cargo test"

# Mark a todo complete
cotext update add-richer-metadata-editing-in-the-tui --status done

# Create a new note
cotext new note "Generated agent guidance comes from src/agents.rs" \
  --section agents/docs \
  --tag agents \
  --tag docs
```

## Good Writeback Quality

Good updates are:

- durable rather than conversational
- specific rather than vague
- supported by evidence such as commands, tests, or file paths
- clear about whether something is active, planned, done, or deferred

Poor updates usually:

- repeat the entire conversation instead of recording the result
- omit validation or evidence
- hide the next step
- leave `cotext` stale relative to the code

## TUI Usage

`cotext tui` is the fastest way to review or edit context on one screen.

Useful TUI capabilities:

- browse by category
- edit body, title, section, and tags
- stage new entries before the first write
- preview packets for the current entry, current category, open category work, and open project work
- switch preview audience and copy packets to the clipboard

Use the TUI when:

- a human wants to review context visually
- multiple metadata fields need edits
- you want to compare packet output while editing

## Generated Asset Map

`cotext agent install` generates agent-facing files from canonical templates in [`src/agents.rs`](../src/agents.rs).

For this repository, the checked-in outputs are:

- [AGENTS.md](../AGENTS.md)
- [CLAUDE.md](../CLAUDE.md)
- [.cotext/agents/codex/cotext-context/SKILL.md](../.cotext/agents/codex/cotext-context/SKILL.md)
- [.claude/skills/cotext-context/SKILL.md](../.claude/skills/cotext-context/SKILL.md)
- [.claude/commands/cotext.md](../.claude/commands/cotext.md)
- [.claude/commands/cotext-sync.md](../.claude/commands/cotext-sync.md)
- [.codex/skills/cotext-context/SKILL.md](../.codex/skills/cotext-context/SKILL.md)

## Maintaining Guidance in the `cotext` Repo

When changing agent guidance in this repository:

1. Edit the canonical templates in [`src/agents.rs`](../src/agents.rs).
2. Regenerate the checked-in outputs:

   ```bash
   cargo run -- agent install all --overwrite --codex-skill-dir ./.codex/skills/cotext-context
   ```

3. Run validation:

   ```bash
   cargo test
   cargo fmt --check
   ```

4. Sync any durable project-state changes back through `cotext`.

This keeps the source templates, the checked-in generated guidance, and the project packet aligned.
