use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

use crate::storage::Project;

const START_MARKER: &str = "<!-- COTEXT:START -->";
const END_MARKER: &str = "<!-- COTEXT:END -->";

#[derive(Clone, Debug, Default)]
pub struct InstallReport {
    pub changed: Vec<PathBuf>,
    pub skipped: Vec<PathBuf>,
}

impl InstallReport {
    fn record_changed(&mut self, path: PathBuf) {
        self.changed.push(path);
    }

    fn record_skipped(&mut self, path: PathBuf) {
        self.skipped.push(path);
    }
}

pub fn install_codex(
    project: &Project,
    codex_skill_dir: Option<&Path>,
    overwrite: bool,
) -> Result<InstallReport> {
    let mut report = InstallReport::default();
    let agents_path = project.root.join("AGENTS.md");
    let agents_block = codex_agents_block(project);
    upsert_marked_markdown(
        &agents_path,
        "# AGENTS.md\n\n",
        &agents_block,
        overwrite,
        &mut report,
    )?;

    let project_skill_root = project
        .root
        .join(".cotext")
        .join("agents")
        .join("codex")
        .join("cotext-context");
    write_codex_skill_bundle(project, &project_skill_root, overwrite, &mut report)?;

    if let Some(skill_dir) = codex_skill_dir {
        write_codex_skill_bundle(project, skill_dir, overwrite, &mut report)?;
    }

    Ok(report)
}

pub fn install_claude(project: &Project, overwrite: bool) -> Result<InstallReport> {
    let mut report = InstallReport::default();
    let claude_path = project.root.join("CLAUDE.md");
    upsert_marked_markdown(
        &claude_path,
        "# CLAUDE.md\n\n",
        &claude_agents_block(project),
        overwrite,
        &mut report,
    )?;

    let skill_root = project
        .root
        .join(".claude")
        .join("skills")
        .join("cotext-context");
    write_file(
        &skill_root.join("SKILL.md"),
        &claude_skill_md(project),
        overwrite,
        &mut report,
    )?;

    let commands_root = project.root.join(".claude").join("commands");
    write_file(
        &commands_root.join("cotext.md"),
        &claude_context_command(project),
        overwrite,
        &mut report,
    )?;
    write_file(
        &commands_root.join("cotext-sync.md"),
        &claude_sync_command(project),
        overwrite,
        &mut report,
    )?;

    Ok(report)
}

fn upsert_marked_markdown(
    path: &Path,
    header: &str,
    block: &str,
    overwrite: bool,
    report: &mut InstallReport,
) -> Result<()> {
    let payload = format!("{START_MARKER}\n{block}\n{END_MARKER}\n");
    let next = if path.exists() {
        let current = fs::read_to_string(path)
            .with_context(|| format!("failed to read {}", path.display()))?;
        if let Some(start) = current.find(START_MARKER) {
            if let Some(end) = current.find(END_MARKER) {
                let end = end + END_MARKER.len();
                let mut updated = String::new();
                updated.push_str(&current[..start]);
                if !updated.ends_with('\n') {
                    updated.push('\n');
                }
                updated.push_str(&payload);
                if end < current.len() {
                    updated.push_str(current[end..].trim_start_matches('\n'));
                    updated.push('\n');
                }
                updated
            } else {
                format!("{current}\n{payload}")
            }
        } else if overwrite {
            format!("{current}\n{payload}")
        } else {
            format!("{current}\n{payload}")
        }
    } else {
        format!("{header}{payload}")
    };

    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }
    fs::write(path, next).with_context(|| format!("failed to write {}", path.display()))?;
    report.record_changed(path.to_path_buf());
    Ok(())
}

fn write_codex_skill_bundle(
    project: &Project,
    root: &Path,
    overwrite: bool,
    report: &mut InstallReport,
) -> Result<()> {
    write_file(
        &root.join("SKILL.md"),
        &codex_skill_md(project),
        overwrite,
        report,
    )?;
    write_file(
        &root.join("agents").join("openai.yaml"),
        CODEX_OPENAI_YAML,
        overwrite,
        report,
    )?;
    Ok(())
}

