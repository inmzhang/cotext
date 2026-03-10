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
updated_at: 2026-03-10T09:07:20.481953298Z
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
