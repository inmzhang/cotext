use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use anyhow::{Context, Result, bail};
use arboard::Clipboard;
use chrono::Utc;
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyModifiers};
use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{
    Block, BorderType, Borders, Clear, List, ListItem, ListState, Paragraph, Wrap,
};
use ratatui::{Frame, Terminal};
use tui_textarea::TextArea;

use crate::model::{
    Audience, Category, Entry, EntryFilter, EntryFrontMatter, EntryStatus, EntryUpdate, NewEntry,
    normalize_section,
};
use crate::render::{render_clipboard_packet, render_packet, render_single_entry};
use crate::storage::{Project, slugify};

const APP_BG: Color = Color::Rgb(6, 11, 19);
const PANEL_BG: Color = Color::Rgb(15, 23, 42);
const PANEL_ALT_BG: Color = Color::Rgb(19, 35, 58);
const PANEL_MUTED_BG: Color = Color::Rgb(30, 41, 59);
const TEXT_PRIMARY: Color = Color::Rgb(226, 232, 240);
const TEXT_BRIGHT: Color = Color::Rgb(248, 250, 252);
const TEXT_MUTED: Color = Color::Rgb(148, 163, 184);
const ACCENT: Color = Color::Rgb(45, 212, 191);
const ACCENT_WARM: Color = Color::Rgb(251, 191, 36);
const INFO: Color = Color::Rgb(96, 165, 250);
const SUCCESS: Color = Color::Rgb(74, 222, 128);
const WARNING: Color = Color::Rgb(250, 204, 21);
const DANGER: Color = Color::Rgb(248, 113, 113);

