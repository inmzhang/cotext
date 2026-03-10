use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use chrono::Utc;
use walkdir::WalkDir;

use crate::model::{
    Category, Entry, EntryFilter, EntryFrontMatter, EntryStatus, EntryUpdate, NewEntry,
    ProjectConfig, normalize_section,
};

const DATA_DIR: &str = ".cotext";
const CONFIG_FILE: &str = "cotext.toml";
const ENTRY_DIR: &str = "entries";

#[derive(Clone, Debug)]
pub struct Project {
    pub root: PathBuf,
    pub data_dir: PathBuf,
    pub config: ProjectConfig,
}

impl Project {
    pub fn init(root: &Path, name: Option<String>, force: bool) -> Result<Self> {
        let root = root
            .canonicalize()
            .or_else(|_| Ok::<PathBuf, std::io::Error>(root.to_path_buf()))?;
        let data_dir = root.join(DATA_DIR);
        let config_path = data_dir.join(CONFIG_FILE);
        if config_path.exists() && !force {
            bail!("cotext project already exists at {}", config_path.display());
        }

        fs::create_dir_all(data_dir.join(ENTRY_DIR))
            .with_context(|| format!("failed to create {}", data_dir.display()))?;
        for category in Category::ALL {
            fs::create_dir_all(data_dir.join(ENTRY_DIR).join(category.dir_name())).with_context(
                || format!("failed to create entry folder for {}", category.dir_name()),
            )?;
        }

        let config = ProjectConfig {
            schema_version: 1,
            name: name.unwrap_or_else(|| {
                root.file_name()
                    .and_then(|name| name.to_str())
                    .unwrap_or("cotext-project")
                    .to_string()
            }),
            created_at: Utc::now(),
        };
        let config_contents = toml::to_string_pretty(&config)?;
        fs::write(&config_path, config_contents)
            .with_context(|| format!("failed to write {}", config_path.display()))?;
        Self::discover(&root)
    }

    pub fn discover(start: &Path) -> Result<Self> {
        let start = if start.is_file() {
            start
                .parent()
                .context("cannot discover cotext project from a file without a parent")?
        } else {
            start
        };

        for candidate in start.ancestors() {
            let data_dir = candidate.join(DATA_DIR);
            let config_path = data_dir.join(CONFIG_FILE);
            if config_path.exists() {
                let raw = fs::read_to_string(&config_path)
                    .with_context(|| format!("failed to read {}", config_path.display()))?;
                let config: ProjectConfig = toml::from_str(&raw).with_context(|| {
                    format!("invalid project config at {}", config_path.display())
                })?;
                return Ok(Self {
                    root: candidate.to_path_buf(),
                    data_dir,
                    config,
                });
            }
        }

        bail!("no cotext project found starting from {}", start.display());
    }

    pub fn entry_dir(&self, category: Category) -> PathBuf {
        self.data_dir.join(ENTRY_DIR).join(category.dir_name())
    }

    pub fn list_entries(&self, filter: &EntryFilter) -> Result<Vec<Entry>> {
        let categories = filter
            .categories
            .clone()
            .unwrap_or_else(|| Category::ALL.to_vec());
        let mut entries = Vec::new();

        for category in categories {
            let category_dir = self.entry_dir(category);
            if !category_dir.exists() {
                continue;
            }
            for result in WalkDir::new(&category_dir) {
                let dent = result?;
                if !dent.file_type().is_file() {
                    continue;
                }
                if dent.path().extension().and_then(|ext| ext.to_str()) != Some("md") {
                    continue;
                }
                let entry = self.load_entry_from_path(dent.path())?;
                if matches_filter(&entry, filter) {
                    entries.push(entry);
                }
            }
        }

        entries.sort_by(|left, right| {
            left.category()
                .cmp(&right.category())
                .then_with(|| {
                    right
                        .front_matter
                        .updated_at
                        .cmp(&left.front_matter.updated_at)
                })
                .then_with(|| left.title().cmp(right.title()))
        });
        Ok(entries)
    }

