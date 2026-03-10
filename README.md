# cotext

`cotext` is a Rust CLI and TUI for keeping project context in one structured, git-friendly place instead of scattering design notes, progress logs, TODOs, and caveats across unrelated markdown files.

This project is built jointly by humans and OpenAI Codex. Treat AI-generated changes like any other contribution: review them, test them, and keep durable project context in sync.

`cotext` manages five entry categories:

- `design`
- `note`
- `progress`
- `todo`
- `deferred`

## Install

```bash
cargo install cotext
```

For local development from a checkout:

```bash
cargo install --path .
```

## Quick Start

```bash
cotext init . --name my-project --with-agents
cotext new design "Context packet format" --section render
cotext new todo "Add export command" --section cli --tag roadmap
cotext list
cotext render --audience codex
cotext tui
```

Each project gets a plain-text `.cotext/` directory:

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

## Core Commands

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

High-signal patterns:

- `cotext render --audience codex`
- `cotext render --category progress --category todo --audience codex`
- `cotext list --category todo --status active --status planned`
- `cotext update <id> --append "Validation: cargo test"`
- `cotext agent install all --overwrite`

## Agent Integration

`cotext agent install codex` writes:

- `AGENTS.md`
- `.codex/skills/cotext-context/SKILL.md`
- `.codex/skills/cotext-context/agents/openai.yaml`

`cotext agent install claude` writes:

- `CLAUDE.md`
- `.claude/skills/cotext-context/SKILL.md`
- `.claude/commands/cotext.md`
- `.claude/commands/cotext-sync.md`

Use `--codex-skill-dir <path>` only when you intentionally want a second Codex skill copy outside the tracked `.codex/skills/cotext-context/` tree.

For the full agent workflow and maintenance notes, see [docs/agent-workflow.md](docs/agent-workflow.md).

## TUI

`cotext tui` provides a single-screen terminal board for browsing, editing, previewing, and copying entries and packets.

Key bindings:

- `Tab` / `Shift-Tab`: switch category
- `j` / `k`: move selection
- `n` / `+`: add entry
- `d` / `Delete`: delete entry with confirmation
- `e` / `t` / `s` / `g`: edit body, title, section, and tags
- `p` / `a`: change preview mode and audience
- `?` in browse mode, or `F1` anywhere: toggle help
- `PageUp` / `PageDown`: scroll preview
- `Ctrl-s`: save
- `Esc`: cancel
- `c` / `C`: copy selected or category packet
- `q`: quit

## Development

Local and CI checks use the same quality gate:

- `cargo fmt --all -- --check`
- `cargo check --all-targets --all-features`
- `cargo clippy --all-targets --all-features -- -D warnings`
- `cargo test`

Install hooks with:

```bash
pre-commit install
```

Run them on demand with:

```bash
pre-commit run --all-files
```

Crates.io publishing is handled by GitHub Actions. The publish workflow expects a `CARGO_REGISTRY_TOKEN` repository secret and publishes from a matching release tag such as `v0.1.0`.
