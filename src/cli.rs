use std::fmt::Write;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use clap::{Args, Parser, Subcommand, ValueEnum};

use crate::agents::{install_claude, install_codex};
use crate::model::{
    Audience, Category, Entry, EntryFilter, EntryStatus, EntryUpdate, NewEntry, normalize_section,
};
use crate::render::{render_packet, render_single_entry};
use crate::storage::Project;

#[derive(Debug, Parser)]
#[command(
    name = "cotext",
    version,
    about = "Structured project context management for humans and code agents"
)]
pub struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    Init(InitArgs),
    New(NewArgs),
    Update(UpdateArgs),
    List(ListArgs),
    Show(ShowArgs),
    #[command(alias = "concat")]
    Render(RenderArgs),
    Agent(AgentArgs),
    Tui(TuiArgs),
}

#[derive(Debug, Args)]
struct InitArgs {
    #[arg(default_value = ".")]
    path: PathBuf,
    #[arg(long)]
    name: Option<String>,
    #[arg(long)]
    force: bool,
    #[arg(long)]
    with_agents: bool,
    #[arg(long)]
    codex_skill_dir: Option<PathBuf>,
}

#[derive(Debug, Args)]
struct NewArgs {
    category: Category,
    title: String,
    #[arg(long, default_value = ".")]
    path: PathBuf,
    #[arg(long)]
    section: Option<String>,
    #[arg(long)]
    status: Option<EntryStatus>,
    #[arg(long = "tag")]
    tags: Vec<String>,
    #[arg(long)]
    body: Option<String>,
    #[arg(long)]
    body_file: Option<PathBuf>,
}

#[derive(Debug, Args)]
struct UpdateArgs {
    id: String,
    #[arg(long, default_value = ".")]
    path: PathBuf,
    #[arg(long)]
    title: Option<String>,
    #[arg(long)]
    category: Option<Category>,
    #[arg(long)]
    section: Option<String>,
    #[arg(long)]
    clear_section: bool,
    #[arg(long)]
    status: Option<EntryStatus>,
    #[arg(long)]
    body: Option<String>,
    #[arg(long)]
    body_file: Option<PathBuf>,
    #[arg(long)]
    append: Option<String>,
    #[arg(long = "add-tag")]
    add_tags: Vec<String>,
    #[arg(long = "remove-tag")]
    remove_tags: Vec<String>,
}

#[derive(Debug, Args)]
struct ListArgs {
    #[arg(long, default_value = ".")]
    path: PathBuf,
    #[arg(long = "category")]
    categories: Vec<Category>,
    #[arg(long = "status")]
    statuses: Vec<EntryStatus>,
    #[arg(long)]
    section: Option<String>,
    #[arg(long)]
    include_archived: bool,
    #[arg(long, value_enum, default_value_t = ListFormat::Table)]
    format: ListFormat,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, ValueEnum)]
enum ListFormat {
    #[default]
    Table,
    Json,
}

#[derive(Debug, Args)]
struct ShowArgs {
    id: String,
    #[arg(long, default_value = ".")]
    path: PathBuf,
}

#[derive(Debug, Args)]
struct RenderArgs {
    #[arg(long, default_value = ".")]
    path: PathBuf,
    #[arg(long = "category")]
    categories: Vec<Category>,
    #[arg(long = "status")]
    statuses: Vec<EntryStatus>,
    #[arg(long)]
    section: Option<String>,
    #[arg(long)]
    include_archived: bool,
    #[arg(long, value_enum, default_value_t = Audience::Human)]
    audience: Audience,
    #[arg(long)]
    output: Option<PathBuf>,
}

#[derive(Debug, Args)]
struct AgentArgs {
    #[command(subcommand)]
    command: AgentCommand,
}

#[derive(Debug, Subcommand)]
enum AgentCommand {
    Install(InstallArgs),
}