fn write_file(
    path: &Path,
    contents: &str,
    overwrite: bool,
    report: &mut InstallReport,
) -> Result<()> {
    if path.exists() && !overwrite {
        report.record_skipped(path.to_path_buf());
        return Ok(());
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }
    fs::write(path, contents).with_context(|| format!("failed to write {}", path.display()))?;
    report.record_changed(path.to_path_buf());
    Ok(())
}

fn codex_agents_block(project: &Project) -> String {
    let project_name = &project.config.name;
    format!(
        r#"## Cotext Workflow

Use `cotext` as the canonical project context manager for `{project_name}`.

### Startup

- Read the current packet with `cotext render --audience codex` before substantial work.
- If the task is about "next", "continue", or resuming active work, also inspect `cotext list --category todo` and `cotext list --category deferred`.
- If the task is scoped, narrow with `cotext list --category <category>`, `cotext render --category <category> --audience codex`, or `cotext show <id>`.

### Sync Rules

- Write back meaningful design, progress, note, todo, or deferred changes through `cotext new`, `cotext update`, or `cotext tui`.
- Prefer `cotext` commands over hand-editing `.cotext/entries/` unless you are repairing broken metadata or debugging cotext itself.
- Use `cotext update <id> --append ...` for incremental progress and `cotext update <id> --status done` when closing tracked work.
- If the work introduced a new durable decision or follow-up item, create a new entry instead of overloading an unrelated one.

### Category Guide

- `design`: architecture decisions, invariants, tradeoffs, or storage-model changes.
- `note`: warnings, operational caveats, and facts later agents should stay aware of.
- `progress`: shipped implementation state, evidence, validation, and the next step.
- `todo`: the next actionable task with a concrete goal and acceptance criteria.
- `deferred`: future work that is real but intentionally postponed.

### Generated Assets

- The project-local Codex skill scaffold lives under `.cotext/agents/codex/cotext-context/`.
- Refresh Codex guidance with `cotext agent install codex --overwrite`.
- Repositories that also track a `.codex/skills/cotext-context/` mirror can refresh both copies with `cotext agent install codex --codex-skill-dir ./.codex/skills/cotext-context --overwrite`.
"#
    )
}

fn claude_agents_block(project: &Project) -> String {
    let project_name = &project.config.name;
    format!(
        r#"## Cotext Workflow

Use `cotext` as the canonical project context manager for `{project_name}`.

### Startup

- Refresh context with `cotext render --audience claude` before coding.
- If the task is about resuming work, next work, or deferred work, inspect `cotext list --category todo` and `cotext list --category deferred`.
- Narrow the read with `cotext render --category <category> --audience claude` or `cotext show <id>` when only one slice matters.

### Sync Rules

- Use `.claude/commands/cotext.md` to load the authoritative packet inside Claude Code.
- Use `.claude/commands/cotext-sync.md` after meaningful work to sync design, progress, note, todo, or deferred changes.
- Prefer `cotext update` and `cotext new` over manual edits to `.cotext/entries/` unless the tool itself is the thing being repaired.

### Generated Assets

- Project-local skill instructions live under `.claude/skills/cotext-context/`.
- Refresh Claude guidance with `cotext agent install claude --overwrite`.
"#
    )
}