    pub fn load_entry(&self, id: &str) -> Result<Entry> {
        let mut filter = EntryFilter::default();
        filter.id = Some(id.to_string());
        filter.include_archived = true;
        let mut entries = self.list_entries(&filter)?;
        match entries.len() {
            0 => bail!("entry `{id}` was not found"),
            1 => Ok(entries.remove(0)),
            _ => bail!("multiple entries matched `{id}`"),
        }
    }

    pub fn create_entry(&self, draft: NewEntry) -> Result<Entry> {
        let existing = self.list_entries(&EntryFilter {
            include_archived: true,
            ..EntryFilter::default()
        })?;
        let existing_ids = existing
            .into_iter()
            .map(|entry| entry.front_matter.id)
            .collect::<BTreeSet<_>>();
        let id = next_available_id(&draft.title, &existing_ids);
        let now = Utc::now();
        let front_matter = EntryFrontMatter {
            id,
            title: draft.title.clone(),
            category: draft.category,
            section: draft
                .section
                .and_then(|section| normalize_section(&section)),
            status: draft
                .status
                .unwrap_or_else(|| draft.category.default_status()),
            tags: draft.tags,
            created_at: now,
            updated_at: now,
        };
        let body = draft
            .body
            .unwrap_or_else(|| draft.category.placeholder_body(&draft.title));
        let entry = Entry {
            front_matter,
            body,
            path: PathBuf::new(),
        };
        self.persist_entry(entry, None)
    }

    pub fn update_entry(&self, id: &str, patch: EntryUpdate) -> Result<Entry> {
        let existing = self.load_entry(id)?;
        let mut updated = existing.clone();
        let previous_path = existing.path.clone();

        if let Some(title) = patch.title {
            updated.front_matter.title = title;
        }
        if let Some(category) = patch.category {
            updated.front_matter.category = category;
        }
        if let Some(section) = patch.section {
            updated.front_matter.section = normalize_section(&section);
        }
        if patch.clear_section {
            updated.front_matter.section = None;
        }
        if let Some(status) = patch.status {
            updated.front_matter.status = status;
        }
        if !patch.add_tags.is_empty() {
            updated.front_matter.tags.extend(patch.add_tags);
        }
        if !patch.remove_tags.is_empty() {
            updated
                .front_matter
                .tags
                .retain(|tag| !patch.remove_tags.contains(tag));
        }
        if let Some(body) = patch.body {
            updated.body = body;
        }
        if let Some(append) = patch.append {
            if !updated.body.trim_end().is_empty() {
                updated.body.push_str("\n\n");
            }
            updated.body.push_str(&append);
        }
        updated.front_matter.updated_at = Utc::now();
        self.persist_entry(updated, Some(previous_path.as_path()))
    }

    fn persist_entry(&self, mut entry: Entry, previous_path: Option<&Path>) -> Result<Entry> {
        let path = entry_path(self, &entry.front_matter);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("failed to create {}", parent.display()))?;
        }
        fs::write(&path, serialize_entry(&entry)?)
            .with_context(|| format!("failed to write {}", path.display()))?;
        if let Some(previous_path) = previous_path {
            if previous_path != path && previous_path.exists() {
                fs::remove_file(previous_path).with_context(|| {
                    format!(
                        "failed to remove superseded entry {}",
                        previous_path.display()
                    )
                })?;
            }
        }
        entry.path = path;
        Ok(entry)
    }

    fn load_entry_from_path(&self, path: &Path) -> Result<Entry> {
        let raw = fs::read_to_string(path)
            .with_context(|| format!("failed to read {}", path.display()))?;
        parse_entry(&raw, path)
    }
}

fn matches_filter(entry: &Entry, filter: &EntryFilter) -> bool {
    if !filter.include_archived && entry.status() == EntryStatus::Archived {
        return false;
    }
    if let Some(id) = &filter.id {
        if entry.id() != id {
            return false;
        }
    }
    if let Some(statuses) = &filter.statuses {
        if !statuses.contains(&entry.status()) {
            return false;
        }
    }
    if let Some(section_prefix) = &filter.section_prefix {
        match entry.section() {
            Some(section) if section.starts_with(section_prefix) => {}
            _ => return false,
        }
    }
    true
}

