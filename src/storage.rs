use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use chrono::Utc;
use walkdir::WalkDir;

use crate::model::{
    Category, Entry, EntryFilter, EntryFrontMatter, EntryStatus, EntryUpdate, NewEntry,
    ProjectConfig, StorageScope, current_schema_version, normalize_section,
};

const LOCAL_DATA_DIR: &str = ".cotext";
const CONFIG_FILE: &str = "cotext.toml";
const ENTRY_DIR: &str = "entries";

#[derive(Clone, Debug)]
pub struct Project {
    pub root: PathBuf,
    pub data_dir: PathBuf,
    pub config: ProjectConfig,
}

impl Project {
    pub fn init(
        root: &Path,
        name: Option<String>,
        force: bool,
        storage: StorageScope,
    ) -> Result<Self> {
        Self::init_with_data_root(root, name, force, storage, None)
    }

    fn init_with_data_root(
        root: &Path,
        name: Option<String>,
        force: bool,
        storage: StorageScope,
        global_data_root: Option<&Path>,
    ) -> Result<Self> {
        let root = root
            .canonicalize()
            .or_else(|_| Ok::<PathBuf, std::io::Error>(root.to_path_buf()))?;
        let project_name = name.unwrap_or_else(|| default_project_name(&root));
        let data_dir = match storage {
            StorageScope::Local => root.join(LOCAL_DATA_DIR),
            StorageScope::Global => {
                let global_data_root = match global_data_root {
                    Some(path) => path.to_path_buf(),
                    None => system_data_root()?,
                };
                global_project_data_dir(&global_data_root, &project_name)
            }
        };
        let config_path = data_dir.join(CONFIG_FILE);
        if config_path.exists() {
            let existing = load_config(&config_path)?;
            if storage == StorageScope::Global
                && existing
                    .project_root
                    .as_ref()
                    .is_some_and(|existing_root| existing_root != &root)
            {
                bail!(
                    "global cotext project `{}` already exists at {} for {}",
                    existing.name,
                    config_path.display(),
                    existing
                        .project_root
                        .as_ref()
                        .map(|path| path.display().to_string())
                        .unwrap_or_else(|| "<unknown root>".to_string())
                );
            }
            if !force {
                bail!("cotext project already exists at {}", config_path.display());
            }
        }

        fs::create_dir_all(data_dir.join(ENTRY_DIR))
            .with_context(|| format!("failed to create {}", data_dir.display()))?;
        for category in Category::ALL {
            fs::create_dir_all(data_dir.join(ENTRY_DIR).join(category.dir_name())).with_context(
                || format!("failed to create entry folder for {}", category.dir_name()),
            )?;
        }

        let config = ProjectConfig {
            schema_version: current_schema_version(),
            name: project_name,
            created_at: Utc::now(),
            storage,
            project_root: match storage {
                StorageScope::Local => None,
                StorageScope::Global => Some(root.clone()),
            },
        };
        let config_contents = toml::to_string_pretty(&config)?;
        fs::write(&config_path, config_contents)
            .with_context(|| format!("failed to write {}", config_path.display()))?;
        Ok(Self {
            root,
            data_dir,
            config,
        })
    }

    pub fn discover(start: &Path, preferred_storage: StorageScope) -> Result<Self> {
        let start = discovery_start(start)?;
        match preferred_storage {
            StorageScope::Local => Self::discover_local(&start),
            StorageScope::Global => match Self::discover_global(&start, None) {
                Ok(project) => Ok(project),
                Err(global_error) => match Self::discover_local(&start) {
                    Ok(project) => Ok(project),
                    Err(local_error) => bail!(
                        "no cotext project found starting from {} (global lookup failed: {}; repo-local lookup failed: {})",
                        start.display(),
                        global_error,
                        local_error
                    ),
                },
            },
        }
    }

    pub fn entry_dir(&self, category: Category) -> PathBuf {
        self.data_dir.join(ENTRY_DIR).join(category.dir_name())
    }

    pub fn storage_scope(&self) -> StorageScope {
        self.config.storage
    }