pub fn run(project: Project) -> Result<()> {
    enable_raw_mode()?;
    let mut stdout = std::io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;
    let result = run_loop(&mut terminal, project);
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;
    result
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Mode {
    Browse,
    EditTitle,
    EditSection,
    EditTags,
    CreateTitle,
    CreateSection,
    CreateTags,
}

impl Mode {
    fn is_create(self) -> bool {
        matches!(
            self,
            Self::CreateTitle | Self::CreateSection | Self::CreateTags
        )
    }

    fn label(self) -> &'static str {
        match self {
            Self::Browse => "browse",
            Self::EditTitle => "title",
            Self::EditSection => "section",
            Self::EditTags => "tags",
            Self::CreateTitle => "add title",
            Self::CreateSection => "add section",
            Self::CreateTags => "add tags",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PreviewMode {
    Entry,
    CategoryPacket,
    CategoryOpenPacket,
    ProjectOpenPacket,
}

impl PreviewMode {
    const ALL: [Self; 4] = [
        Self::Entry,
        Self::CategoryPacket,
        Self::CategoryOpenPacket,
        Self::ProjectOpenPacket,
    ];

    fn next(self) -> Self {
        let index = Self::ALL
            .iter()
            .position(|candidate| *candidate == self)
            .unwrap_or(0);
        Self::ALL[(index + 1) % Self::ALL.len()]
    }

    fn label(self) -> &'static str {
        match self {
            Self::Entry => "entry preview",
            Self::CategoryPacket => "category packet",
            Self::CategoryOpenPacket => "open category packet",
            Self::ProjectOpenPacket => "open project packet",
        }
    }
}

#[derive(Clone, Debug)]
struct CreateDraft {
    category: Category,
    title: String,
    section: Option<String>,
    tags: BTreeSet<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum Overlay {
    Help,
    ConfirmDelete { id: String, title: String },
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum AppAction {
    OpenEntryInEditor { id: String, path: PathBuf },
}

struct App {
    project: Project,
    entries: Vec<Entry>,
    category_index: usize,
    selected: usize,
    mode: Mode,
    field_editor: TextArea<'static>,
    status: String,
    preview_mode: PreviewMode,
    preview_audience: Audience,
    preview_scroll: u16,
    create_draft: Option<CreateDraft>,
    overlay: Option<Overlay>,
    pending_action: Option<AppAction>,
}

impl App {
    fn new(project: Project) -> Result<Self> {
        let mut app = Self {
            project,
            entries: Vec::new(),
            category_index: 0,
            selected: 0,
            mode: Mode::Browse,
            field_editor: TextArea::default(),
            status: default_status_message().to_string(),
            preview_mode: PreviewMode::Entry,
            preview_audience: Audience::Agent,
            preview_scroll: 0,
            create_draft: None,
            overlay: None,
            pending_action: None,
        };
        app.refresh(None)?;
        Ok(app)
    }

    fn current_category(&self) -> Category {
        Category::ALL[self.category_index]
    }

    fn refresh(&mut self, preserve_id: Option<String>) -> Result<()> {
        self.entries = self.project.list_entries(&EntryFilter {
            include_archived: true,
            ..EntryFilter::default()
        })?;
        let visible = self.visible_indices();
        self.selected = if let Some(id) = preserve_id {
            visible
                .iter()
                .position(|index| self.entries[*index].id() == id)
                .unwrap_or_else(|| self.selected.min(visible.len().saturating_sub(1)))
        } else {
            self.selected.min(visible.len().saturating_sub(1))
        };
        self.preview_scroll = 0;
        Ok(())
    }

    fn visible_indices(&self) -> Vec<usize> {
        self.entries
            .iter()
            .enumerate()
            .filter(|(_, entry)| entry.category() == self.current_category())
            .map(|(index, _)| index)
            .collect()
    }

    fn current_category_entries(&self) -> Vec<Entry> {
        self.entries
            .iter()
            .filter(|entry| entry.category() == self.current_category())
            .cloned()
            .collect()
    }

    fn open_category_entries(&self) -> Vec<Entry> {
        self.entries
            .iter()
            .filter(|entry| entry.category() == self.current_category() && is_open_entry(entry))
            .cloned()
            .collect()
    }

    fn open_project_entries(&self) -> Vec<Entry> {
        self.entries
            .iter()
            .filter(|entry| is_open_entry(entry))
            .cloned()
            .collect()
    }

    fn selected_entry(&self) -> Option<&Entry> {
        let visible = self.visible_indices();
        visible
            .get(self.selected)
            .and_then(|entry_index| self.entries.get(*entry_index))
    }

    fn selected_entry_id(&self) -> Option<String> {
        self.selected_entry().map(|entry| entry.id().to_string())
    }

    fn total_entries(&self) -> usize {
        self.entries.len()
    }

    fn open_entry_count(&self) -> usize {
        self.entries
            .iter()
            .filter(|entry| is_open_entry(entry))
            .count()
    }

    fn category_counts(&self, category: Category) -> (usize, usize) {
        let total = self
            .entries
            .iter()
            .filter(|entry| entry.category() == category)
            .count();
        let open = self
            .entries
            .iter()
            .filter(|entry| entry.category() == category && is_open_entry(entry))
            .count();
        (open, total)
    }

    fn set_field_editor(&mut self, block_title: impl Into<String>, initial_value: String) {
        let block_title = block_title.into();
        self.field_editor = TextArea::from(vec![initial_value]);
        configure_text_area(&mut self.field_editor, block_title, INFO);
    }

    fn field_text_inline(&self) -> String {
        self.field_editor.lines().join(" ").trim().to_string()
    }

    fn field_text_multiline(&self) -> String {
        self.field_editor.lines().join("\n")
    }

    fn next_category(&mut self) {
        self.category_index = (self.category_index + 1) % Category::ALL.len();
        self.selected = 0;
        self.preview_scroll = 0;
    }

    fn previous_category(&mut self) {
        if self.category_index == 0 {
            self.category_index = Category::ALL.len() - 1;
        } else {
            self.category_index -= 1;
        }
        self.selected = 0;
        self.preview_scroll = 0;
    }

    fn move_down(&mut self) {
        let len = self.visible_indices().len();
        if len == 0 {
            self.selected = 0;
        } else {
            self.selected = (self.selected + 1).min(len - 1);
        }
        self.preview_scroll = 0;
    }

    fn move_up(&mut self) {
        self.selected = self.selected.saturating_sub(1);
        self.preview_scroll = 0;
    }

    fn start_title_edit(&mut self) {
        if let Some(title) = self.selected_entry().map(|entry| entry.title().to_string()) {
            self.set_field_editor(
                "Title Editor (Ctrl-S to save, Esc to cancel)",
                title.clone(),
            );
            self.mode = Mode::EditTitle;
            self.status = format!("Editing title for `{title}`");
        }
    }

    fn start_section_edit(&mut self) {
        let Some(entry) = self.selected_entry().cloned() else {
            return;
        };
        self.set_field_editor(
            "Section Editor (Ctrl-S to save, Esc to cancel)",
            entry.section().unwrap_or_default().to_string(),
        );
        self.mode = Mode::EditSection;
        self.status = format!("Editing section for `{}`", entry.title());
    }

    fn start_tags_edit(&mut self) {
        let Some(entry) = self.selected_entry().cloned() else {
            return;
        };
        self.set_field_editor(
            "Tags Editor (comma or newline separated)",
            tags_editor_value(&entry.front_matter.tags),
        );
        self.mode = Mode::EditTags;
        self.status = format!("Editing tags for `{}`", entry.title());
    }

    fn start_create(&mut self) {
        let category = self.current_category();
        self.create_draft = Some(CreateDraft {
            category,
            title: String::new(),
            section: None,
            tags: BTreeSet::new(),
        });
        self.set_field_editor(
            "Add Entry Title (Ctrl-S to continue, Esc to cancel)",
            String::new(),
        );
        self.mode = Mode::CreateTitle;
        self.status = format!("Add {} entry: set the title", category.dir_name());
    }

    fn toggle_help(&mut self) {
        if self.overlay == Some(Overlay::Help) {
            self.overlay = None;
            self.status = "Help closed".to_string();
        } else if self.overlay.is_none() {
            self.overlay = Some(Overlay::Help);
            self.status = "Help open: Esc, ?, or F1 closes the overlay".to_string();
        }
    }

    fn prompt_delete(&mut self) {
        let Some((id, title)) = self
            .selected_entry()
            .map(|entry| (entry.id().to_string(), entry.title().to_string()))
        else {
            self.status = "No entry selected to delete".to_string();
            return;
        };
        self.overlay = Some(Overlay::ConfirmDelete {
            id,
            title: title.clone(),
        });
        self.status = format!("Delete `{title}`? Press Enter to confirm.");
    }

    fn queue_open_entry_in_editor(&mut self, entry: Entry) {
        let id = entry.id().to_string();
        self.pending_action = Some(AppAction::OpenEntryInEditor {
            id: id.clone(),
            path: entry.path,
        });
        self.mode = Mode::Browse;
        self.status = format!("Opening `{id}` in external editor...");
    }

    fn request_open_selected_in_editor(&mut self) {
        let Some(entry) = self.selected_entry().cloned() else {
            self.status = "No entry selected to open in the editor".to_string();
            return;
        };
        self.queue_open_entry_in_editor(entry);
    }

    fn take_pending_action(&mut self) -> Option<AppAction> {
        self.pending_action.take()
    }

    fn confirm_delete(&mut self) -> Result<()> {
        let Some(Overlay::ConfirmDelete { id, title }) = self.overlay.clone() else {
            return Ok(());
        };
        self.project.delete_entry(&id)?;
        self.overlay = None;
        self.refresh(None)?;
        self.status = format!("Deleted `{title}`");
        Ok(())
    }

    fn close_overlay(&mut self, message: impl Into<String>) {
        self.overlay = None;
        self.status = message.into();
    }

    fn cycle_status(&mut self) -> Result<()> {
        let Some(entry) = self.selected_entry().cloned() else {
            return Ok(());
        };
        let id = entry.id().to_string();
        let next_status = entry.status().next();
        self.project.update_entry(
            &id,
            EntryUpdate {
                status: Some(next_status),
                ..EntryUpdate::default()
            },
        )?;
        self.refresh(Some(id))?;
        self.status = format!("Status set to `{next_status}`");
        Ok(())
    }

    fn cycle_preview_mode(&mut self) {
        self.preview_mode = self.preview_mode.next();
        self.preview_scroll = 0;
        self.status = format!(
            "Preview mode: {} [{}]",
            self.preview_mode.label(),
            self.preview_audience
        );
    }

    fn cycle_preview_audience(&mut self) {
        self.preview_audience = self.preview_audience.next();
        self.preview_scroll = 0;
        self.status = format!(
            "Preview audience: {} ({})",
            self.preview_audience,
            self.preview_mode.label()
        );
    }

    fn scroll_preview_down(&mut self) {
        self.preview_scroll = self.preview_scroll.saturating_add(4);
    }

    fn scroll_preview_up(&mut self) {
        self.preview_scroll = self.preview_scroll.saturating_sub(4);
    }

    fn copy_selected(&mut self) {
        if let Some(entry) = self.selected_entry() {
            match copy_to_clipboard(&render_clipboard_packet(
                &self.project,
                std::slice::from_ref(entry),
                self.preview_audience,
            )) {
                Ok(()) => {
                    self.status = format!(
                        "Copied `{}` packet to clipboard [{}]",
                        entry.id(),
                        self.preview_audience
                    );
                }
                Err(error) => {
                    self.status = format!("Clipboard error: {error}");
                }
            }
        }
    }

    fn copy_category(&mut self) {
        let visible = self
            .visible_indices()
            .into_iter()
            .filter_map(|index| self.entries.get(index).cloned())
            .collect::<Vec<_>>();
        match copy_to_clipboard(&render_clipboard_packet(
            &self.project,
            &visible,
            self.preview_audience,
        )) {
            Ok(()) => {
                self.status = format!(
                    "Copied {} packet for {} [{}]",
                    self.current_category().dir_name(),
                    self.project.config.name,
                    self.preview_audience
                );
            }
            Err(error) => {
                self.status = format!("Clipboard error: {error}");
            }
        }
    }

    fn save_editor(&mut self) -> Result<()> {
        match self.mode {
            Mode::Browse => {}
            Mode::EditTitle => {
                let Some(entry) = self.selected_entry().cloned() else {
                    return Ok(());
                };
                let title = self.field_text_inline();
                if title.is_empty() {
                    self.status = "Title cannot be empty".to_string();
                    return Ok(());
                }
                if title == entry.title() {
                    self.mode = Mode::Browse;
                    self.status = "Title unchanged".to_string();
                    return Ok(());
                }
                let id = entry.id().to_string();
                self.project.update_entry(
                    &id,
                    EntryUpdate {
                        title: Some(title),
                        ..EntryUpdate::default()
                    },
                )?;
                self.mode = Mode::Browse;
                self.refresh(Some(id.clone()))?;
                self.status = format!("Saved title for `{id}`");
            }
            Mode::EditSection => {
                let Some(entry) = self.selected_entry().cloned() else {
                    return Ok(());
                };
                let raw_section = self.field_text_inline();
                let next_section = normalize_section(&raw_section);
                let current_section = entry.section().map(str::to_string);
                if next_section == current_section {
                    self.mode = Mode::Browse;
                    self.status = "Section unchanged".to_string();
                    return Ok(());
                }
                let id = entry.id().to_string();
                self.project.update_entry(
                    &id,
                    EntryUpdate {
                        section: Some(raw_section),
                        ..EntryUpdate::default()
                    },
                )?;
                self.mode = Mode::Browse;
                self.refresh(Some(id.clone()))?;
                self.status = match next_section {
                    Some(section) => format!("Saved section `{section}` for `{id}`"),
                    None => format!("Cleared section for `{id}`"),
                };
            }
            Mode::EditTags => {
                let Some(entry) = self.selected_entry().cloned() else {
                    return Ok(());
                };
                let next_tags = parse_tags_input(&self.field_text_multiline());
                if next_tags == entry.front_matter.tags {
                    self.mode = Mode::Browse;
                    self.status = "Tags unchanged".to_string();
                    return Ok(());
                }
                let current_tags = entry.front_matter.tags.clone();
                let add_tags = next_tags
                    .difference(&current_tags)
                    .cloned()
                    .collect::<BTreeSet<_>>();
                let remove_tags = current_tags
                    .difference(&next_tags)
                    .cloned()
                    .collect::<BTreeSet<_>>();
                let id = entry.id().to_string();
                self.project.update_entry(
                    &id,
                    EntryUpdate {
                        add_tags,
                        remove_tags,
                        ..EntryUpdate::default()
                    },
                )?;
                self.mode = Mode::Browse;
                self.refresh(Some(id.clone()))?;
                self.status = format!("Saved tags for `{id}`");
            }
            Mode::CreateTitle => {
                let title = self.field_text_inline();
                if title.is_empty() {
                    self.status = "Title cannot be empty".to_string();
                    return Ok(());
                }
                if let Some(draft) = self.create_draft.as_mut() {
                    draft.title = title;
                }
                let section = self
                    .create_draft
                    .as_ref()
                    .and_then(|draft| draft.section.clone())
                    .unwrap_or_default();
                self.set_field_editor("Add Entry Section (optional, Ctrl-S to continue)", section);
                self.mode = Mode::CreateSection;
                self.status = format!(
                    "Add {} entry: set the section (optional)",
                    self.current_category().dir_name()
                );
            }
            Mode::CreateSection => {
                let section = normalize_section(&self.field_text_inline());
                if let Some(draft) = self.create_draft.as_mut() {
                    draft.section = section;
                }
                let existing_tags = self
                    .create_draft
                    .as_ref()
                    .map(|draft| tags_editor_value(&draft.tags))
                    .unwrap_or_default();
                self.set_field_editor("Add Entry Tags (comma or newline separated)", existing_tags);
                self.mode = Mode::CreateTags;
                self.status = format!(
                    "Add {} entry: set tags (optional)",
                    self.current_category().dir_name()
                );
            }
            Mode::CreateTags => {
                let tags = parse_tags_input(&self.field_text_multiline());
                let Some(mut draft) = self.create_draft.take() else {
                    self.mode = Mode::Browse;
                    return Ok(());
                };
                draft.tags = tags;
                let created = self.project.create_entry(NewEntry {
                    category: draft.category,
                    title: draft.title.clone(),
                    section: draft.section.clone(),
                    status: None,
                    tags: draft.tags,
                    body: Some(draft.category.placeholder_body(&draft.title)),
                })?;
                let preserve_id = Some(created.id().to_string());
                self.mode = Mode::Browse;
                self.refresh(preserve_id)?;
                self.queue_open_entry_in_editor(created);
            }
        }
        Ok(())
    }

    fn cancel_edit(&mut self) {
        if self.mode.is_create() {
            self.create_draft = None;
            self.mode = Mode::Browse;
            self.status = "Add entry cancelled".to_string();
            return;
        }
        self.mode = Mode::Browse;
        self.status = "Edit cancelled".to_string();
    }

    fn preview_entries(&self) -> Vec<Entry> {
        match self.preview_mode {
            PreviewMode::Entry => self.selected_entry().cloned().into_iter().collect(),
            PreviewMode::CategoryPacket => self.current_category_entries(),
            PreviewMode::CategoryOpenPacket => self.open_category_entries(),
            PreviewMode::ProjectOpenPacket => self.open_project_entries(),
        }
    }

    fn browse_preview_title(&self) -> String {
        match self.preview_mode {
            PreviewMode::Entry => " Entry Preview ".to_string(),
            PreviewMode::CategoryPacket => format!(
                " {} Packet [{}] ",
                self.current_category().dir_name(),
                self.preview_audience
            ),
            PreviewMode::CategoryOpenPacket => format!(
                " open {} Packet [{}] ",
                self.current_category().dir_name(),
                self.preview_audience
            ),
            PreviewMode::ProjectOpenPacket => {
                format!(" open Project Packet [{}] ", self.preview_audience)
            }
        }
    }

    fn browse_preview_contents(&self) -> String {
        match self.preview_mode {
            PreviewMode::Entry => self
                .selected_entry()
                .map(render_single_entry)
                .unwrap_or_else(|| "No entry selected.".to_string()),
            PreviewMode::CategoryPacket
            | PreviewMode::CategoryOpenPacket
            | PreviewMode::ProjectOpenPacket => render_packet(
                &self.project,
                &self.preview_entries(),
                self.preview_audience,
            ),
        }
    }

    fn field_preview_entry(&self, entry: &Entry) -> Entry {
        let mut preview = entry.clone();
        match self.mode {
            Mode::EditTitle => {
                preview.front_matter.title = self.field_text_inline();
            }
            Mode::EditSection => {
                preview.front_matter.section = normalize_section(&self.field_text_inline());
            }
            Mode::EditTags => {
                preview.front_matter.tags = parse_tags_input(&self.field_text_multiline());
            }
            _ => {}
        }
        preview
    }

    fn create_preview_entry(&self) -> Option<Entry> {
        let draft = self.create_draft.as_ref()?;
        let title = match self.mode {
            Mode::CreateTitle => {
                let live_title = self.field_text_inline();
                if live_title.is_empty() {
                    "Untitled entry".to_string()
                } else {
                    live_title
                }
            }
            _ => draft.title.clone(),
        };
        let section = match self.mode {
            Mode::CreateSection => normalize_section(&self.field_text_inline()),
            _ => draft.section.clone(),
        };
        let tags = match self.mode {
            Mode::CreateTags => parse_tags_input(&self.field_text_multiline()),
            _ => draft.tags.clone(),
        };
        let now = Utc::now();
        Some(Entry {
            front_matter: EntryFrontMatter {
                id: slugify(&title),
                title: title.clone(),
                category: draft.category,
                section,
                status: draft.category.default_status(),
                tags,
                created_at: now,
                updated_at: now,
            },
            body: draft.category.placeholder_body(&title),
            path: PathBuf::new(),
        })
    }

    fn handle_overlay_key(&mut self, key: KeyEvent) -> Result<bool> {
        match self.overlay.clone() {
            Some(Overlay::Help) => match key.code {
                KeyCode::Esc | KeyCode::F(1) | KeyCode::Char('?') => {
                    self.close_overlay("Help closed");
                }
                _ => {}
            },
            Some(Overlay::ConfirmDelete { .. }) => match key.code {
                KeyCode::Enter | KeyCode::Char('y') => self.confirm_delete()?,
                KeyCode::Esc | KeyCode::Char('n') => self.close_overlay("Delete cancelled"),
                _ => {}
            },
            None => {}
        }
        Ok(false)
    }

    fn handle_key(&mut self, key: KeyEvent) -> Result<bool> {
        if self.overlay.is_some() {
            return self.handle_overlay_key(key);
        }

        if key.code == KeyCode::F(1)
            || (self.mode == Mode::Browse && key.code == KeyCode::Char('?'))
        {
            self.toggle_help();
            return Ok(false);
        }

        if key.code == KeyCode::Char('q') && self.mode == Mode::Browse {
            return Ok(true);
        }

        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('s') {
            self.save_editor()?;
            return Ok(false);
        }

        match self.mode {
            Mode::Browse => match key.code {
                KeyCode::Tab => self.next_category(),
                KeyCode::BackTab => self.previous_category(),
                KeyCode::Char('j') | KeyCode::Down => self.move_down(),
                KeyCode::Char('k') | KeyCode::Up => self.move_up(),
                KeyCode::Enter | KeyCode::Char('e') => self.request_open_selected_in_editor(),
                KeyCode::Char('t') => self.start_title_edit(),
                KeyCode::Char('s') => self.start_section_edit(),
                KeyCode::Char('g') => self.start_tags_edit(),
                KeyCode::Char('n') | KeyCode::Char('+') => self.start_create(),
                KeyCode::Char('d') | KeyCode::Delete => self.prompt_delete(),
                KeyCode::Char('S') => self.cycle_status()?,
                KeyCode::Char('p') => self.cycle_preview_mode(),
                KeyCode::Char('a') => self.cycle_preview_audience(),
                KeyCode::Char('c') => self.copy_selected(),
                KeyCode::Char('C') => self.copy_category(),
                KeyCode::PageDown => self.scroll_preview_down(),
                KeyCode::PageUp => self.scroll_preview_up(),
                KeyCode::Char('r') => self.refresh(self.selected_entry_id())?,
                _ => {}
            },
            Mode::EditTitle
            | Mode::EditSection
            | Mode::EditTags
            | Mode::CreateTitle
            | Mode::CreateSection
            | Mode::CreateTags => {
                if key.code == KeyCode::Esc {
                    self.cancel_edit();
                } else {
                    self.field_editor.input(key);
                }
            }
        }

        Ok(false)
    }
}

fn run_loop(
    terminal: &mut Terminal<CrosstermBackend<std::io::Stdout>>,
    project: Project,
) -> Result<()> {
    let mut app = App::new(project)?;
    loop {
        terminal.draw(|frame| draw(frame, &mut app))?;
        if event::poll(Duration::from_millis(150))?
            && let Event::Key(key) = event::read()?
            && app.handle_key(key)?
        {
            break;
        }
        if let Some(action) = app.take_pending_action() {
            handle_app_action(terminal, &mut app, action)?;
        }
    }
    Ok(())
}

fn draw(frame: &mut Frame<'_>, app: &mut App) {
    frame.render_widget(
        Block::default().style(Style::default().bg(APP_BG)),
        frame.area(),
    );
    let layout = Layout::default()
        .margin(1)
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(5),
            Constraint::Length(4),
            Constraint::Min(10),
            Constraint::Length(4),
        ])
        .split(frame.area());

    draw_header(frame, layout[0], app);
    draw_category_cards(frame, layout[1], app);
    draw_body(frame, layout[2], app);
    draw_footer(frame, layout[3], app);
    draw_overlay(frame, app);
}

fn draw_header(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let header = Paragraph::new(vec![
        Line::from(vec![
            pill("cotext", ACCENT, APP_BG),
            Span::raw(" "),
            Span::styled(
                app.project.config.name.clone(),
                Style::default()
                    .fg(TEXT_BRIGHT)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw("  "),
            pill(
                format!("mode {}", app.mode.label()),
                category_color(app.current_category()),
                APP_BG,
            ),
            Span::raw(" "),
            pill(
                format!("preview {}", app.preview_mode.label()),
                INFO,
                APP_BG,
            ),
        ]),
        Line::from(Span::styled(
            format!("root: {}", app.project.root.display()),
            Style::default().fg(TEXT_MUTED),
        )),
        Line::from(vec![
            pill(
                format!("{} total", app.total_entries()),
                ACCENT_WARM,
                APP_BG,
            ),
            Span::raw(" "),
            pill(format!("{} open", app.open_entry_count()), SUCCESS, APP_BG),
            Span::raw(" "),
            pill(
                format!("{} visible", app.visible_indices().len()),
                INFO,
                APP_BG,
            ),
            Span::raw(" "),
            Span::styled(
                app.current_category().label(),
                Style::default()
                    .fg(TEXT_PRIMARY)
                    .add_modifier(Modifier::BOLD),
            ),
        ]),
    ])
    .block(panel_block("Context Board", ACCENT))
    .wrap(Wrap { trim: true });
    frame.render_widget(header, area);
}

fn draw_category_cards(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let constraints = vec![Constraint::Fill(1); Category::ALL.len()];
    let cards = Layout::default()
        .direction(Direction::Horizontal)
        .constraints(constraints)
        .split(area);
    for (index, category) in Category::ALL.iter().enumerate() {
        let (open, total) = app.category_counts(*category);
        let active = index == app.category_index;
        let accent = category_color(*category);
        let block = Block::default()
            .title(format!(" {} ", category.dir_name()))
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(if active { accent } else { PANEL_MUTED_BG }))
            .style(Style::default().bg(if active { PANEL_ALT_BG } else { PANEL_BG }));
        let paragraph = Paragraph::new(vec![
            Line::from(Span::styled(
                category.label(),
                Style::default()
                    .fg(if active { TEXT_BRIGHT } else { TEXT_PRIMARY })
                    .add_modifier(Modifier::BOLD),
            )),
            Line::from(Span::styled(
                format!("{open} open / {total} total"),
                Style::default().fg(TEXT_MUTED),
            )),
        ])
        .block(block)
        .alignment(Alignment::Center)
        .wrap(Wrap { trim: true });
        frame.render_widget(paragraph, cards[index]);
    }
}

fn draw_body(frame: &mut Frame<'_>, area: Rect, app: &mut App) {
    let columns = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(37), Constraint::Percentage(63)])
        .split(area);
    draw_entry_list(frame, columns[0], app);
    draw_detail(frame, columns[1], app);
}

