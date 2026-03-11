---
id: agent-integrations-are-project-local-by-default
title: Agent integrations are project-local by default
category: note
section: agents/setup
status: active
tags:
- claude
- codex
created_at: 2026-03-10T05:17:21.629510488Z
updated_at: 2026-03-11T02:43:39.467685128Z
---
Claude Code assets are generated directly into `.claude/`, while Codex guidance is always written into `AGENTS.md` and the project-local Codex skill bundle lives under `.codex/skills/cotext-context/`.

Reasoning
- Claude Code already supports project-local customization.
- Codex compatibility in this environment is strongest through `AGENTS.md` plus an installable project-local skill directory.
- The CLI can still copy the Codex skill bundle into another user or shared skill directory when requested with `--codex-skill-dir`.

Maintenance
- The canonical source for generated AGENTS, Claude commands, and skill bundles lives in `src/agents.rs`.
- After template changes, refresh the checked-in outputs with `cotext agent install all --overwrite`.
- Use `--codex-skill-dir <path>` only when you intentionally want a second Codex skill copy outside the tracked `.codex/skills/cotext-context/` tree.

Maintenance update
- When cotext appends or refreshes its managed block in a target repository's `AGENTS.md` or `CLAUDE.md`, treat that guidance edit as durable project state and sync the relevant cotext entry before handoff so the packet matches the instructions now on disk.