    pub fn data_dir_display(&self) -> String {
        self.data_dir
            .strip_prefix(&self.root)
            .map(|path| path.display().to_string())
            .unwrap_or_else(|_| self.data_dir.display().to_string())
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
        let filter = EntryFilter {
            id: Some(id.to_string()),
            include_archived: true,
            ..EntryFilter::default()
        };
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

    pub fn reconcile_edited_entry(&self, original_path: &Path) -> Result<Entry> {
        let mut edited = self.load_entry_from_path(original_path)?;
        let previous_path = edited.path.clone();
        edited.front_matter.updated_at = Utc::now();
        self.persist_entry(edited, Some(previous_path.as_path()))
    }

    pub fn delete_entry(&self, id: &str) -> Result<Entry> {
        let entry = self.load_entry(id)?;
        fs::remove_file(&entry.path)
            .with_context(|| format!("failed to delete {}", entry.path.display()))?;
        prune_empty_section_dirs(entry.path.parent(), &self.entry_dir(entry.category()))?;
        Ok(entry)
    }

    fn persist_entry(&self, mut entry: Entry, previous_path: Option<&Path>) -> Result<Entry> {
        let path = entry_path(self, &entry.front_matter);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("failed to create {}", parent.display()))?;
        }
        fs::write(&path, serialize_entry(&entry)?)
            .with_context(|| format!("failed to write {}", path.display()))?;
        if let Some(previous_path) = previous_path
            && previous_path != path
            && previous_path.exists()
        {
            fs::remove_file(previous_path).with_context(|| {
                format!(
                    "failed to remove superseded entry {}",
                    previous_path.display()
                )
            })?;
        }
        entry.path = path;
        Ok(entry)
    }

    fn load_entry_from_path(&self, path: &Path) -> Result<Entry> {
        let raw = fs::read_to_string(path)
            .with_context(|| format!("failed to read {}", path.display()))?;
        parse_entry(&raw, path)
    }

    fn discover_local(start: &Path) -> Result<Self> {
        for candidate in start.ancestors() {
            let data_dir = candidate.join(LOCAL_DATA_DIR);
            let config_path = data_dir.join(CONFIG_FILE);
            if config_path.exists() {
                return load_project(&config_path, data_dir, candidate.to_path_buf());
            }
        }

        bail!(
            "no repo-local cotext project found from {}",
            start.display()
        );
    }

    fn discover_global(start: &Path, global_data_root: Option<&Path>) -> Result<Self> {
        for candidate in start.ancestors() {
            let config_path = candidate.join(CONFIG_FILE);
            if config_path.exists() {
                let project = load_project(
                    &config_path,
                    candidate.to_path_buf(),
                    candidate.to_path_buf(),
                )?;
                if project.storage_scope() == StorageScope::Global {
                    return Ok(project);
                }
            }
        }

        let global_data_root = match global_data_root {
            Some(path) => path.to_path_buf(),
            None => system_data_root()?,
        };
        if !global_data_root.exists() {
            bail!(
                "global cotext data root {} does not exist",
                global_data_root.display()
            );
        }

        let ancestors = start.ancestors().map(Path::to_path_buf).collect::<Vec<_>>();
        let mut best_match: Option<(usize, Project)> = None;

        for result in fs::read_dir(&global_data_root)
            .with_context(|| format!("failed to inspect {}", global_data_root.display()))?
        {
            let entry = result?;
            if !entry.file_type()?.is_dir() {
                continue;
            }

            let data_dir = entry.path();
            let config_path = data_dir.join(CONFIG_FILE);
            if !config_path.exists() {
                continue;
            }

            let project = load_project(&config_path, data_dir.clone(), data_dir)?;
            if project.storage_scope() != StorageScope::Global {
                continue;
            }

            let Some(rank) = ancestors
                .iter()
                .position(|candidate| *candidate == project.root)
            else {
                continue;
            };

            if let Some((best_rank, best_project)) = &best_match {
                if rank < *best_rank {
                    best_match = Some((rank, project));
                    continue;
                }
                if rank == *best_rank && best_project.data_dir != project.data_dir {
                    bail!(
                        "multiple global cotext projects matched {}: {} and {}",
                        project.root.display(),
                        best_project.data_dir.display(),
                        project.data_dir.display()
                    );
                }
                continue;
            }

            best_match = Some((rank, project));
        }

        best_match
            .map(|(_, project)| project)
            .context("no global cotext project matched the requested path")
    }
}

fn discovery_start(start: &Path) -> Result<PathBuf> {
    let start = if start.is_file() {
        start
            .parent()
            .context("cannot discover cotext project from a file without a parent")?
    } else {
        start
    };
    Ok(start
        .canonicalize()
        .or_else(|_| Ok::<PathBuf, std::io::Error>(start.to_path_buf()))?)
}

fn default_project_name(root: &Path) -> String {
    root.file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("cotext-project")
        .to_string()
}

fn system_data_root() -> Result<PathBuf> {
    dirs::data_local_dir().context("failed to resolve the system-local data directory")
}

fn global_project_data_dir(global_data_root: &Path, project_name: &str) -> PathBuf {
    global_data_root.join(global_project_dir_name(project_name))
}

fn global_project_dir_name(project_name: &str) -> String {
    let sanitized = project_name
        .chars()
        .map(|ch| match ch {
            '/' | '\\' => '-',
            _ if ch.is_control() => '-',
            _ => ch,
        })
        .collect::<String>()
        .trim()
        .trim_matches('.')
        .to_string();
    if sanitized.is_empty() {
        "cotext-project".to_string()
    } else {
        sanitized
    }
}

fn load_config(config_path: &Path) -> Result<ProjectConfig> {
    let raw = fs::read_to_string(config_path)
        .with_context(|| format!("failed to read {}", config_path.display()))?;
    toml::from_str(&raw)
        .with_context(|| format!("invalid project config at {}", config_path.display()))
}

fn load_project(config_path: &Path, data_dir: PathBuf, fallback_root: PathBuf) -> Result<Project> {
    let config = load_config(config_path)?;
    let root = match config.storage {
        StorageScope::Local => fallback_root,
        StorageScope::Global => config
            .project_root
            .clone()
            .context("global cotext config is missing `project_root`")?,
    };
    Ok(Project {
        root,
        data_dir,
        config,
    })
}