#[derive(Debug, Args)]
struct InstallArgs {
    target: AgentTarget,
    #[arg(long, default_value = ".")]
    path: PathBuf,
    #[arg(long)]
    codex_skill_dir: Option<PathBuf>,
    #[arg(long)]
    overwrite: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
enum AgentTarget {
    Codex,
    Claude,
    All,
}

#[derive(Debug, Args)]
struct TuiArgs {
    #[arg(default_value = ".")]
    path: PathBuf,
}

pub fn run() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Command::Init(args) => cmd_init(args),
        Command::New(args) => cmd_new(args),
        Command::Update(args) => cmd_update(args),
        Command::List(args) => cmd_list(args),
        Command::Show(args) => cmd_show(args),
        Command::Render(args) => cmd_render(args),
        Command::Agent(args) => cmd_agent(args),
        Command::Tui(args) => cmd_tui(args),
    }
}

fn cmd_init(args: InitArgs) -> Result<()> {
    let project = Project::init(&args.path, args.name, args.force)?;
    println!(
        "Initialized cotext project `{}` at {}",
        project.config.name,
        project.root.display()
    );
    if args.with_agents {
        let codex = install_codex(&project, args.codex_skill_dir.as_deref(), true)?;
        let claude = install_claude(&project, true)?;
        print_install_report("codex", &codex.changed);
        print_install_report("claude", &claude.changed);
    }
    Ok(())
}

fn cmd_new(args: NewArgs) -> Result<()> {
    let project = discover(&args.path)?;
    let body = read_body(args.body, args.body_file.as_deref())?;
    let entry = project.create_entry(NewEntry {
        category: args.category,
        title: args.title,
        section: args.section.and_then(|section| normalize_section(&section)),
        status: args.status,
        tags: args.tags.into_iter().collect(),
        body,
    })?;
    println!(
        "Created `{}` at {}",
        entry.id(),
        entry
            .path
            .strip_prefix(&project.root)
            .unwrap_or(&entry.path)
            .display()
    );
    Ok(())
}

fn cmd_update(args: UpdateArgs) -> Result<()> {
    let project = discover(&args.path)?;
    let body = read_body(args.body, args.body_file.as_deref())?;
    let entry = project.update_entry(
        &args.id,
        EntryUpdate {
            title: args.title,
            category: args.category,
            section: args.section.and_then(|section| normalize_section(&section)),
            clear_section: args.clear_section,
            status: args.status,
            body,
            append: args.append,
            add_tags: args.add_tags.into_iter().collect(),
            remove_tags: args.remove_tags.into_iter().collect(),
        },
    )?;
    println!("Updated `{}` ({})", entry.id(), entry.status());
    Ok(())
}

fn cmd_list(args: ListArgs) -> Result<()> {
    let project = discover(&args.path)?;
    let entries = project.list_entries(&build_filter(
        args.categories,
        args.statuses,
        args.section,
        args.include_archived,
    ))?;
    match args.format {
        ListFormat::Table => print!("{}", render_table(&entries)),
        ListFormat::Json => {
            let json = serde_json::to_string_pretty(
                &entries
                    .iter()
                    .map(|entry| &entry.front_matter)
                    .collect::<Vec<_>>(),
            )?;
            println!("{json}");
        }
    }
    Ok(())
}

fn cmd_show(args: ShowArgs) -> Result<()> {
    let project = discover(&args.path)?;
    let entry = project.load_entry(&args.id)?;
    print!("{}", render_single_entry(&entry));
    Ok(())
}

fn cmd_render(args: RenderArgs) -> Result<()> {
    let project = discover(&args.path)?;
    let entries = project.list_entries(&build_filter(
        args.categories,
        args.statuses,
        args.section,
        args.include_archived,
    ))?;
    let rendered = render_packet(&project, &entries, args.audience);
    if let Some(output) = args.output {
        fs::write(&output, rendered)
            .with_context(|| format!("failed to write {}", output.display()))?;
    } else {
        print!("{rendered}");
    }
    Ok(())
}

