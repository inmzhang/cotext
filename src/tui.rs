use std::collections::BTreeSet;
use std::path::PathBuf;
use std::time::Duration;

use anyhow::Result;
use arboard::Clipboard;
use chrono::Utc;
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyModifiers};
use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, List, ListItem, ListState, Paragraph, Wrap};
use ratatui::{Frame, Terminal};
use tui_textarea::TextArea;

use crate::model::{
    Audience, Category, Entry, EntryFilter, EntryFrontMatter, EntryStatus, EntryUpdate, NewEntry,
    normalize_section,
};
use crate::render::{render_clipboard_packet, render_packet, render_single_entry};
use crate::storage::{Project, slugify};

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
    EditBody,
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
            Self::EditBody => "body",
            Self::EditTitle => "title",
            Self::EditSection => "section",
            Self::EditTags => "tags",
            Self::CreateTitle => "new title",
            Self::CreateSection => "new section",
            Self::CreateTags => "new tags",
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

struct App {
    project: Project,
    entries: Vec<Entry>,
    category_index: usize,
    selected: usize,
    mode: Mode,
    body_editor: TextArea<'static>,
    field_editor: TextArea<'static>,
    status: String,
    preview_mode: PreviewMode,
    preview_audience: Audience,
    preview_scroll: u16,
    create_draft: Option<CreateDraft>,
}

