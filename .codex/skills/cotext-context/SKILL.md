---
name: cotext-context
description: Read and update structured project context for cotext with cotext. Use when you need the latest design notes, awareness notes, progress, next todos, or deferred implementation items.
---

# Cotext Context

## When to use

Use this skill when the user asks for prior design, notes to keep in mind, progress state, current todos, or deferred work, or when you are about to change the code and need the canonical project packet.

## Workflow

1. Read the current packet with `cotext render --audience codex`.
2. Narrow the view with `cotext list --category <category>` or `cotext render --category <category>` when only one slice matters.
3. After meaningful work, write back updates with `cotext new` or `cotext update`.
4. When a human wants a single visual review/edit surface, suggest `cotext tui`.
