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
    format!(
        "## Cotext Workflow\n\n\
Use `cotext` as the canonical project context manager for `{}`.\n\n\
- Read the current packet with `cotext render --audience codex` before substantial work.\n\
- Use `cotext list --category todo` or `cotext list --category deferred` when planning next steps.\n\
- Write back meaningful design, progress, note, or todo changes through `cotext new`, `cotext update`, or `cotext tui`.\n\
- The project-local Codex skill scaffold lives under `.cotext/agents/codex/cotext-context/`.\n",
        project.config.name
    )
}

fn claude_agents_block(project: &Project) -> String {
    format!(
        "## Cotext Workflow\n\n\
Use `cotext` as the canonical project context manager for `{}`.\n\n\
- Refresh context with `cotext render --audience claude` before coding.\n\
- Use `.claude/commands/cotext.md` to load the full packet inside Claude Code.\n\
- Use `.claude/commands/cotext-sync.md` after meaningful work to sync design/progress/todo changes.\n\
- Project-local skill instructions live under `.claude/skills/cotext-context/`.\n",
        project.config.name
    )
}

fn codex_skill_md(project: &Project) -> String {
    format!(
        "---\n\
name: cotext-context\n\
description: Read and update structured project context for {} with cotext. Use when you need the latest design notes, awareness notes, progress, next todos, or deferred implementation items.\n\
---\n\n\
# Cotext Context\n\n\
## When to use\n\n\
Use this skill when the user asks for prior design, notes to keep in mind, progress state, current todos, or deferred work, or when you are about to change the code and need the canonical project packet.\n\n\
## Workflow\n\n\
1. Read the current packet with `cotext render --audience codex`.\n\
2. Narrow the view with `cotext list --category <category>` or `cotext render --category <category>` when only one slice matters.\n\
3. After meaningful work, write back updates with `cotext new` or `cotext update`.\n\
4. When a human wants a single visual review/edit surface, suggest `cotext tui`.\n",
        project.config.name
    )
}

fn claude_skill_md(project: &Project) -> String {
    format!(
        "---\n\
name: cotext-context\n\
description: Read and update structured project context for {} with cotext. Use when you need design notes, awareness notes, progress, next todos, or deferred work.\n\
---\n\n\
# Cotext Context\n\n\
1. Run `cotext render --audience claude` to load the current project packet.\n\
2. Filter by category when the task only touches one slice of context.\n\
3. After meaningful work, sync changes with `cotext update` or `cotext new`.\n\
4. Use `cotext tui` when a human wants to review and edit context on a single page.\n",
        project.config.name
    )
}

fn claude_context_command(project: &Project) -> String {
    format!(
        "---\n\
description: Load the current cotext packet for this repository.\n\
---\n\n\
Run `cotext render --audience claude` from the project root, read the packet carefully, and treat it as the authoritative design/notes/progress/todo context for `{}` before proceeding.\n",
        project.config.name
    )
}

fn claude_sync_command(project: &Project) -> String {
    format!(
        "---\n\
description: Sync meaningful project context changes back into cotext.\n\
---\n\n\
Summarize the new design, note, progress, todo, or deferred changes that came out of the current work on `{}`. Then run the appropriate `cotext new` or `cotext update` commands so the project context stays current.\n",
        project.config.name
    )
}

const CODEX_OPENAI_YAML: &str = "interface:\n  display_name: \"Cotext Context\"\n  short_description: \"Read or update project context with cotext\"\n";