fn draw_entry_list(frame: &mut Frame<'_>, area: Rect, app: &mut App) {
    let visible = app.visible_indices();
    let items = visible
        .iter()
        .enumerate()
        .filter_map(|(row, index)| app.entries.get(*index).map(|entry| (row, entry)))
        .map(|(row, entry)| {
            let mut lines = vec![Line::from(Span::styled(
                entry.title().to_string(),
                Style::default().add_modifier(Modifier::BOLD),
            ))];
            let mut meta = vec![status_badge(entry.status()), Span::raw(" ")];
            meta.push(Span::styled(
                entry.section().unwrap_or("(root)").to_string(),
                Style::default().fg(TEXT_MUTED),
            ));
            if let Some(tags) = compact_tag_summary(&entry.front_matter.tags) {
                meta.push(Span::styled(
                    format!("  |  {tags}"),
                    Style::default().fg(INFO),
                ));
            }
            lines.push(Line::from(meta));
            ListItem::new(lines).style(Style::default().bg(if row % 2 == 0 {
                PANEL_BG
            } else {
                PANEL_ALT_BG
            }))
        })
        .collect::<Vec<_>>();

    let mut state = ListState::default();
    if !items.is_empty() {
        state.select(Some(app.selected));
    }
    let list = List::new(items)
        .block(panel_block(
            format!(
                "{} entries [{}]",
                app.current_category().dir_name(),
                visible.len()
            ),
            category_color(app.current_category()),
        ))
        .highlight_style(
            Style::default()
                .bg(Color::Rgb(14, 116, 144))
                .fg(TEXT_BRIGHT)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol(">> ");
    frame.render_stateful_widget(list, area, &mut state);
}

fn draw_detail(frame: &mut Frame<'_>, area: Rect, app: &mut App) {
    if app.mode.is_create() {
        draw_create_detail(frame, area, app);
        return;
    }

    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(8), Constraint::Min(8)])
        .split(area);
    draw_selected_meta(frame, rows[0], app, app.selected_entry());

    match app.mode {
        Mode::Browse => draw_browse_preview(frame, rows[1], app),
        Mode::EditTitle | Mode::EditSection | Mode::EditTags => {
            let split = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Length(5), Constraint::Min(3)])
                .split(rows[1]);
            frame.render_widget(&app.field_editor, split[0]);
            let preview = app
                .selected_entry()
                .map(|entry| render_single_entry(&app.field_preview_entry(entry)))
                .unwrap_or_else(|| "No entry selected.".to_string());
            let preview_widget = Paragraph::new(preview)
                .block(panel_block("Preview", INFO))
                .wrap(Wrap { trim: false });
            frame.render_widget(preview_widget, split[1]);
        }
        Mode::CreateTitle | Mode::CreateSection | Mode::CreateTags => {}
    }
}

