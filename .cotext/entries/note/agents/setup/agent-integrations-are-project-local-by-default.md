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
updated_at: 2026-03-10T05:58:19.762925985Z
---
Claude Code assets are generated directly into `.claude/`, while Codex guidance is always written into `AGENTS.md` and a skill bundle is generated under the project tree.

Reasoning
- Claude Code already supports project-local customization.
- Codex compatibility in this environment is strongest through `AGENTS.md` and installable skill folders.
- The CLI can also copy the Codex skill bundle into a user skill directory when requested.

Maintenance
- The canonical source for generated AGENTS, Claude commands, and skill bundles lives in `src/agents.rs`.
- After template changes, refresh the checked-in outputs with `cotext agent install all --overwrite --codex-skill-dir ./.codex/skills/cotext-context` so the generated files stay in sync.