fn codex_skill_md(project: &Project) -> String {
    let project_name = &project.config.name;
    format!(
        r#"---
name: cotext-context
description: Read and update structured project context for {project_name} with cotext. Use when you need the current design, awareness notes, progress, active todos, deferred work, or a reliable workflow for syncing those back after implementation.
---

# Cotext Context

## Goal

Use `cotext` as the canonical context layer for `{project_name}`. The normal loop is:

1. Load the packet.
2. Narrow it if the task is scoped.
3. Perform the implementation or analysis.
4. Sync back any durable design, note, progress, todo, or deferred changes before handoff.

## Default operating sequence

1. Start with `cotext render --audience codex`.
2. If the task is about resuming work, "continue", or finding the next item, inspect `cotext list --category todo` and `cotext list --category deferred`.
3. If the task is about one slice of context, narrow with `cotext list --category <category>`, `cotext render --category <category> --audience codex`, or `cotext show <id>`.
4. Do the work.
5. If durable context changed, update the relevant entry with `cotext update` or create a new one with `cotext new`.
6. When a human wants one-screen review/editing, use `cotext tui`.

## Read patterns

- Full implementation packet: `cotext render --audience codex`
- Focused active-work packet: `cotext render --category progress --category todo --audience codex`
- Actionable queue: `cotext list --category todo --status active --status planned`
- Deferred queue: `cotext list --category deferred`
- Single-item inspection: `cotext show <id>`
- Machine-readable listing: `cotext list --format json --category todo`

## Update rules

- Prefer `cotext update <id> ...` when advancing or closing an existing tracked item.
- Use `cotext update <id> --append ...` for short progress/evidence additions.
- Use `cotext update <id> --status done` when a tracked task is complete.
- Use `cotext new <category> <title> ...` when the work introduced a new durable decision, warning, next step, or deferred item.
- Prefer `cotext update` / `cotext new` / `cotext tui` over direct edits to `.cotext/entries/` unless you are repairing broken metadata or debugging cotext.

## Category guide

- `design`: stable architecture decisions, invariants, tradeoffs, storage changes.
- `note`: warnings, environment quirks, operator guidance, facts later agents should remember.
- `progress`: what landed, how it was validated, and what should happen next.
- `todo`: the next actionable task with a concrete goal and acceptance criteria.
- `deferred`: real future work intentionally postponed.

## Status guide

- `draft`: rough or incomplete design state.
- `active`: currently in force or currently being worked.
- `planned`: accepted next work that has not started.
- `blocked`: valid work waiting on a dependency.
- `done`: completed work retained for history.
- `deferred`: postponed work.
- `archived`: kept for reference but no longer part of active context.

## Command cookbook

```bash
# Load the full packet before coding
cotext render --audience codex

# Resume the active frontier
cotext list --category todo
cotext list --category deferred
cotext render --category progress --category todo --audience codex

# Inspect and update an existing entry
cotext show cli-render-pipeline-and-tui-mvp-landed
cotext update cli-render-pipeline-and-tui-mvp-landed --append "Validation: cargo test"

# Close a completed todo
cotext update add-richer-metadata-editing-in-the-tui --status done

# Capture a new design decision
cotext new design "Canonical agent guidance templates" --section agents/docs --tag agents --tag docs

# Open the single-screen review surface
cotext tui
```

## Good writeback quality

- Record durable facts instead of chatty narration.
- Include concrete evidence such as commands, tests, file paths, or validation results.
- Keep titles stable and make the body rich enough that a later agent can resume without replaying the conversation.
- If code and cotext disagree, sync cotext before finishing.

## Refreshing generated guidance

- Project-local Codex guidance is generated from `cotext agent install codex`.
- Repositories that also track a `.codex/skills/cotext-context/` mirror can refresh both copies with `cotext agent install all --codex-skill-dir ./.codex/skills/cotext-context --overwrite`.
"#
    )
}

fn claude_skill_md(project: &Project) -> String {
    let project_name = &project.config.name;
    format!(
        r#"---
name: cotext-context
description: Read and update structured project context for {project_name} with cotext. Use when you need design notes, awareness notes, progress, next todos, deferred work, or a reliable sync workflow for Claude Code.
---

# Cotext Context

## Default operating sequence

1. Run `cotext render --audience claude` to load the current project packet.
2. If the task is about continuing work or finding the next item, inspect `cotext list --category todo` and `cotext list --category deferred`.
3. If the task is scoped, narrow the context with `cotext render --category <category> --audience claude`, `cotext list --category <category>`, or `cotext show <id>`.
4. Perform the implementation or analysis.
5. After meaningful work, sync durable changes with `cotext update` or `cotext new`.
6. Use `cotext tui` when a human wants to review and edit context on a single page.

## What belongs in each category

- `design`: architecture decisions, invariants, tradeoffs.
- `note`: warnings, operating assumptions, facts that should stay top-of-mind.
- `progress`: completed implementation state, validation, next step.
- `todo`: the next concrete task.
- `deferred`: real but postponed work.

## Update guidance

- Use `cotext update <id> --append ...` for incremental progress or validation evidence.
- Use `cotext update <id> --status done` when closing work.
- Use `cotext new <category> <title> ...` for newly discovered durable context.
- Prefer `cotext` commands over direct edits to `.cotext/entries/` unless you are repairing the tool or broken metadata.

## Command cookbook

```bash
cotext render --audience claude
cotext list --category todo --status active --status planned
cotext show <id>
cotext update <id> --append "Validation: ..."
cotext new note "Important environment caveat" --section env/setup --tag ops
cotext tui
```

## Generated guidance

- Project-local Claude guidance lives under `.claude/skills/cotext-context/` and `.claude/commands/`.
- Refresh it with `cotext agent install claude --overwrite`.
"#
    )
}