fn draw_selected_meta(frame: &mut Frame<'_>, area: Rect, app: &App, selected: Option<&Entry>) {
    let tags = selected
        .map(|entry| format_tags(&entry.front_matter.tags))
        .unwrap_or_else(|| "none".to_string());
    let title = selected
        .map(|entry| entry.title().to_string())
        .unwrap_or_else(|| format!("No {} entries yet", app.current_category().dir_name()));
    let status_badge = selected
        .map(|entry| entry.status().to_string())
        .unwrap_or_else(|| "empty".to_string());
    let id = selected
        .map(|entry| entry.id().to_string())
        .unwrap_or_else(|| "-".to_string());
    let section = selected
        .and_then(Entry::section)
        .unwrap_or("(root)")
        .to_string();
    let updated = selected
        .map(|entry| {
            entry
                .front_matter
                .updated_at
                .format("%Y-%m-%d %H:%M UTC")
                .to_string()
        })
        .unwrap_or_else(|| "-".to_string());
    let meta = Paragraph::new(vec![
        Line::from(vec![
            Span::styled(
                title,
                Style::default()
                    .fg(TEXT_BRIGHT)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw("  "),
            pill(
                status_badge,
                status_color(selected.map(Entry::status).unwrap_or(EntryStatus::Draft)),
                APP_BG,
            ),
            Span::raw(" "),
            pill(
                app.current_category().dir_name(),
                category_color(app.current_category()),
                APP_BG,
            ),
        ]),
        Line::from(vec![
            Span::styled("id: ", Style::default().fg(TEXT_MUTED)),
            Span::styled(id, Style::default().fg(TEXT_PRIMARY)),
        ]),
        Line::from(vec![
            Span::styled("section: ", Style::default().fg(TEXT_MUTED)),
            Span::styled(section, Style::default().fg(TEXT_PRIMARY)),
        ]),
        Line::from(vec![
            Span::styled("tags: ", Style::default().fg(TEXT_MUTED)),
            Span::styled(tags, Style::default().fg(INFO)),
        ]),
        Line::from(vec![
            Span::styled("updated: ", Style::default().fg(TEXT_MUTED)),
            Span::styled(updated, Style::default().fg(TEXT_PRIMARY)),
        ]),
        Line::from(vec![
            Span::styled("preview: ", Style::default().fg(TEXT_MUTED)),
            Span::styled(
                format!("{} [{}]", app.preview_mode.label(), app.preview_audience),
                Style::default().fg(TEXT_PRIMARY),
            ),
        ]),
    ])
    .block(panel_block(
        "Selected Entry",
        category_color(app.current_category()),
    ))
    .wrap(Wrap { trim: true });
    frame.render_widget(meta, area);
}

fn draw_browse_preview(frame: &mut Frame<'_>, area: Rect, app: &App) {
    if app.preview_mode == PreviewMode::Entry && app.selected_entry().is_none() {
        draw_empty_detail(frame, area);
        return;
    }

    let preview = Paragraph::new(app.browse_preview_contents())
        .block(panel_block(app.browse_preview_title(), INFO))
        .scroll((app.preview_scroll, 0))
        .wrap(Wrap { trim: false });
    frame.render_widget(preview, area);
}

fn draw_create_detail(frame: &mut Frame<'_>, area: Rect, app: &mut App) {
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(8), Constraint::Min(8)])
        .split(area);

    let Some(preview_entry) = app.create_preview_entry() else {
        draw_empty_detail(frame, area);
        return;
    };

    let meta = Paragraph::new(vec![
        Line::from(vec![
            Span::styled(
                format!(
                    "Add {} entry",
                    preview_entry.front_matter.category.dir_name()
                ),
                Style::default()
                    .fg(TEXT_BRIGHT)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw("  "),
            status_badge(preview_entry.status()),
        ]),
        Line::from(format!("id preview: {}", preview_entry.id())),
        Line::from(format!(
            "section: {}",
            preview_entry.section().unwrap_or("(root)")
        )),
        Line::from(format!(
            "tags: {}",
            format_tags(&preview_entry.front_matter.tags)
        )),
        Line::from(format!("prompt: {}", app.mode.label())),
        Line::from("flow: title -> section -> tags -> body"),
    ])
    .block(panel_block("Add Entry", ACCENT_WARM))
    .wrap(Wrap { trim: true });
    frame.render_widget(meta, rows[0]);

    let split = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(5), Constraint::Min(3)])
        .split(rows[1]);
    frame.render_widget(&app.field_editor, split[0]);
    let preview = Paragraph::new(render_single_entry(&preview_entry))
        .block(panel_block("Preview", INFO))
        .wrap(Wrap { trim: false });
    frame.render_widget(preview, split[1]);
}