fn matches_filter(entry: &Entry, filter: &EntryFilter) -> bool {
    if !filter.include_archived && entry.status() == EntryStatus::Archived {
        return false;
    }
    if let Some(id) = &filter.id
        && entry.id() != id
    {
        return false;
    }
    if let Some(statuses) = &filter.statuses
        && !statuses.contains(&entry.status())
    {
        return false;
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

fn prune_empty_section_dirs(start: Option<&Path>, stop: &Path) -> Result<()> {
    let mut current = start.map(Path::to_path_buf);
    while let Some(path) = current {
        if path == stop {
            break;
        }

        let mut entries =
            fs::read_dir(&path).with_context(|| format!("failed to inspect {}", path.display()))?;
        if entries.next().is_some() {
            break;
        }

        fs::remove_dir(&path)
            .with_context(|| format!("failed to remove empty directory {}", path.display()))?;
        current = path.parent().map(Path::to_path_buf);
    }
    Ok(())
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
    use std::time::Duration;

    use tempfile::TempDir;

    use super::*;
    use crate::model::{Category, EntryStatus, NewEntry, StorageScope};

    #[test]
    fn init_create_update_roundtrip() -> Result<()> {
        let temp = TempDir::new()?;
        let project = Project::init(
            temp.path(),
            Some("demo".to_string()),
            false,
            StorageScope::Local,
        )?;
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

    #[test]
    fn delete_entry_removes_file_and_empty_section_dirs() -> Result<()> {
        let temp = TempDir::new()?;
        let project = Project::init(
            temp.path(),
            Some("demo".to_string()),
            false,
            StorageScope::Local,
        )?;
        let entry = project.create_entry(NewEntry {
            category: Category::Todo,
            title: "Delete me".to_string(),
            section: Some("agents/codex".to_string()),
            status: Some(EntryStatus::Planned),
            tags: BTreeSet::new(),
            body: Some("Cleanup body".to_string()),
        })?;

        let entry_path = entry.path.clone();
        let section_root = project.entry_dir(Category::Todo).join("agents");

        project.delete_entry(entry.id())?;

        assert!(!entry_path.exists());
        assert!(!section_root.exists());
        assert!(project.load_entry(entry.id()).is_err());
        Ok(())
    }

    #[test]
    fn reconcile_edited_entry_updates_timestamp_and_body() -> Result<()> {
        let temp = TempDir::new()?;
        let project = Project::init(
            temp.path(),
            Some("demo".to_string()),
            false,
            StorageScope::Local,
        )?;
        let entry = project.create_entry(NewEntry {
            category: Category::Todo,
            title: "Edit me outside".to_string(),
            section: None,
            status: Some(EntryStatus::Planned),
            tags: BTreeSet::new(),
            body: Some("Original body".to_string()),
        })?;

        let original_updated_at = entry.front_matter.updated_at;
        std::thread::sleep(Duration::from_millis(5));
        let raw = fs::read_to_string(&entry.path)?;
        fs::write(&entry.path, raw.replace("Original body", "Edited body"))?;

        let reconciled = project.reconcile_edited_entry(&entry.path)?;

        assert_eq!(reconciled.body, "Edited body");
        assert!(reconciled.front_matter.updated_at > original_updated_at);
        Ok(())
    }

    #[test]
    fn global_init_stores_entries_under_system_data_root() -> Result<()> {
        let repo = TempDir::new()?;
        let data_root = TempDir::new()?;
        fs::create_dir_all(repo.path().join("src"))?;

        let project = Project::init_with_data_root(
            repo.path(),
            Some("demo-project".to_string()),
            false,
            StorageScope::Global,
            Some(data_root.path()),
        )?;

        assert_eq!(project.storage_scope(), StorageScope::Global);
        assert_eq!(project.root, repo.path().canonicalize()?);
        assert_eq!(project.data_dir, data_root.path().join("demo-project"));
        assert!(project.data_dir.join(CONFIG_FILE).exists());
        assert!(project.entry_dir(Category::Todo).exists());

        let discovered =
            Project::discover_global(&repo.path().join("src"), Some(data_root.path()))?;
        assert_eq!(discovered.root, project.root);
        assert_eq!(discovered.data_dir, project.data_dir);
        Ok(())
    }

    #[test]
    fn global_discovery_prefers_global_storage_when_local_and_global_exist() -> Result<()> {
        let repo = TempDir::new()?;
        let data_root = TempDir::new()?;

        let local = Project::init(
            repo.path(),
            Some("demo-project".to_string()),
            false,
            StorageScope::Local,
        )?;
        let global = Project::init_with_data_root(
            repo.path(),
            Some("demo-project".to_string()),
            false,
            StorageScope::Global,
            Some(data_root.path()),
        )?;

        let discovered = Project::discover_global(repo.path(), Some(data_root.path()))?;
        assert_eq!(discovered.data_dir, global.data_dir);
        assert_ne!(discovered.data_dir, local.data_dir);
        Ok(())
    }
}
