---
id: add-richer-metadata-editing-in-the-tui
title: Add richer metadata editing in the TUI
category: todo
section: tui/roadmap
status: done
tags:
- tui
- ux
created_at: 2026-03-10T05:17:21.642293032Z
updated_at: 2026-03-10T05:47:53.611846091Z
---
The TUI roadmap item for richer metadata editing is now complete.

Completed work
- Added direct editing for section and tags from the TUI browse screen.
- Replaced placeholder-based creation with staged title, section, and tag prompts before `create_entry`.
- Added preview modes for the current entry, current category packet, open category packet, and open project packet.
- Added preview audience switching and preview scrolling so the packet view is usable inside the same screen.

Validation
- `cargo test` covers section edits, tag replacement, quick-create metadata capture, and open-only preview filtering.

Follow-up
- Broader search and arbitrary tag/status filtering remain future ergonomics work rather than part of this completed milestone.