fn draw_empty_detail(frame: &mut Frame<'_>, area: Rect) {
    let empty = Paragraph::new("No entries in this category yet. Press `n` or `+` to add one.")
        .block(panel_block("Empty", PANEL_MUTED_BG))
        .wrap(Wrap { trim: true });
    frame.render_widget(empty, area);
}

fn draw_footer(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let footer = Paragraph::new(vec![
        Line::from(footer_hint_spans(app)),
        Line::from(vec![
            pill("status", ACCENT_WARM, APP_BG),
            Span::raw(" "),
            Span::styled(app.status.clone(), Style::default().fg(TEXT_PRIMARY)),
        ]),
    ])
    .block(panel_block("Commands", PANEL_MUTED_BG))
    .wrap(Wrap { trim: true });
    frame.render_widget(footer, area);
}

fn draw_overlay(frame: &mut Frame<'_>, app: &App) {
    match app.overlay.as_ref() {
        Some(Overlay::Help) => draw_help_popup(frame, app),
        Some(Overlay::ConfirmDelete { id, title }) => draw_delete_popup(frame, id, title),
        None => {}
    }
}

fn draw_help_popup(frame: &mut Frame<'_>, app: &App) {
    let popup = centered_rect(frame.area(), 96, 22);
    frame.render_widget(Clear, popup);
    let block = modal_block("Quick Help", ACCENT);
    let inner = block.inner(popup);
    frame.render_widget(block, popup);

    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(4),
            Constraint::Min(8),
            Constraint::Length(3),
        ])
        .split(inner);

    let summary = Paragraph::new(vec![
        Line::from(vec![
            pill(format!("mode {}", app.mode.label()), category_color(app.current_category()), APP_BG),
            Span::raw(" "),
            pill(
                format!("category {}", app.current_category().dir_name()),
                category_color(app.current_category()),
                APP_BG,
            ),
            Span::raw(" "),
            pill(
                format!("preview {} [{}]", app.preview_mode.label(), app.preview_audience),
                INFO,
                APP_BG,
            ),
        ]),
        Line::from(Span::styled(
            "Use F1 anywhere to toggle help. The board underneath stays intact while the overlay is open.",
            Style::default().fg(TEXT_MUTED),
        )),
    ])
    .block(panel_block("Session", PANEL_MUTED_BG))
    .wrap(Wrap { trim: true });
    frame.render_widget(summary, rows[0]);

    let body_direction = if rows[1].width < 88 {
        Direction::Vertical
    } else {
        Direction::Horizontal
    };
    let body = Layout::default()
        .direction(body_direction)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(rows[1]);

    let browse_help = Paragraph::new(vec![
        Line::from(vec![
            keycap("Tab"),
            Span::styled(" switch category", Style::default().fg(TEXT_MUTED)),
        ]),
        Line::from(vec![
            keycap("j/k"),
            Span::styled(" move selection", Style::default().fg(TEXT_MUTED)),
        ]),
        Line::from(vec![
            keycap("n/+"),
            Span::styled(
                " add entry in the current category",
                Style::default().fg(TEXT_MUTED),
            ),
        ]),
        Line::from(vec![
            keycap("d"),
            Span::styled(
                " delete the selected entry",
                Style::default().fg(TEXT_MUTED),
            ),
        ]),
        Line::from(vec![
            keycap("S"),
            Span::styled(" cycle entry status", Style::default().fg(TEXT_MUTED)),
        ]),
        Line::from(vec![
            keycap("r"),
            Span::styled(
                " refresh entries from disk",
                Style::default().fg(TEXT_MUTED),
            ),
        ]),
        Line::from(vec![
            keycap("q"),
            Span::styled(" quit from browse mode", Style::default().fg(TEXT_MUTED)),
        ]),
    ])
    .block(panel_block(
        "Browse and Organize",
        category_color(app.current_category()),
    ))
    .wrap(Wrap { trim: true });
    frame.render_widget(browse_help, body[0]);

    let write_help = Paragraph::new(vec![
        Line::from(vec![
            keycap("Enter/e"),
            Span::styled(
                " open the selected entry in your editor",
                Style::default().fg(TEXT_MUTED),
            ),
        ]),
        Line::from(vec![
            keycap("t/s/g"),
            Span::styled(
                " edit title, section, and tags in the TUI",
                Style::default().fg(TEXT_MUTED),
            ),
        ]),
        Line::from(vec![
            keycap("Ctrl-S"),
            Span::styled(
                " save metadata edits or advance the add-entry flow",
                Style::default().fg(TEXT_MUTED),
            ),
        ]),
        Line::from(vec![
            keycap("Esc"),
            Span::styled(
                " cancel the current editor or popup",
                Style::default().fg(TEXT_MUTED),
            ),
        ]),
        Line::from(vec![
            keycap("p/a"),
            Span::styled(
                " change preview scope and audience",
                Style::default().fg(TEXT_MUTED),
            ),
        ]),
        Line::from(vec![
            keycap("PgUp/PgDn"),
            Span::styled(" scroll long previews", Style::default().fg(TEXT_MUTED)),
        ]),
        Line::from(vec![
            keycap("c/C"),
            Span::styled(
                " copy the selected or current-category packet",
                Style::default().fg(TEXT_MUTED),
            ),
        ]),
    ])
    .block(panel_block("Edit and Preview", INFO))
    .wrap(Wrap { trim: true });
    frame.render_widget(write_help, body[1]);

    let tip = Paragraph::new(vec![
        Line::from(Span::styled(
            "Add flow: title -> section -> tags -> editor. Delete removes the markdown file from .cotext/entries and prunes empty section directories.",
            Style::default().fg(TEXT_PRIMARY),
        )),
        Line::from(Span::styled(
            "Close help with Esc, ?, or F1.",
            Style::default().fg(TEXT_MUTED),
        )),
    ])
    .block(panel_block("Tips", ACCENT_WARM))
    .wrap(Wrap { trim: true });
    frame.render_widget(tip, rows[2]);
}

