---
id: detailed-agent-workflow-docs-and-skills-landed
title: Detailed agent workflow docs and skills landed
category: progress
section: agents/docs
status: active
tags:
- agents
- docs
created_at: 2026-03-10T05:58:13.028996147Z
updated_at: 2026-03-11T02:43:39.462838715Z
---
Completed
- Expanded the canonical Codex and Claude guidance templates in `src/agents.rs` so generated AGENTS, skill bundles, and Claude commands explain the full read, narrow, implement, and sync workflow.
- Added a long-form repo guide in `docs/agent-workflow.md` covering command patterns, category/status selection, TUI usage, and maintenance steps.
- Regenerated the checked-in agent assets, including `AGENTS.md`, `CLAUDE.md`, the tracked `.codex/skills/cotext-context/...` bundle, and the `.claude/...` guidance files.
- Standardized the project-local Codex skill bundle under `.codex/skills/cotext-context/`.

Evidence
- `cargo run -- agent install all --overwrite` refreshed the generated files.
- `cargo test` passes, including `src/agents.rs` guidance tests.
- `cargo fmt --check` passes.

Next step
- Keep the generated guidance aligned with future CLI, TUI, and storage-model changes so agent instructions do not drift from the real tool behavior.

Completed
- Tightened the generated AGENTS, CLAUDE, skill, and sync guidance so appending or refreshing the managed cotext block in a target repository's `AGENTS.md` or `CLAUDE.md` explicitly requires a cotext sync before handoff.

Evidence
- `cargo run -- agent install all --overwrite` refreshed `AGENTS.md`, `CLAUDE.md`, `.codex/skills/cotext-context/SKILL.md`, `.claude/skills/cotext-context/SKILL.md`, and `.claude/commands/cotext-sync.md`.
- `cargo test` passes with guidance assertions covering the new sync reminder.
- `cargo fmt --check` passes.
