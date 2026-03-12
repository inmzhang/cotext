use std::collections::BTreeMap;
use std::fmt::Write;

use chrono::Local;

use crate::model::{Audience, Category, Entry, EntryStatus};
use crate::storage::Project;

pub fn render_packet(project: &Project, entries: &[Entry], audience: Audience) -> String {
    let mut grouped: BTreeMap<Category, Vec<&Entry>> = BTreeMap::new();
    for category in Category::ALL {
        grouped.entry(category).or_default();
    }
    for entry in entries {
        grouped.entry(entry.category()).or_default().push(entry);
    }

    let mut out = String::new();
    let generated_at = Local::now().format("%Y-%m-%d %H:%M:%S %Z");
    let title = match audience {
        Audience::Human => "Cotext Project View",
        Audience::Agent => "Cotext Agent Packet",
        Audience::Codex => "Cotext Packet for Codex",
        Audience::Claude => "Cotext Packet for Claude Code",
    };
    let _ = writeln!(&mut out, "# {title}");
    let _ = writeln!(&mut out);
    let _ = writeln!(&mut out, "- Project: {}", project.config.name);
    let _ = writeln!(&mut out, "- Root: `{}`", project.root.display());
    let _ = writeln!(
        &mut out,
        "- Storage: {} (`{}`)",
        project.storage_scope(),
        project.data_dir_display()
    );
    let _ = writeln!(&mut out, "- Generated: {generated_at}");
    let _ = writeln!(
        &mut out,
        "- Update flow: `cotext new`, `cotext update`, or `cotext tui`"
    );
    let _ = writeln!(&mut out);
    match audience {
        Audience::Human => {
            let _ = writeln!(
                &mut out,
                "Use this as the single source of truth for design, awareness notes, progress, next work, and deferred work."
            );
        }
        Audience::Agent | Audience::Codex | Audience::Claude => {
            let _ = writeln!(
                &mut out,
                "Read this packet before coding. When work changes design, notes, progress, or todo state, write the update back through cotext instead of scattering raw markdown."
            );
        }
    }
    let _ = writeln!(&mut out);
    let _ = writeln!(&mut out, "## Snapshot");
    let _ = writeln!(&mut out);
    for category in Category::ALL {
        let items = grouped.get(&category).map(Vec::as_slice).unwrap_or(&[]);
        let open = items
            .iter()
            .filter(|entry| !matches!(entry.status(), EntryStatus::Done | EntryStatus::Archived))
            .count();
        let _ = writeln!(
            &mut out,
            "- {}: {} entries, {} open",
            category.label(),
            items.len(),
            open
        );
    }

    for category in Category::ALL {
        let _ = writeln!(&mut out);
        let _ = writeln!(&mut out, "## {}", category.label());
        let _ = writeln!(&mut out);
        let items = grouped.get(&category).map(Vec::as_slice).unwrap_or(&[]);
        if items.is_empty() {
            let _ = writeln!(&mut out, "_None yet._");
            continue;
        }
        for entry in items {
            render_entry(&mut out, entry);
        }
    }

    out
}

pub fn render_single_entry(entry: &Entry) -> String {
    let mut out = String::new();
    render_entry(&mut out, entry);
    out
}

pub fn render_clipboard_packet(project: &Project, entries: &[Entry], audience: Audience) -> String {
    let mut rendered = String::new();
    let _ = writeln!(&mut rendered, "BEGIN_COTEXT_PACKET");
    rendered.push_str(&render_packet(project, entries, audience));
    let _ = writeln!(&mut rendered, "END_COTEXT_PACKET");
    rendered
}

fn render_entry(out: &mut String, entry: &Entry) {
    let _ = writeln!(out, "### {} (`{}`)", entry.title(), entry.id());
    let _ = writeln!(out);
    let _ = writeln!(out, "- Status: {}", entry.status());
    if let Some(section) = entry.section() {
        let _ = writeln!(out, "- Section: `{section}`");
    }
    if !entry.front_matter.tags.is_empty() {
        let tags = entry
            .front_matter
            .tags
            .iter()
            .map(String::as_str)
            .collect::<Vec<_>>()
            .join(", ");
        let _ = writeln!(out, "- Tags: `{tags}`");
    }
    let _ = writeln!(
        out,
        "- Updated: {}",
        entry.front_matter.updated_at.format("%Y-%m-%d %H:%M UTC")
    );
    let _ = writeln!(out);
    let _ = writeln!(out, "{}", entry.body.trim());
    let _ = writeln!(out);
}
