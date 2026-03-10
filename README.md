# cotext

`cotext` is a standalone Rust CLI and TUI for keeping project tracking context in one structured place instead of scattering raw markdown across ad hoc files.

It manages five context categories:

- `design`
- `note`
- `progress`
- `todo`
- `deferred`

The tool is built for both humans and code agents. It can:

- create and update structured context entries
- render a concatenated packet for humans, generic agents, Codex, or Claude Code
- generate agent-facing scaffolding for Codex and Claude Code
- open a single-page terminal UI for browsing, editing, previewing, and copying structured packets
- document a repeatable read, narrow, implement, and sync workflow for code agents

## Why

Large coding projects usually end up with:

- design docs in one file
- progress notes in another
- TODOs in a third place
- deferred work hidden in issue comments or scratch notes

`cotext` gives all of that a uniform storage model under `.cotext/`, while still keeping the data plain-text and git-friendly.

## Storage Model

Each project initialized with `cotext` gets a `.cotext/` folder:

```text
.cotext/
  cotext.toml
  entries/
    design/
    note/
    progress/
    todo/
    deferred/
```

Every entry is a markdown file with YAML front matter:

```md
---
id: tui-copy-loop
title: TUI clipboard review loop
category: design
section: tui/review
status: active
tags:
  - tui
  - clipboard
created_at: 2026-03-10T05:00:00Z
updated_at: 2026-03-10T05:00:00Z
---
Explain the decision here.
```

That keeps the storage readable while allowing the CLI and TUI to filter, group, and render it consistently.

## Quick Start

```bash
cargo run -- init . --name my-project --with-agents
cargo run -- new design "Context packet format" --section render
cargo run -- new todo "Add export command" --section cli --tag roadmap
cargo run -- list
cargo run -- render --audience codex
cargo run -- tui
```

## CLI Surface

```text
cotext init [path]
cotext new <category> <title>
cotext update <id>
cotext list
cotext show <id>
cotext render [--audience human|agent|codex|claude]
cotext agent install <codex|claude|all>
cotext tui
```

Useful patterns:

- `cotext render --audience codex`
  Use when a Codex session should read the current project packet.
- `cotext render --category todo --category deferred`
  Use when you only want next work and postponed work.
- `cotext update <id> --append "..."`
  Use for lightweight progress updates.
- `cotext agent install all --overwrite`
  Refresh the generated project-local agent assets.

## Agent Integration

### Codex

`cotext agent install codex` writes:

- `AGENTS.md` guidance that tells Codex to read and update context through `cotext`
- `.cotext/agents/codex/cotext-context/SKILL.md`
- `.cotext/agents/codex/cotext-context/agents/openai.yaml`

If you also pass `--codex-skill-dir ~/.codex/skills/cotext-context`, the same skill bundle is installed into the live Codex skill directory.

### Claude Code

`cotext agent install claude` writes:

- `CLAUDE.md` guidance for the repository
- `.claude/skills/cotext-context/SKILL.md`
- `.claude/commands/cotext.md`
- `.claude/commands/cotext-sync.md`

This follows Claude Code's project-local customization model so a repository can ship its own context workflow.

## Agent Workflow

`cotext` is designed to be read at the start of a substantial agent task and synced at the end if durable project context changed.

A good default loop is:

1. Load the packet with `cotext render --audience codex` or `cotext render --audience claude`.
2. Narrow the view with `cotext list --category ...`, `cotext render --category ...`, or `cotext show <id>` when the task is scoped.
3. Do the implementation or analysis.
4. Write back durable design, note, progress, todo, or deferred changes with `cotext update`, `cotext new`, or `cotext tui`.

Useful patterns:

- `cotext list --category todo --status active --status planned`
  Surface the actionable queue.
- `cotext render --category progress --category todo --audience codex`
  Load a focused packet when resuming active work.
- `cotext update <id> --append "Validation: cargo test"`
  Add short evidence to an existing progress entry.
- `cotext update <id> --status done`
  Close a completed todo.
- `cotext new note "Important environment caveat" --section env/setup --tag ops`
  Capture a new durable warning or operating fact.

Category guide:

- `design`: architecture, invariants, tradeoffs, durable decisions
- `note`: warnings, caveats, environment facts, operator reminders
- `progress`: landed work, evidence, next step
- `todo`: the next concrete task
- `deferred`: intentionally postponed future work

For the detailed workflow, maintenance notes, and regeneration steps for the checked-in agent guidance, see [docs/agent-workflow.md](docs/agent-workflow.md).

## TUI

`cotext tui` opens a single-page terminal board with:

- category cards across the top
- a filtered entry list on the left
- detail and preview/editor panes on the right
- staged quick-create prompts for title, section, and tags before the first file write
- packet preview modes for the current entry, current category, open category work, and open project work
- clipboard export for the selected entry or the whole current category using the current preview audience

Key bindings:

- `Tab` / `Shift-Tab`: switch category
- `j` / `k`: move selection
- `n`: create a new entry in the current category
- `e`: edit the selected body
- `t`: edit the selected title
- `s`: edit the selected section
- `g`: edit the selected tags
- `p`: cycle preview mode
- `a`: cycle preview audience
- `PageUp` / `PageDown`: scroll the preview pane
- `Ctrl-s`: save changes
- `Esc`: cancel editing
- `S`: cycle status
- `c`: copy the selected entry packet
- `C`: copy the current category packet
- `q`: quit

## Current Scope

The first implementation is intentionally simple:

- entries are markdown files with YAML front matter
- section filtering is prefix-based
- the TUI edits one selected item at a time, but now covers body, title, section, and tags
- new entries collect metadata before the initial file write so ids and paths start correct
- packet preview can switch between entry, category, and open-work views without leaving the TUI
- clipboard export is text-based and audience-aware, aimed at quick paste into an agent session

That is enough to consolidate design notes, awareness notes, progress, next work, and deferred work without introducing a database or web app.