fn draw_delete_popup(frame: &mut Frame<'_>, id: &str, title: &str) {
    let popup = centered_rect(frame.area(), 68, 10);
    frame.render_widget(Clear, popup);
    let body = Paragraph::new(vec![
        Line::from(vec![
            pill("delete", DANGER, APP_BG),
            Span::raw(" "),
            Span::styled(
                "This removes the entry from disk.",
                Style::default().fg(TEXT_PRIMARY),
            ),
        ]),
        Line::from(vec![
            Span::styled("title: ", Style::default().fg(TEXT_MUTED)),
            Span::styled(
                title.to_string(),
                Style::default()
                    .fg(TEXT_BRIGHT)
                    .add_modifier(Modifier::BOLD),
            ),
        ]),
        Line::from(vec![
            Span::styled("id: ", Style::default().fg(TEXT_MUTED)),
            Span::styled(id.to_string(), Style::default().fg(TEXT_PRIMARY)),
        ]),
        Line::from(""),
        Line::from(vec![
            keycap("Enter"),
            Span::styled(" confirm", Style::default().fg(TEXT_MUTED)),
            Span::raw("  "),
            keycap("Esc"),
            Span::styled(" cancel", Style::default().fg(TEXT_MUTED)),
        ]),
    ])
    .block(modal_block("Delete Entry", DANGER))
    .wrap(Wrap { trim: true });
    frame.render_widget(body, popup);
}

fn panel_block(title: impl Into<String>, border_color: Color) -> Block<'static> {
    Block::default()
        .title(format!(" {} ", title.into()))
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(border_color))
        .style(Style::default().fg(TEXT_PRIMARY).bg(PANEL_BG))
}

fn modal_block(title: impl Into<String>, border_color: Color) -> Block<'static> {
    Block::default()
        .title(format!(" {} ", title.into()))
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(border_color))
        .style(Style::default().fg(TEXT_PRIMARY).bg(PANEL_ALT_BG))
}

