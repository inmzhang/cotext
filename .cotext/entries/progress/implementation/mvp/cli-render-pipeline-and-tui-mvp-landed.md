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
updated_at: 2026-03-10T05:47:47.543192064Z
---
The initial implementation now supports project initialization, structured entry creation, list/show/render flows, agent asset generation, and a ratatui single-page editor.

Completed
- The TUI now edits section and tags directly alongside title and body.
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
