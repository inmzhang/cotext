use std::collections::BTreeSet;
use std::fmt;
use std::path::PathBuf;

use chrono::{DateTime, Utc};
use clap::ValueEnum;
use serde::{Deserialize, Serialize};

#[derive(
    Clone, Copy, Debug, Default, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize, ValueEnum,
)]
#[serde(rename_all = "kebab-case")]
pub enum Audience {
    #[default]
    Human,
    Agent,
    Codex,
    Claude,
}

impl fmt::Display for Audience {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let label = match self {
            Self::Human => "human",
            Self::Agent => "agent",
            Self::Codex => "codex",
            Self::Claude => "claude",
        };
        write!(f, "{label}")
    }
}

#[derive(
    Clone, Copy, Debug, Default, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize, ValueEnum,
)]
#[serde(rename_all = "kebab-case")]
pub enum Category {
    #[default]
    Design,
    Note,
    Progress,
    Todo,
    Deferred,
}

impl Category {
    pub const ALL: [Self; 5] = [
        Self::Design,
        Self::Note,
        Self::Progress,
        Self::Todo,
        Self::Deferred,
    ];

    pub fn dir_name(self) -> &'static str {
        match self {
            Self::Design => "design",
            Self::Note => "note",
            Self::Progress => "progress",
            Self::Todo => "todo",
            Self::Deferred => "deferred",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Design => "Design",
            Self::Note => "Notes To Be Aware Of",
            Self::Progress => "Progress",
            Self::Todo => "Next Todos",
            Self::Deferred => "Deferred Implementation",
        }
    }

    pub fn placeholder_body(self, title: &str) -> String {
        match self {
            Self::Design => format!(
                "Why this matters\n- Explain the design choice behind `{title}`.\n\nDecision\n- Capture the current direction and the constraints that shaped it.\n\nOpen questions\n- Record what still needs validation."
            ),
            Self::Note => format!(
                "Context\n- Record the thing the agent or human should stay aware of for `{title}`.\n\nImpact\n- Explain how it changes implementation or review decisions."
            ),
            Self::Progress => format!(
                "Completed\n- Summarize what has already landed for `{title}`.\n\nEvidence\n- Mention the files, commands, or validation that prove the current state.\n\nNext step\n- State the immediate follow-up."
            ),
            Self::Todo => format!(
                "Goal\n- Describe the next actionable task for `{title}`.\n\nAcceptance\n- Define what would count as done.\n\nDependencies\n- Mention blockers or prerequisites."
            ),
            Self::Deferred => format!(
                "Deferred scope\n- Describe the future work captured by `{title}`.\n\nReason deferred\n- Explain why it is postponed.\n\nTrigger to resume\n- Record the signal that should bring it back."
            ),
        }
    }

    pub fn default_status(self) -> EntryStatus {
        match self {
            Self::Design => EntryStatus::Draft,
            Self::Note => EntryStatus::Active,
            Self::Progress => EntryStatus::Active,
            Self::Todo => EntryStatus::Planned,
            Self::Deferred => EntryStatus::Deferred,
        }
    }
}

impl fmt::Display for Category {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.dir_name())
    }
}

#[derive(
    Clone, Copy, Debug, Default, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize, ValueEnum,
)]
#[serde(rename_all = "kebab-case")]
pub enum EntryStatus {
    #[default]
    Draft,
    Active,
    Planned,
    Blocked,
    Done,
    Deferred,
    Archived,
}

impl EntryStatus {
    pub const CYCLE: [Self; 7] = [
        Self::Draft,
        Self::Active,
        Self::Planned,
        Self::Blocked,
        Self::Done,
        Self::Deferred,
        Self::Archived,
    ];

    pub fn badge(self) -> &'static str {
        match self {
            Self::Draft => "draft",
            Self::Active => "active",
            Self::Planned => "planned",
            Self::Blocked => "blocked",
            Self::Done => "done",
            Self::Deferred => "deferred",
            Self::Archived => "archived",
        }
    }

    pub fn next(self) -> Self {
        let index = Self::CYCLE
            .iter()
            .position(|candidate| *candidate == self)
            .unwrap_or(0);
        Self::CYCLE[(index + 1) % Self::CYCLE.len()]
    }
}

impl fmt::Display for EntryStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.badge())
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ProjectConfig {
    pub schema_version: u32,
    pub name: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct EntryFrontMatter {
    pub id: String,
    pub title: String,
    pub category: Category,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub section: Option<String>,
    #[serde(default)]
    pub status: EntryStatus,
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub tags: BTreeSet<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Clone, Debug)]
pub struct Entry {
    pub front_matter: EntryFrontMatter,
    pub body: String,
    pub path: PathBuf,
}

impl Entry {
    pub fn id(&self) -> &str {
        &self.front_matter.id
    }

    pub fn title(&self) -> &str {
        &self.front_matter.title
    }

    pub fn category(&self) -> Category {
        self.front_matter.category
    }

    pub fn status(&self) -> EntryStatus {
        self.front_matter.status
    }

    pub fn section(&self) -> Option<&str> {
        self.front_matter.section.as_deref()
    }
}

#[derive(Clone, Debug, Default)]
pub struct EntryFilter {
    pub categories: Option<Vec<Category>>,
    pub statuses: Option<Vec<EntryStatus>>,
    pub section_prefix: Option<String>,
    pub id: Option<String>,
    pub include_archived: bool,
}

#[derive(Clone, Debug)]
pub struct NewEntry {
    pub category: Category,
    pub title: String,
    pub section: Option<String>,
    pub status: Option<EntryStatus>,
    pub tags: BTreeSet<String>,
    pub body: Option<String>,
}

#[derive(Clone, Debug, Default)]
pub struct EntryUpdate {
    pub title: Option<String>,
    pub category: Option<Category>,
    pub section: Option<String>,
    pub clear_section: bool,
    pub status: Option<EntryStatus>,
    pub body: Option<String>,
    pub append: Option<String>,
    pub add_tags: BTreeSet<String>,
    pub remove_tags: BTreeSet<String>,
}

pub fn normalize_section(input: &str) -> Option<String> {
    let normalized = input
        .split('/')
        .map(str::trim)
        .filter(|segment| !segment.is_empty())
        .collect::<Vec<_>>()
        .join("/");
    if normalized.is_empty() {
        None
    } else {
        Some(normalized)
    }
}