fn entry_path(project: &Project, front_matter: &EntryFrontMatter) -> PathBuf {
    let mut path = project.entry_dir(front_matter.category);
    if let Some(section) = &front_matter.section {
        for segment in section.split('/') {
            path.push(segment);
        }
    }
    path.push(format!("{}.md", front_matter.id));
    path
}

fn parse_entry(raw: &str, path: &Path) -> Result<Entry> {
    let normalized = raw.replace("\r\n", "\n");
    let remainder = normalized
        .strip_prefix("---\n")
        .context("entry is missing YAML front matter")?;
    let divider = remainder
        .find("\n---\n")
        .context("entry front matter is not terminated with `---`")?;
    let metadata = &remainder[..divider];
    let body = &remainder[(divider + "\n---\n".len())..];
    let front_matter: EntryFrontMatter = serde_yaml::from_str(metadata)
        .with_context(|| format!("invalid front matter at {}", path.display()))?;
    Ok(Entry {
        front_matter,
        body: body.trim_end().to_string(),
        path: path.to_path_buf(),
    })
}

fn serialize_entry(entry: &Entry) -> Result<String> {
    let front_matter = serde_yaml::to_string(&entry.front_matter)?;
    let mut output = String::new();
    output.push_str("---\n");
    output.push_str(&front_matter);
    output.push_str("---\n");
    output.push_str(entry.body.trim_end());
    output.push('\n');
    Ok(output)
}

fn next_available_id(title: &str, existing: &BTreeSet<String>) -> String {
    let base = slugify(title);
    if !existing.contains(&base) {
        return base;
    }

    let mut counter = 2;
    loop {
        let candidate = format!("{base}-{counter}");
        if !existing.contains(&candidate) {
            return candidate;
        }
        counter += 1;
    }
}

pub fn slugify(input: &str) -> String {
    let mut slug = String::new();
    let mut last_dash = false;
    for ch in input.chars() {
        if ch.is_ascii_alphanumeric() {
            slug.push(ch.to_ascii_lowercase());
            last_dash = false;
        } else if !slug.is_empty() && !last_dash {
            slug.push('-');
            last_dash = true;
        }
    }
    let trimmed = slug.trim_matches('-').to_string();
    if trimmed.is_empty() {
        "entry".to_string()
    } else {
        trimmed
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use tempfile::TempDir;

    use super::*;
    use crate::model::{Category, EntryStatus, NewEntry};

    #[test]
    fn init_create_update_roundtrip() -> Result<()> {
        let temp = TempDir::new()?;
        let project = Project::init(temp.path(), Some("demo".to_string()), false)?;
        let entry = project.create_entry(NewEntry {
            category: Category::Todo,
            title: "Add agent sync".to_string(),
            section: Some("agents/codex".to_string()),
            status: Some(EntryStatus::Planned),
            tags: BTreeSet::from(["agent".to_string(), "codex".to_string()]),
            body: Some("Need a reliable sync path.".to_string()),
        })?;

        let loaded = project.load_entry(entry.id())?;
        assert_eq!(loaded.title(), "Add agent sync");
        assert_eq!(loaded.section(), Some("agents/codex"));

        let updated = project.update_entry(
            entry.id(),
            EntryUpdate {
                status: Some(EntryStatus::Active),
                append: Some("Validation: command and skill scaffolds.".to_string()),
                ..EntryUpdate::default()
            },
        )?;
        assert_eq!(updated.status(), EntryStatus::Active);
        assert!(updated.body.contains("Validation"));

        let rendered = fs::read_to_string(updated.path)?;
        assert!(rendered.contains("Add agent sync"));
        Ok(())
    }

    #[test]
    fn slugify_is_stable() {
        assert_eq!(slugify("Hello, World!"), "hello-world");
        assert_eq!(slugify("  "), "entry");
        assert_eq!(slugify("already-slugged"), "already-slugged");
    }
}
