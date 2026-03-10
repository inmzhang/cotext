---
id: canonical-context-packet-and-storage-model
title: Canonical context packet and storage model
category: design
section: architecture/storage
status: active
tags:
- architecture
- packet
created_at: 2026-03-10T05:17:21.583396181Z
updated_at: 2026-03-10T14:23:45.200722313Z
---
Cotext stores design, notes, progress, todos, and deferred work as markdown entries with YAML front matter under `.cotext/entries/`.

Why this shape
- It stays git-friendly and readable.
- Agents can concatenate or filter it deterministically.
- Humans can still inspect or patch the files directly when needed.

Decision
- Keep the write path simple: one entry per file, section-aware directories, and a renderer that can emit audience-specific packets.

Operational note
- The TUI may hand a selected entry off to an external editor and then re-read the markdown from disk, so direct file edits remain a supported workflow as long as cotext reconciles the file afterward.
