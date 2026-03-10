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
updated_at: 2026-03-10T05:17:21.629510488Z
---
Claude Code assets are generated directly into `.claude/`, while Codex guidance is always written into `AGENTS.md` and a skill bundle is generated under the project tree.

Reasoning
- Claude Code already supports project-local customization.
- Codex compatibility in this environment is strongest through `AGENTS.md` and installable skill folders.
- The CLI can also copy the Codex skill bundle into a user skill directory when requested.