fn configure_text_area(
    editor: &mut TextArea<'static>,
    title: impl Into<String>,
    border_color: Color,
) {
    editor.set_style(Style::default().fg(TEXT_PRIMARY).bg(PANEL_ALT_BG));
    editor.set_cursor_style(Style::default().fg(APP_BG).bg(ACCENT_WARM));
    editor.set_cursor_line_style(Style::default().bg(Color::Rgb(27, 45, 72)));
    editor.set_block(modal_block(title, border_color));
}

fn category_color(category: Category) -> Color {
    match category {
        Category::Design => INFO,
        Category::Note => ACCENT,
        Category::Progress => SUCCESS,
        Category::Todo => ACCENT_WARM,
        Category::Deferred => DANGER,
    }
}

fn status_color(status: EntryStatus) -> Color {
    match status {
        EntryStatus::Draft => TEXT_MUTED,
        EntryStatus::Active => ACCENT,
        EntryStatus::Planned => INFO,
        EntryStatus::Blocked => DANGER,
        EntryStatus::Done => SUCCESS,
        EntryStatus::Deferred => WARNING,
        EntryStatus::Archived => PANEL_MUTED_BG,
    }
}

fn pill(label: impl Into<String>, bg: Color, fg: Color) -> Span<'static> {
    Span::styled(
        format!(" {} ", label.into()),
        Style::default().bg(bg).fg(fg).add_modifier(Modifier::BOLD),
    )
}

fn keycap(label: impl Into<String>) -> Span<'static> {
    Span::styled(
        format!(" {} ", label.into()),
        Style::default()
            .bg(PANEL_MUTED_BG)
            .fg(TEXT_BRIGHT)
            .add_modifier(Modifier::BOLD),
    )
}

fn status_badge(status: EntryStatus) -> Span<'static> {
    pill(status.badge(), status_color(status), APP_BG)
}

fn footer_hint_spans(app: &App) -> Vec<Span<'static>> {
    let hints = match app.overlay.as_ref() {
        Some(Overlay::Help) => vec![("Esc", "close help"), ("F1", "toggle help")],
        Some(Overlay::ConfirmDelete { .. }) => {
            vec![("Enter", "confirm delete"), ("Esc", "cancel")]
        }
        None => match app.mode {
            Mode::Browse => vec![
                ("Tab", "category"),
                ("j/k", "move"),
                ("Enter", "edit"),
                ("n/+", "add"),
                ("d", "delete"),
                ("?", "help"),
                ("p/a", "preview"),
                ("q", "quit"),
            ],
            Mode::EditTitle | Mode::EditSection | Mode::EditTags => {
                vec![("Ctrl-S", "save"), ("Esc", "cancel"), ("F1", "help")]
            }
            Mode::CreateTitle | Mode::CreateSection | Mode::CreateTags => {
                vec![("Ctrl-S", "continue"), ("Esc", "cancel"), ("F1", "help")]
            }
        },
    };

    let mut spans = Vec::new();
    for (index, (key, label)) in hints.into_iter().enumerate() {
        if index > 0 {
            spans.push(Span::raw("  "));
        }
        spans.push(keycap(key));
        spans.push(Span::styled(
            format!(" {label}"),
            Style::default().fg(TEXT_MUTED),
        ));
    }
    spans
}

fn compact_tag_summary(tags: &BTreeSet<String>) -> Option<String> {
    if tags.is_empty() {
        return None;
    }

    let mut preview = tags.iter().take(2).cloned().collect::<Vec<_>>();
    let remaining = tags.len().saturating_sub(preview.len());
    if remaining > 0 {
        preview.push(format!("+{remaining}"));
    }
    Some(preview.join(", "))
}

fn centered_rect(area: Rect, width: u16, height: u16) -> Rect {
    let width = width.min(area.width.saturating_sub(2)).max(1);
    let height = height.min(area.height.saturating_sub(2)).max(1);
    let x = area.x + area.width.saturating_sub(width) / 2;
    let y = area.y + area.height.saturating_sub(height) / 2;
    Rect::new(x, y, width, height)
}

fn default_status_message() -> &'static str {
    "Browse the current category, press Enter to edit, or ? for help."
}

fn is_open_entry(entry: &Entry) -> bool {
    !matches!(entry.status(), EntryStatus::Done | EntryStatus::Archived)
}

fn format_tags(tags: &BTreeSet<String>) -> String {
    if tags.is_empty() {
        "none".to_string()
    } else {
        tags.iter().cloned().collect::<Vec<_>>().join(", ")
    }
}

fn tags_editor_value(tags: &BTreeSet<String>) -> String {
    tags.iter().cloned().collect::<Vec<_>>().join(", ")
}

fn parse_tags_input(input: &str) -> BTreeSet<String> {
    input
        .split([',', '\n'])
        .map(str::trim)
        .filter(|tag| !tag.is_empty())
        .map(ToOwned::to_owned)
        .collect()
}

fn copy_to_clipboard(contents: &str) -> Result<()> {
    let mut clipboard = Clipboard::new()?;
    clipboard.set_text(contents.to_string())?;
    Ok(())
}

enum ExternalEditOutcome {
    Unchanged { id: String },
    Updated { entry: Entry },
}

fn handle_app_action(
    terminal: &mut Terminal<CrosstermBackend<std::io::Stdout>>,
    app: &mut App,
    action: AppAction,
) -> Result<()> {
    match action {
        AppAction::OpenEntryInEditor { id, path } => {
            match open_entry_in_external_editor(terminal, &app.project, &id, &path) {
                Ok(ExternalEditOutcome::Unchanged { id }) => {
                    app.status = format!("Closed editor for `{id}` without changes");
                }
                Ok(ExternalEditOutcome::Updated { entry }) => {
                    let updated_id = entry.id().to_string();
                    match app.refresh(Some(updated_id.clone())) {
                        Ok(()) => {
                            app.status = format!("Updated `{updated_id}` from the external editor");
                        }
                        Err(error) => {
                            app.status =
                                format!("Edited `{updated_id}`, but refresh failed: {error}");
                        }
                    }
                }
                Err(error) => {
                    app.status = format!("Editor error for `{id}`: {error}");
                }
            }
        }
    }
    Ok(())
}

fn open_entry_in_external_editor(
    terminal: &mut Terminal<CrosstermBackend<std::io::Stdout>>,
    project: &Project,
    id: &str,
    path: &Path,
) -> Result<ExternalEditOutcome> {
    let original_raw =
        fs::read_to_string(path).with_context(|| format!("failed to read {}", path.display()))?;
    suspend_terminal(terminal)?;
    let editor_result = launch_external_editor(path);
    let resume_result = resume_terminal(terminal);
    resume_result?;
    editor_result?;

    let current_raw =
        fs::read_to_string(path).with_context(|| format!("failed to read {}", path.display()))?;
    if current_raw == original_raw {
        return Ok(ExternalEditOutcome::Unchanged { id: id.to_string() });
    }

    let entry = project.reconcile_edited_entry(path)?;
    Ok(ExternalEditOutcome::Updated { entry })
}

fn suspend_terminal(terminal: &mut Terminal<CrosstermBackend<std::io::Stdout>>) -> Result<()> {
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;
    Ok(())
}

fn resume_terminal(terminal: &mut Terminal<CrosstermBackend<std::io::Stdout>>) -> Result<()> {
    enable_raw_mode()?;
    execute!(terminal.backend_mut(), EnterAlternateScreen)?;
    terminal.clear()?;
    Ok(())
}