fn claude_context_command(project: &Project) -> String {
    let project_name = &project.config.name;
    format!(
        r#"---
description: Load the current cotext packet for this repository.
---

Run `cotext render --audience claude` from the project root and treat the result as the authoritative design/notes/progress/todo context for `{project_name}`.

Then:

1. If the user is asking what to do next or to continue ongoing work, also run `cotext list --category todo` and `cotext list --category deferred`.
2. If only one slice matters, narrow with `cotext render --category <category> --audience claude`, `cotext list --category <category>`, or `cotext show <id>`.
3. Summarize the active items you are going to follow before you proceed with implementation.
"#
    )
}

fn claude_sync_command(project: &Project) -> String {
    let project_name = &project.config.name;
    format!(
        r#"---
description: Sync meaningful project context changes back into cotext.
---

Compare the work you just completed against the current cotext packet for `{project_name}`.

Sync context with this checklist:

1. Update an existing entry with `cotext update <id> ...` if you advanced, clarified, or closed tracked work.
2. Use `cotext update <id> --append ...` for short evidence or validation notes.
3. Use `cotext update <id> --status done` when a todo is complete.
4. Create a new entry with `cotext new <category> <title> ...` when the work introduced a new durable design decision, warning, next step, or deferred item.
5. Prefer `cotext` commands or `cotext tui` over direct edits to `.cotext/entries/` unless you are repairing cotext itself.
6. Re-render the relevant packet or list after syncing so the final state is confirmed before handoff.
"#
    )
}

const CODEX_OPENAI_YAML: &str = "interface:\n  display_name: \"Cotext Context\"\n  short_description: \"Load, filter, and sync project context with cotext\"\n";

#[cfg(test)]
mod tests {
    use chrono::Utc;

    use super::*;
    use crate::model::ProjectConfig;

    fn demo_project() -> Project {
        let root = PathBuf::from("/tmp/cotext-demo");
        Project {
            data_dir: root.join(".cotext"),
            root,
            config: ProjectConfig {
                schema_version: 1,
                name: "demo".to_string(),
                created_at: Utc::now(),
            },
        }
    }

    #[test]
    fn codex_agents_block_includes_startup_and_refresh_guidance() {
        let block = codex_agents_block(&demo_project());
        assert!(block.contains("### Startup"));
        assert!(block.contains("cotext render --audience codex"));
        assert!(block.contains("cotext agent install codex --overwrite"));
    }

    #[test]
    fn codex_skill_includes_category_guide_and_cookbook() {
        let skill = codex_skill_md(&demo_project());
        assert!(skill.contains("## Category guide"));
        assert!(skill.contains("## Command cookbook"));
        assert!(skill.contains("cotext list --format json --category todo"));
        assert!(skill.contains("Prefer `cotext update` / `cotext new` / `cotext tui`"));
    }

    #[test]
    fn claude_guidance_mentions_load_and_sync_commands() {
        let skill = claude_skill_md(&demo_project());
        let load = claude_context_command(&demo_project());
        let sync = claude_sync_command(&demo_project());
        assert!(skill.contains("cotext render --audience claude"));
        assert!(load.contains("cotext list --category todo"));
        assert!(sync.contains("Re-render the relevant packet or list after syncing"));
    }
}