impl App {
    fn new(project: Project) -> Result<Self> {
        let mut app = Self {
            project,
            entries: Vec::new(),
            category_index: 0,
            selected: 0,
            mode: Mode::Browse,
            body_editor: TextArea::default(),
            field_editor: TextArea::default(),
            status: default_status_message().to_string(),
            preview_mode: PreviewMode::Entry,
            preview_audience: Audience::Agent,
            preview_scroll: 0,
            create_draft: None,
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

    fn set_field_editor(&mut self, block_title: impl Into<String>, initial_value: String) {
        let block_title = block_title.into();
        self.field_editor = TextArea::from(vec![initial_value]);
        self.field_editor.set_block(
            Block::default()
                .title(block_title)
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded),
        );
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

    fn start_body_edit(&mut self) {
        if let Some((title, body)) = self
            .selected_entry()
            .map(|entry| (entry.title().to_string(), entry.body.clone()))
        {
            self.body_editor = TextArea::from(body.lines());
            self.body_editor.set_block(
                Block::default()
                    .title("Body Editor (Ctrl-S to save, Esc to cancel)")
                    .borders(Borders::ALL)
                    .border_type(BorderType::Rounded),
            );
            self.mode = Mode::EditBody;
            self.status = format!("Editing body for `{title}`");
        }
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
            "New Entry Title (Ctrl-S to continue, Esc to cancel)",
            String::new(),
        );
        self.mode = Mode::CreateTitle;
        self.status = format!("New {} entry: set the title", category.dir_name());
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
                &[entry.clone()],
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
            Mode::EditBody => {
                let Some(entry) = self.selected_entry().cloned() else {
                    return Ok(());
                };
                let body = self.body_editor.lines().join("\n");
                if body == entry.body {
                    self.mode = Mode::Browse;
                    self.status = "Body unchanged".to_string();
                    return Ok(());
                }
                let id = entry.id().to_string();
                self.project.update_entry(
                    &id,
                    EntryUpdate {
                        body: Some(body),
                        ..EntryUpdate::default()
                    },
                )?;
                self.mode = Mode::Browse;
                self.refresh(Some(id.clone()))?;
                self.status = format!("Saved body for `{id}`");
            }
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
                self.set_field_editor("New Entry Section (optional, Ctrl-S to continue)", section);
                self.mode = Mode::CreateSection;
                self.status = format!(
                    "New {} entry: set the section (optional)",
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
                self.set_field_editor("New Entry Tags (comma or newline separated)", existing_tags);
                self.mode = Mode::CreateTags;
                self.status = format!(
                    "New {} entry: set tags (optional)",
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
                self.start_body_edit();
            }
        }
        Ok(())
    }

    fn cancel_edit(&mut self) {
        if self.mode.is_create() {
            self.create_draft = None;
            self.mode = Mode::Browse;
            self.status = "New entry cancelled".to_string();
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

    fn handle_key(&mut self, key: KeyEvent) -> Result<bool> {
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
                KeyCode::Char('e') => self.start_body_edit(),
                KeyCode::Char('t') => self.start_title_edit(),
                KeyCode::Char('s') => self.start_section_edit(),
                KeyCode::Char('g') => self.start_tags_edit(),
                KeyCode::Char('n') => self.start_create(),
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
            Mode::EditBody => {
                if key.code == KeyCode::Esc {
                    self.cancel_edit();
                } else {
                    self.body_editor.input(key);
                }
            }
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
        if event::poll(Duration::from_millis(150))? {
            if let Event::Key(key) = event::read()? {
                if app.handle_key(key)? {
                    break;
                }
            }
        }
    }
    Ok(())
}

fn draw(frame: &mut Frame<'_>, app: &mut App) {
    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Length(5),
            Constraint::Min(10),
            Constraint::Length(2),
        ])
        .split(frame.area());

    draw_header(frame, layout[0], app);
    draw_category_cards(frame, layout[1], app);
    draw_body(frame, layout[2], app);
    draw_footer(frame, layout[3], app);
}

fn draw_header(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let header = Paragraph::new(vec![
        Line::from(vec![
            Span::styled(
                format!(" cotext :: {} ", app.project.config.name),
                Style::default()
                    .fg(Color::Rgb(246, 211, 101))
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw("single-page context board"),
        ]),
        Line::from(Span::styled(
            format!(" root: {} ", app.project.root.display()),
            Style::default().fg(Color::Rgb(148, 163, 184)),
        )),
    ])
    .block(
        Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .style(Style::default().bg(Color::Rgb(15, 23, 42))),
    );
    frame.render_widget(header, area);
}

fn draw_category_cards(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let constraints = vec![Constraint::Ratio(1, 5); Category::ALL.len()];
    let cards = Layout::default()
        .direction(Direction::Horizontal)
        .constraints(constraints)
        .split(area);
    for (index, category) in Category::ALL.iter().enumerate() {
        let count = app
            .entries
            .iter()
            .filter(|entry| entry.category() == *category)
            .count();
        let active = index == app.category_index;
        let block = Block::default()
            .title(format!(" {}", category.dir_name()))
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .style(if active {
                Style::default()
                    .fg(Color::Rgb(15, 23, 42))
                    .bg(Color::Rgb(94, 234, 212))
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default()
                    .fg(Color::Rgb(226, 232, 240))
                    .bg(Color::Rgb(30, 41, 59))
            });
        let paragraph = Paragraph::new(vec![
            Line::from(Span::styled(
                category.label(),
                Style::default().add_modifier(Modifier::BOLD),
            )),
            Line::from(format!("{count} entries")),
        ])
        .block(block)
        .wrap(Wrap { trim: true });
        frame.render_widget(paragraph, cards[index]);
    }
}

fn draw_body(frame: &mut Frame<'_>, area: Rect, app: &mut App) {
    let columns = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(35), Constraint::Percentage(65)])
        .split(area);
    draw_entry_list(frame, columns[0], app);
    draw_detail(frame, columns[1], app);
}

