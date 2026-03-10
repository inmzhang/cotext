---
id: cli-render-pipeline-and-tui-mvp-landed
title: CLI, render pipeline, and TUI MVP landed
category: progress
section: implementation/mvp
status: active
tags:
- cli
- tui
created_at: 2026-03-10T05:17:21.625518189Z
updated_at: 2026-03-10T14:14:09.689971182Z
---
The initial implementation now supports project initialization, structured entry creation, list/show/render flows, agent asset generation, and a ratatui single-page context board.

Completed
- The TUI edits title, section, and tags inline, while body/front-matter editing can jump into the external editor.
- New entries stage title, section, and tags before the initial file write, so ids and paths start correct.
- Browse mode can preview the current entry, the full current category packet, the open current-category packet, or the open project packet.
- Preview scope now includes audience switching and scroll state so larger packets remain usable inside the same screen.

Validation
- `cargo test` passes.
- `cotext init --with-agents` works on this repository.
- The generated agent assets and `.cotext` layout are present in-tree.
- Added TUI state tests covering section edits, tag replacement, quick-create metadata capture, and open-only preview filtering.

Next step
- Add in-TUI filtering and search so larger projects do not rely only on category tabs and packet previews.

Completed
- Added storage-backed entry deletion plus empty-section pruning so the TUI can remove entries cleanly.
- Added modal help and delete-confirmation overlays, plus a `+` shortcut alongside `n` for add-entry flow.
- Restyled the TUI with a stronger palette, live stats header, richer category cards, denser entry metadata rows, and a command footer.

Validation
- `cargo fmt` passes.
- `cargo test` passes.
- Added TUI state tests for the `+` add-entry shortcut, delete confirmation, and help overlay toggling.

Completed
- Browse mode now uses `Enter` or `e` to suspend the TUI and open the selected entry in the external editor, then reconciles the edited file back through cotext so `updated_at` and path-sensitive front matter stay consistent.
- New entries now go from title/section/tags staging straight into the external editor instead of the inline body editor.
- Updated the README and `docs/agent-workflow.md` to document the external-editor flow.

Validation
- `cargo test` passes.
- Added TUI state coverage for the Enter shortcut queueing the external editor action and storage coverage for reconciling externally edited files.