fn launch_external_editor(path: &Path) -> Result<()> {
    let status = Command::new("sh")
        .arg("-c")
        .arg("exec ${VISUAL:-${EDITOR:-vi}} \"$1\"")
        .arg("cotext-editor")
        .arg(path)
        .status()
        .with_context(|| format!("failed to launch editor for {}", path.display()))?;
    if !status.success() {
        bail!("external editor exited with status {status}");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use tempfile::TempDir;

    use super::*;

    fn seeded_project() -> Result<Project> {
        let temp = TempDir::new()?;
        let root = temp.path().to_path_buf();
        std::mem::forget(temp);
        Project::init(&root, Some("demo".to_string()), false)
    }

    fn replace_field(app: &mut App, value: &str) {
        app.field_editor = TextArea::from(vec![value.to_string()]);
    }

    #[test]
    fn section_edit_updates_entry_section() -> Result<()> {
        let project = seeded_project()?;
        project.create_entry(NewEntry {
            category: Category::Todo,
            title: "Ship metadata editor".to_string(),
            section: Some("tui".to_string()),
            status: Some(EntryStatus::Planned),
            tags: BTreeSet::new(),
            body: Some("Body".to_string()),
        })?;

        let mut app = App::new(project)?;
        app.category_index = 3;
        app.start_section_edit();
        replace_field(&mut app, "tui/roadmap");
        app.save_editor()?;

        assert_eq!(
            app.selected_entry().and_then(Entry::section),
            Some("tui/roadmap")
        );
        Ok(())
    }

    #[test]
    fn tags_edit_replaces_full_tag_set() -> Result<()> {
        let project = seeded_project()?;
        project.create_entry(NewEntry {
            category: Category::Todo,
            title: "Ship metadata editor".to_string(),
            section: Some("tui".to_string()),
            status: Some(EntryStatus::Planned),
            tags: BTreeSet::from(["old".to_string(), "keep".to_string()]),
            body: Some("Body".to_string()),
        })?;

        let mut app = App::new(project)?;
        app.category_index = 3;
        app.start_tags_edit();
        replace_field(&mut app, "keep, new-tag");
        app.save_editor()?;

        assert_eq!(
            app.selected_entry()
                .map(|entry| entry.front_matter.tags.clone()),
            Some(BTreeSet::from(["keep".to_string(), "new-tag".to_string()]))
        );
        Ok(())
    }

    #[test]
    fn quick_create_collects_metadata_before_creating_entry() -> Result<()> {
        let project = seeded_project()?;
        let mut app = App::new(project)?;
        app.category_index = 3;

        app.start_create();
        replace_field(&mut app, "Roadmap item");
        app.save_editor()?;
        assert_eq!(app.mode, Mode::CreateSection);

        replace_field(&mut app, "tui/roadmap");
        app.save_editor()?;
        assert_eq!(app.mode, Mode::CreateTags);

        replace_field(&mut app, "tui, ux");
        app.save_editor()?;

        let entry = app.selected_entry().cloned().expect("new entry selected");
        assert_eq!(app.mode, Mode::Browse);
        assert_eq!(entry.id(), "roadmap-item");
        assert_eq!(entry.title(), "Roadmap item");
        assert_eq!(entry.section(), Some("tui/roadmap"));
        assert_eq!(
            entry.front_matter.tags,
            BTreeSet::from(["tui".to_string(), "ux".to_string()])
        );
        assert!(matches!(
            app.take_pending_action(),
            Some(AppAction::OpenEntryInEditor { id, .. }) if id == "roadmap-item"
        ));
        Ok(())
    }

    #[test]
    fn open_category_preview_filters_done_entries() -> Result<()> {
        let project = seeded_project()?;
        project.create_entry(NewEntry {
            category: Category::Todo,
            title: "Keep me open".to_string(),
            section: None,
            status: Some(EntryStatus::Planned),
            tags: BTreeSet::new(),
            body: Some("Body".to_string()),
        })?;
        project.create_entry(NewEntry {
            category: Category::Todo,
            title: "Hide me done".to_string(),
            section: None,
            status: Some(EntryStatus::Done),
            tags: BTreeSet::new(),
            body: Some("Body".to_string()),
        })?;

        let mut app = App::new(project)?;
        app.category_index = 3;
        app.preview_mode = PreviewMode::CategoryOpenPacket;

        let preview_titles = app
            .preview_entries()
            .into_iter()
            .map(|entry| entry.title().to_string())
            .collect::<Vec<_>>();
        assert_eq!(preview_titles, vec!["Keep me open".to_string()]);
        Ok(())
    }

    #[test]
    fn plus_shortcut_starts_add_entry_flow() -> Result<()> {
        let project = seeded_project()?;
        let mut app = App::new(project)?;
        app.category_index = 3;

        app.handle_key(KeyEvent::new(KeyCode::Char('+'), KeyModifiers::NONE))?;

        assert_eq!(app.mode, Mode::CreateTitle);
        assert!(app.create_draft.is_some());
        Ok(())
    }

    #[test]
    fn delete_confirmation_removes_selected_entry() -> Result<()> {
        let project = seeded_project()?;
        project.create_entry(NewEntry {
            category: Category::Todo,
            title: "Keep me".to_string(),
            section: None,
            status: Some(EntryStatus::Planned),
            tags: BTreeSet::new(),
            body: Some("Body".to_string()),
        })?;
        project.create_entry(NewEntry {
            category: Category::Todo,
            title: "Delete me".to_string(),
            section: None,
            status: Some(EntryStatus::Planned),
            tags: BTreeSet::new(),
            body: Some("Body".to_string()),
        })?;

        let mut app = App::new(project)?;
        app.category_index = 3;

        app.handle_key(KeyEvent::new(KeyCode::Char('d'), KeyModifiers::NONE))?;
        assert_eq!(
            app.overlay,
            Some(Overlay::ConfirmDelete {
                id: "delete-me".to_string(),
                title: "Delete me".to_string(),
            })
        );

        app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE))?;

        let titles = app
            .visible_indices()
            .into_iter()
            .filter_map(|index| app.entries.get(index))
            .map(|entry| entry.title().to_string())
            .collect::<Vec<_>>();
        assert_eq!(titles, vec!["Keep me".to_string()]);
        assert!(app.overlay.is_none());
        Ok(())
    }

    #[test]
    fn help_overlay_toggles_from_browse() -> Result<()> {
        let project = seeded_project()?;
        let mut app = App::new(project)?;

        app.handle_key(KeyEvent::new(KeyCode::Char('?'), KeyModifiers::NONE))?;
        assert_eq!(app.overlay, Some(Overlay::Help));

        app.handle_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE))?;
        assert!(app.overlay.is_none());
        Ok(())
    }

    #[test]
    fn enter_shortcut_queues_external_editor_for_selected_entry() -> Result<()> {
        let project = seeded_project()?;
        project.create_entry(NewEntry {
            category: Category::Todo,
            title: "Edit me".to_string(),
            section: Some("tui".to_string()),
            status: Some(EntryStatus::Planned),
            tags: BTreeSet::new(),
            body: Some("Body".to_string()),
        })?;

        let mut app = App::new(project)?;
        app.category_index = 3;

        app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE))?;

        assert!(matches!(
            app.take_pending_action(),
            Some(AppAction::OpenEntryInEditor { id, .. }) if id == "edit-me"
        ));
        Ok(())
    }
}