fn draw_entry_list(frame: &mut Frame<'_>, area: Rect, app: &mut App) {
    let visible = app.visible_indices();
    let items = visible
        .iter()
        .filter_map(|index| app.entries.get(*index))
        .map(|entry| {
            let mut lines = vec![Line::from(Span::styled(
                entry.title().to_string(),
                Style::default().add_modifier(Modifier::BOLD),
            ))];
            let meta = match entry.section() {
                Some(section) => format!("{}  |  {}", entry.status(), section),
                None => entry.status().to_string(),
            };
            lines.push(Line::from(Span::styled(
                meta,
                Style::default().fg(Color::Rgb(148, 163, 184)),
            )));
            ListItem::new(lines)
        })
        .collect::<Vec<_>>();

    let mut state = ListState::default();
    if !items.is_empty() {
        state.select(Some(app.selected));
    }
    let list = List::new(items)
        .block(
            Block::default()
                .title(format!(" {} list ", app.current_category().dir_name()))
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded),
        )
        .highlight_style(
            Style::default()
                .bg(Color::Rgb(15, 118, 110))
                .fg(Color::Rgb(248, 250, 252))
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol("> ");
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
        Mode::EditBody => frame.render_widget(&app.body_editor, rows[1]),
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
                .block(
                    Block::default()
                        .title(" Preview ")
                        .borders(Borders::ALL)
                        .border_type(BorderType::Rounded),
                )
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
                    .fg(Color::Rgb(248, 250, 252))
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw("  "),
            Span::styled(
                format!("[{status_badge}]"),
                Style::default().fg(Color::Rgb(94, 234, 212)),
            ),
        ]),
        Line::from(format!("id: {id}")),
        Line::from(format!("section: {section}")),
        Line::from(format!("tags: {tags}")),
        Line::from(format!("updated: {updated}")),
        Line::from(format!(
            "preview: {} [{}]",
            app.preview_mode.label(),
            app.preview_audience
        )),
    ])
    .block(
        Block::default()
            .title(" Detail ")
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded),
    )
    .wrap(Wrap { trim: true });
    frame.render_widget(meta, area);
}

fn draw_browse_preview(frame: &mut Frame<'_>, area: Rect, app: &App) {
    if app.preview_mode == PreviewMode::Entry && app.selected_entry().is_none() {
        draw_empty_detail(frame, area);
        return;
    }

    let preview = Paragraph::new(app.browse_preview_contents())
        .block(
            Block::default()
                .title(app.browse_preview_title())
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded),
        )
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
                    "New {} entry",
                    preview_entry.front_matter.category.dir_name()
                ),
                Style::default()
                    .fg(Color::Rgb(248, 250, 252))
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw("  "),
            Span::styled(
                format!("[{}]", preview_entry.status()),
                Style::default().fg(Color::Rgb(94, 234, 212)),
            ),
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
    .block(
        Block::default()
            .title(" New Entry ")
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded),
    )
    .wrap(Wrap { trim: true });
    frame.render_widget(meta, rows[0]);

    let split = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(5), Constraint::Min(3)])
        .split(rows[1]);
    frame.render_widget(&app.field_editor, split[0]);
    let preview = Paragraph::new(render_single_entry(&preview_entry))
        .block(
            Block::default()
                .title(" Preview ")
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded),
        )
        .wrap(Wrap { trim: false });
    frame.render_widget(preview, split[1]);
}

fn draw_empty_detail(frame: &mut Frame<'_>, area: Rect) {
    let empty = Paragraph::new("No entries in this category yet. Press `n` to create one.")
        .block(
            Block::default()
                .title(" Empty ")
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded),
        )
        .wrap(Wrap { trim: true });
    frame.render_widget(empty, area);
}

fn draw_footer(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let footer = Paragraph::new(Line::from(vec![
        Span::styled(
            " status ",
            Style::default()
                .bg(Color::Rgb(234, 179, 8))
                .fg(Color::Rgb(15, 23, 42))
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(format!(" {}", app.status)),
    ]))
    .block(
        Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded),
    );
    frame.render_widget(footer, area);
}

fn default_status_message() -> &'static str {
    "Tab: category  j/k: move  e/t/s/g: edit  n: new  p/a: preview  PgUp/PgDn: scroll  S: status  c/C: copy  q: quit"
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
        .split(|ch| ch == ',' || ch == '\n')
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
        assert_eq!(app.mode, Mode::EditBody);
        assert_eq!(entry.id(), "roadmap-item");
        assert_eq!(entry.title(), "Roadmap item");
        assert_eq!(entry.section(), Some("tui/roadmap"));
        assert_eq!(
            entry.front_matter.tags,
            BTreeSet::from(["tui".to_string(), "ux".to_string()])
        );
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
}