fn cmd_agent(args: AgentArgs) -> Result<()> {
    match args.command {
        AgentCommand::Install(args) => {
            let project = discover(&args.path)?;
            match args.target {
                AgentTarget::Codex => {
                    let report =
                        install_codex(&project, args.codex_skill_dir.as_deref(), args.overwrite)?;
                    print_install_report("codex", &report.changed);
                    print_skip_report(&report.skipped);
                }
                AgentTarget::Claude => {
                    let report = install_claude(&project, args.overwrite)?;
                    print_install_report("claude", &report.changed);
                    print_skip_report(&report.skipped);
                }
                AgentTarget::All => {
                    let codex =
                        install_codex(&project, args.codex_skill_dir.as_deref(), args.overwrite)?;
                    let claude = install_claude(&project, args.overwrite)?;
                    print_install_report("codex", &codex.changed);
                    print_install_report("claude", &claude.changed);
                    print_skip_report(&codex.skipped);
                    print_skip_report(&claude.skipped);
                }
            }
        }
    }
    Ok(())
}

fn cmd_tui(args: TuiArgs) -> Result<()> {
    let project = discover(&args.path)?;
    crate::tui::run(project)
}

fn discover(path: &Path) -> Result<Project> {
    Project::discover(path)
}

fn build_filter(
    categories: Vec<Category>,
    statuses: Vec<EntryStatus>,
    section: Option<String>,
    include_archived: bool,
) -> EntryFilter {
    EntryFilter {
        categories: (!categories.is_empty()).then_some(categories),
        statuses: (!statuses.is_empty()).then_some(statuses),
        section_prefix: section.and_then(|value| normalize_section(&value)),
        include_archived,
        ..EntryFilter::default()
    }
}

fn read_body(inline: Option<String>, file: Option<&Path>) -> Result<Option<String>> {
    match (inline, file) {
        (Some(inline), None) => Ok(Some(inline)),
        (None, Some(file)) => {
            Ok(Some(fs::read_to_string(file).with_context(|| {
                format!("failed to read body from {}", file.display())
            })?))
        }
        (Some(_), Some(_)) => anyhow::bail!("use either `--body` or `--body-file`, not both"),
        (None, None) => Ok(None),
    }
}

fn render_table(entries: &[Entry]) -> String {
    if entries.is_empty() {
        return "No entries matched.\n".to_string();
    }

    let id_width = entries
        .iter()
        .map(|entry| entry.id().len())
        .max()
        .unwrap_or(2);
    let section_width = entries
        .iter()
        .map(|entry| entry.section().unwrap_or("-").len())
        .max()
        .unwrap_or(7);
    let mut output = String::new();
    let _ = writeln!(
        &mut output,
        "{:<8} {:<10} {:<id_width$} {:<section_width$} Title",
        "Category",
        "Status",
        "ID",
        "Section",
        id_width = id_width,
        section_width = section_width,
    );
    let _ = writeln!(
        &mut output,
        "{}",
        "-".repeat(8 + 1 + 10 + 1 + id_width + 1 + section_width + 1 + 20)
    );
    for entry in entries {
        let _ = writeln!(
            &mut output,
            "{:<8} {:<10} {:<id_width$} {:<section_width$} {}",
            entry.category(),
            entry.status(),
            entry.id(),
            entry.section().unwrap_or("-"),
            entry.title(),
            id_width = id_width,
            section_width = section_width,
        );
    }
    output
}

fn print_install_report(label: &str, paths: &[PathBuf]) {
    if paths.is_empty() {
        return;
    }
    println!("Installed {label} assets:");
    for path in paths {
        println!("  - {}", path.display());
    }
}

fn print_skip_report(paths: &[PathBuf]) {
    if paths.is_empty() {
        return;
    }
    println!("Skipped existing assets:");
    for path in paths {
        println!("  - {}", path.display());
    }
}
