use std::time::Duration;

use anyhow::Result;
use arboard::Clipboard;
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

use crate::model::{Audience, Category, Entry, EntryFilter, EntryStatus, EntryUpdate, NewEntry};
use crate::render::{render_clipboard_packet, render_single_entry};
use crate::storage::Project;

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
}

struct App {
    project: Project,
    entries: Vec<Entry>,
    category_index: usize,
    selected: usize,
    mode: Mode,
    body_editor: TextArea<'static>,
    title_editor: TextArea<'static>,
    status: String,
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
            title_editor: TextArea::default(),
            status:
                "Tab: category  j/k: move  e: edit  t: title  n: new  S: status  c/C: copy  q: quit"
                    .to_string(),
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

    fn selected_entry(&self) -> Option<&Entry> {
        let visible = self.visible_indices();
        visible
            .get(self.selected)
            .and_then(|entry_index| self.entries.get(*entry_index))
    }

    fn selected_entry_id(&self) -> Option<String> {
        self.selected_entry().map(|entry| entry.id().to_string())
    }

    fn next_category(&mut self) {
        self.category_index = (self.category_index + 1) % Category::ALL.len();
        self.selected = 0;
    }

    fn previous_category(&mut self) {
        if self.category_index == 0 {
            self.category_index = Category::ALL.len() - 1;
        } else {
            self.category_index -= 1;
        }
        self.selected = 0;
    }

    fn move_down(&mut self) {
        let len = self.visible_indices().len();
        if len == 0 {
            self.selected = 0;
        } else {
            self.selected = (self.selected + 1).min(len - 1);
        }
    }

    fn move_up(&mut self) {
        self.selected = self.selected.saturating_sub(1);
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
            self.title_editor = TextArea::from([title.clone()]);
            self.title_editor.set_block(
                Block::default()
                    .title("Title Editor (Ctrl-S to save, Esc to cancel)")
                    .borders(Borders::ALL)
                    .border_type(BorderType::Rounded),
            );
            self.mode = Mode::EditTitle;
            self.status = format!("Editing title for `{title}`");
        }
    }

    fn create_entry(&mut self) -> Result<()> {
        let category = self.current_category();
        let title = format!("Untitled {}", category.dir_name());
        let created = self.project.create_entry(NewEntry {
            category,
            title: title.clone(),
            section: None,
            status: None,
            tags: Default::default(),
            body: Some(category.placeholder_body(&title)),
        })?;
        let preserve_id = Some(created.id().to_string());
        self.refresh(preserve_id)?;
        self.start_body_edit();
        Ok(())
    }

    fn cycle_status(&mut self) -> Result<()> {
        if let Some(id) = self.selected_entry_id() {
            let next_status = self
                .selected_entry()
                .map(|entry| entry.status().next())
                .unwrap_or(EntryStatus::Active);
            self.project.update_entry(
                &id,
                EntryUpdate {
                    status: Some(next_status),
                    ..EntryUpdate::default()
                },
            )?;
            self.refresh(Some(id))?;
            self.status = format!("Status set to `{next_status}`");
        }
        Ok(())
    }

    fn copy_selected(&mut self) {
        if let Some(entry) = self.selected_entry() {
            match copy_to_clipboard(&render_clipboard_packet(
                &self.project,
                &[entry.clone()],
                Audience::Agent,
            )) {
                Ok(()) => {
                    self.status = format!("Copied `{}` packet to clipboard", entry.id());
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
            Audience::Agent,
        )) {
            Ok(()) => {
                self.status = format!(
                    "Copied {} packet for {}",
                    self.current_category().dir_name(),
                    self.project.config.name
                );
            }
            Err(error) => {
                self.status = format!("Clipboard error: {error}");
            }
        }
    }

    fn save_editor(&mut self) -> Result<()> {
        let Some(id) = self.selected_entry_id() else {
            return Ok(());
        };
        match self.mode {
            Mode::Browse => {}
            Mode::EditBody => {
                let body = self.body_editor.lines().join("\n");
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
                let title = self.title_editor.lines().join(" ").trim().to_string();
                if title.is_empty() {
                    self.status = "Title cannot be empty".to_string();
                } else {
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
            }
        }
        Ok(())
    }

    fn cancel_edit(&mut self) {
        self.mode = Mode::Browse;
        self.status = "Edit cancelled".to_string();
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
                KeyCode::Char('n') => self.create_entry()?,
                KeyCode::Char('S') => self.cycle_status()?,
                KeyCode::Char('c') => self.copy_selected(),
                KeyCode::Char('C') => self.copy_category(),
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
            Mode::EditTitle => {
                if key.code == KeyCode::Esc {
                    self.cancel_edit();
                } else {
                    self.title_editor.input(key);
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
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(7), Constraint::Min(8)])
        .split(area);

    let Some(entry) = app.selected_entry().cloned() else {
        let empty = Paragraph::new("No entries in this category yet. Press `n` to create one.")
            .block(
                Block::default()
                    .title(" Empty ")
                    .borders(Borders::ALL)
                    .border_type(BorderType::Rounded),
            )
            .wrap(Wrap { trim: true });
        frame.render_widget(empty, area);
        return;
    };

    let tags = if entry.front_matter.tags.is_empty() {
        "none".to_string()
    } else {
        entry
            .front_matter
            .tags
            .iter()
            .cloned()
            .collect::<Vec<_>>()
            .join(", ")
    };
    let meta = Paragraph::new(vec![
        Line::from(vec![
            Span::styled(
                entry.title().to_string(),
                Style::default()
                    .fg(Color::Rgb(248, 250, 252))
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw("  "),
            Span::styled(
                format!("[{}]", entry.status()),
                Style::default().fg(Color::Rgb(94, 234, 212)),
            ),
        ]),
        Line::from(format!("id: {}", entry.id())),
        Line::from(format!("section: {}", entry.section().unwrap_or("(root)"))),
        Line::from(format!("tags: {tags}")),
        Line::from(format!(
            "updated: {}",
            entry.front_matter.updated_at.format("%Y-%m-%d %H:%M UTC")
        )),
    ])
    .block(
        Block::default()
            .title(" Detail ")
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded),
    )
    .wrap(Wrap { trim: true });
    frame.render_widget(meta, rows[0]);

    match app.mode {
        Mode::Browse => {
            let body = Paragraph::new(render_single_entry(&entry))
                .block(
                    Block::default()
                        .title(" Body ")
                        .borders(Borders::ALL)
                        .border_type(BorderType::Rounded),
                )
                .wrap(Wrap { trim: false });
            frame.render_widget(body, rows[1]);
        }
        Mode::EditBody => {
            frame.render_widget(&app.body_editor, rows[1]);
        }
        Mode::EditTitle => {
            let split = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Length(5), Constraint::Min(3)])
                .split(rows[1]);
            frame.render_widget(&app.title_editor, split[0]);
            let help =
                Paragraph::new("Edit the title above. The body preview stays visible below.")
                    .block(
                        Block::default()
                            .title(" Body Preview ")
                            .borders(Borders::ALL)
                            .border_type(BorderType::Rounded),
                    )
                    .wrap(Wrap { trim: true });
            frame.render_widget(help, split[1]);
        }
    }
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

fn copy_to_clipboard(contents: &str) -> Result<()> {
    let mut clipboard = Clipboard::new()?;
    clipboard.set_text(contents.to_string())?;
    Ok(())
}
