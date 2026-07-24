use crate::api::{CurseForgeClient, ModrinthClient, Platform, SearchFilters, SearchResult};
use crate::cli::Cli;
use crate::config::Config;
use color_eyre::eyre::Result;
use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use crossterm::terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen};
use crossterm::ExecutableCommand;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, Paragraph, Wrap};
use ratatui::Terminal;
use ratatui_image::picker::Picker;
use ratatui_image::protocol::Protocol;
use ratatui_image::{Image, Resize};
use std::cell::Cell;
use std::collections::HashSet;
use std::io::stdout;
use std::time::Duration;

const TICK_RATE: Duration = Duration::from_millis(100);

#[derive(PartialEq)]
enum Focus {
    Neutral,
    Query,
    Filters,
    Results,
}

struct FiltersState {
    version: String,
    loader: String,
    project_type: String,
    platform: String,
}

impl Default for FiltersState {
    fn default() -> Self {
        Self {
            version: String::new(),
            loader: String::new(),
            project_type: "mod".into(),
            platform: "modrinth, curseforge".into(),
        }
    }
}

#[derive(Clone, PartialEq)]
enum FilterKind {
    Version,
    Loader,
    Type,
    Platform,
}

impl FilterKind {
    fn label(&self) -> &str {
        match self {
            FilterKind::Version => "Version",
            FilterKind::Loader => "Loader",
            FilterKind::Type => "Type",
            FilterKind::Platform => "Platform",
        }
    }
}

struct BrowseState {
    kind: FilterKind,
    options: Vec<String>,
    filtered: Vec<usize>,
    filter_text: String,
    selected: usize,
    toggled: HashSet<usize>,
    scroll: usize,
}

#[derive(PartialEq)]
enum Status {
    Idle,
    Searching,
    Error(String),
    ApiKeyPrompt,
    Done,
}

enum AppEvent {
    Results(Vec<SearchResult>),
    Error(String),
    BrowseOptions { kind: FilterKind, options: Vec<String>, saved: HashSet<String> },
    IconLoaded(usize, Vec<u8>),
}

pub async fn run_tui(_args: Cli, mut cfg: Config) -> Result<()> {
    enable_raw_mode()?;
    let mut stdout = stdout();
    stdout.execute(EnterAlternateScreen)?;
    let mut terminal = Terminal::new(ratatui::backend::CrosstermBackend::new(stdout))?;

    let (event_tx, mut event_rx) = tokio::sync::mpsc::channel(32);

    let mut app = App {
        query: String::new(),
        cursor_pos: 0,
        filters: FiltersState::default(),
        results: Vec::new(),
        focus: Focus::Neutral,
        status: Status::Idle,
        scroll: 0,
        selected: 0,
        browse_mode: None,
        visible_count: Cell::new(0),
        filter_selected: 0,
        api_key_input: String::new(),
        picker: Picker::from_query_stdio().unwrap_or_else(|_| Picker::halfblocks()),
        proto_cache: Vec::new(),
    };

    let res = run(&mut terminal, &mut app, &mut cfg, &mut event_rx, &event_tx).await;

    disable_raw_mode()?;
    terminal.backend_mut().execute(LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    if let Err(e) = res {
        eprintln!("{e:?}");
    }
    Ok(())
}

struct App {
    query: String,
    cursor_pos: usize,
    filters: FiltersState,
    results: Vec<SearchResult>,
    focus: Focus,
    status: Status,
    scroll: usize,
    selected: usize,
    browse_mode: Option<BrowseState>,
    filter_selected: usize,
    api_key_input: String,
    picker: Picker,
    proto_cache: Vec<Option<Protocol>>,
    visible_count: Cell<usize>,
}

async fn run(
    terminal: &mut Terminal<ratatui::backend::CrosstermBackend<std::io::Stdout>>,
    app: &mut App,
    cfg: &mut Config,
    rx: &mut tokio::sync::mpsc::Receiver<AppEvent>,
    tx: &tokio::sync::mpsc::Sender<AppEvent>,
) -> Result<()> {
    loop {
        terminal.draw(|f| render(f, app))?;

        if event::poll(TICK_RATE)? {

            if let Event::Key(key) = event::read()? {
                if key.kind != KeyEventKind::Press {
                    continue;
                }

                if app.browse_mode.is_some() {
                    let action = {
                        let b = app.browse_mode.as_mut().unwrap();
                        handle_browse_key_inner(b, key.code)
                    };
                    match action {
                        BrowseAction::Toggle(kind) => {
                            if let Some(ref browse) = app.browse_mode {
                                let val = {
                                    let mut v: Vec<String> = browse
                                        .toggled
                                        .iter()
                                        .map(|&i| browse.options[i].clone())
                                        .collect();
                                    v.sort();
                                    v.join(", ")
                                };
                                match kind {
                                    FilterKind::Version => app.filters.version = val,
                                    FilterKind::Loader => app.filters.loader = val,
                                    FilterKind::Type => app.filters.project_type = val,
                                    FilterKind::Platform => app.filters.platform = val,
                                }
                            }
                        }
                        BrowseAction::Close => app.browse_mode = None,
                        BrowseAction::None => {}
                    }
                } else if app.status == Status::ApiKeyPrompt {
                    handle_apikey_key(app, key.code, cfg, tx).await;
                } else {
                    match key.code {
                        KeyCode::Char('q') if app.focus == Focus::Neutral => break Ok(()),
                        KeyCode::Char('s') if app.focus == Focus::Neutral => app.focus = Focus::Query,
                        KeyCode::Char('f') if app.focus == Focus::Neutral => app.focus = Focus::Filters,
                        KeyCode::Char('r') if app.focus == Focus::Neutral => app.focus = Focus::Results,
                        KeyCode::Esc => app.focus = Focus::Neutral,
                        KeyCode::Tab => {
                            app.focus = match app.focus {
                                Focus::Neutral | Focus::Query => Focus::Filters,
                                Focus::Filters => Focus::Results,
                                Focus::Results => Focus::Query,
                            };
                        }
                        KeyCode::BackTab => {
                            app.focus = match app.focus {
                                Focus::Neutral | Focus::Query => Focus::Results,
                                Focus::Filters => Focus::Query,
                                Focus::Results => Focus::Filters,
                            };
                        }
                        _ => handle_key(app, key.code, cfg, tx).await,
                    }
                }
            }
        }

        if let Ok(event) = rx.try_recv() {
            match event {
                AppEvent::Results(results) => {
                    app.results = results;
                    app.status = Status::Done;
                    app.scroll = 0;
                    app.selected = 0;
                    app.focus = Focus::Results;
                    app.proto_cache = vec![None; app.results.len()];
                    // Trigger icon downloads
                    let tx = tx.clone();
                    let urls: Vec<Option<String>> = app.results.iter().map(|r| r.icon_url.clone()).collect();
                    tokio::spawn(async move {
                        for (i, url) in urls.iter().enumerate() {
                            if let Some(url) = url {
                                if let Ok(resp) = reqwest::get(url).await {
                                    if let Ok(bytes) = resp.bytes().await {
                                        let _ = tx.send(AppEvent::IconLoaded(i, bytes.to_vec())).await;
                                    }
                                }
                            }
                        }
                    });
                }
                AppEvent::Error(msg) => {
                    app.status = Status::Error(msg);
                }
                AppEvent::BrowseOptions { kind, options, saved } => {
                    if let Some(ref mut browse) = app.browse_mode {
                        if browse.kind == kind {
                            browse.options = options;
                            browse.filtered = (0..browse.options.len()).collect();
                            browse.selected = 0;
                            browse.scroll = 0;
                            browse.toggled = saved.iter().filter_map(|s| browse.options.iter().position(|o| o == s)).collect();
                        }
                    }
                }

                AppEvent::IconLoaded(i, bytes) => {
                    if i < app.proto_cache.len() {
                        match image::load_from_memory(&bytes) {
                            Ok(dyn_img) => {
                                let size = ratatui::prelude::Size::new(6, 2);
                                match app.picker.new_protocol(dyn_img, size, Resize::Fit(None)) {
                                    Ok(proto) => app.proto_cache[i] = Some(proto),
                                    Err(e) => eprintln!("protocol error for {i}: {e}"),
                                }
                            }
                            Err(e) => eprintln!("image decode error for {i}: {e}"),
                        }
                    }
                }
            }
        }
    }
}

enum BrowseAction {
    Toggle(FilterKind),
    Close,
    None,
}

fn handle_browse_key_inner(browse: &mut BrowseState, key: KeyCode) -> BrowseAction {
    match key {
        KeyCode::Esc => BrowseAction::Close,
        KeyCode::Char('\n') | KeyCode::Enter => {
            if browse.filtered.is_empty() {
                return BrowseAction::None;
            }
            let idx = browse.filtered[browse.selected];
            if browse.kind == FilterKind::Version || browse.kind == FilterKind::Type {
                // Single-select: press again to reset to "any"
                if browse.toggled.contains(&idx) {
                    browse.toggled.clear();
                } else {
                    browse.toggled.clear();
                    browse.toggled.insert(idx);
                }
            } else if browse.toggled.contains(&idx) {
                browse.toggled.remove(&idx);
            } else {
                browse.toggled.insert(idx);
            }
            // Build the filter value from toggled items
            let values: Vec<String> = {
                let mut v: Vec<String> = browse
                    .toggled
                    .iter()
                    .map(|&i| browse.options[i].clone())
                    .collect();
                v.sort();
                v
            };
            let _val = values.join(", ");
            let kind = browse.kind.clone();
            // Always send Toggle so the filter value gets updated (even when resetting to empty)
            BrowseAction::Toggle(kind)
        }
        KeyCode::Tab => BrowseAction::Close,
        KeyCode::Up => {
            if browse.selected > 0 {
                browse.selected -= 1;
            }
            if browse.selected < browse.scroll {
                browse.scroll = browse.selected;
            }
            BrowseAction::None
        }
        KeyCode::Down => {
            let max = browse.filtered.len().saturating_sub(1);
            if browse.selected < max {
                browse.selected += 1;
            }
            let visible = browse.filtered.len().min(15).max(5);
            if browse.selected >= browse.scroll + visible - 1 {
                browse.scroll += 1;
            }
            BrowseAction::None
        }
        KeyCode::Char(c) => {
            browse.filter_text.push(c);
            update_filtered(browse);
            BrowseAction::None
        }
        KeyCode::Backspace => {
            browse.filter_text.pop();
            update_filtered(browse);
            BrowseAction::None
        }
        KeyCode::Delete => {
            browse.filter_text.pop();
            update_filtered(browse);
            BrowseAction::None
        }
        _ => BrowseAction::None,
    }
}

fn update_filtered(browse: &mut BrowseState) {
    if browse.filter_text.is_empty() {
        browse.filtered = (0..browse.options.len()).collect();
    } else {
        browse.filtered = browse
            .options
            .iter()
            .enumerate()
            .filter(|(_, opt)| opt.to_lowercase().contains(&browse.filter_text.to_lowercase()))
            .map(|(i, _)| i)
            .collect();
    }
    if browse.selected >= browse.filtered.len() {
        browse.selected = browse.filtered.len().saturating_sub(1);
    }
}

async fn handle_key(app: &mut App, key: KeyCode, cfg: &Config, tx: &tokio::sync::mpsc::Sender<AppEvent>) {
    match app.focus {
        Focus::Neutral => match key {
            KeyCode::Char('q') => {} // handled above, should never reach here
            _ => app.focus = Focus::Query,
        },
        Focus::Query => match key {
            KeyCode::Char(c) => {
                app.query.insert(app.cursor_pos, c);
                app.cursor_pos += 1;
            }
            KeyCode::Backspace => {
                if app.cursor_pos > 0 {
                    app.cursor_pos -= 1;
                    app.query.remove(app.cursor_pos);
                }
            }
            KeyCode::Delete => {
                if app.cursor_pos < app.query.len() {
                    app.query.remove(app.cursor_pos);
                }
            }
            KeyCode::Left if app.cursor_pos > 0 => app.cursor_pos -= 1,
            KeyCode::Right if app.cursor_pos < app.query.len() => app.cursor_pos += 1,
            KeyCode::Home => app.cursor_pos = 0,
            KeyCode::End => app.cursor_pos = app.query.len(),
            KeyCode::Enter => start_search(app, cfg, tx).await,
            _ => {}
        },
        Focus::Filters => match key {
            KeyCode::Down => app.filter_selected = app.filter_selected.saturating_add(1).min(3),
            KeyCode::Up => app.filter_selected = app.filter_selected.saturating_sub(1),
            KeyCode::Enter => {
                let kind = match app.filter_selected {
                    0 => FilterKind::Version,
                    1 => FilterKind::Loader,
                    2 => FilterKind::Type,
                    3 => FilterKind::Platform,
                    _ => FilterKind::Platform,
                };
                // Pre-populate toggled from current filter value
                let current_val = match &kind {
                    FilterKind::Version => &app.filters.version,
                    FilterKind::Loader => &app.filters.loader,
                    FilterKind::Type => &app.filters.project_type,
                    FilterKind::Platform => &app.filters.platform,
                };
                let saved: std::collections::HashSet<String> = current_val.split(", ").map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect();
                app.browse_mode = Some(BrowseState {
                    kind: kind.clone(),
                    options: Vec::new(),
                    filtered: Vec::new(),
                    filter_text: String::new(),
                    selected: 0,
                    toggled: HashSet::new(),
                    scroll: 0,
                });
                let options: Vec<String> = match kind {
                    FilterKind::Version => crate::api::filters::VERSIONS.iter().map(|s| s.to_string()).collect(),
                    FilterKind::Loader => crate::api::filters::LOADERS.iter().map(|s| s.to_string()).collect(),
                    FilterKind::Type => crate::api::filters::TYPES.iter().map(|s| s.to_string()).collect(),
                    FilterKind::Platform => vec!["modrinth".into(), "curseforge".into()],
                };
                let _ = tx.try_send(AppEvent::BrowseOptions { kind, options, saved });
            }
            KeyCode::Char(c) => match app.filter_selected {
                0 => app.filters.version.push(c),
                1 => app.filters.loader.push(c),
                2 => app.filters.project_type.push(c),
                3 => app.filters.platform.push(c),
                _ => {}
            },
            KeyCode::Backspace => match app.filter_selected {
                0 => { let _ = app.filters.version.pop(); }
                1 => { let _ = app.filters.loader.pop(); }
                2 => { let _ = app.filters.project_type.pop(); }
                3 => { let _ = app.filters.platform.pop(); }
                _ => {}
            },
            KeyCode::Delete => match app.filter_selected {
                0 => { let _ = app.filters.version.pop(); }
                1 => { let _ = app.filters.loader.pop(); }
                2 => { let _ = app.filters.project_type.pop(); }
                3 => { let _ = app.filters.platform.pop(); }
                _ => {}
            },
            _ => {}
        },
        Focus::Results => match key {
            KeyCode::Up => {
                app.selected = app.selected.saturating_sub(1);
                if app.selected < app.scroll {
                    app.scroll = app.selected;
                }
            }
            KeyCode::Down => {
                let max = app.results.len().saturating_sub(1);
                app.selected = app.selected.saturating_add(1).min(max);
                let vis = app.visible_count.get().max(3);
                if app.selected >= app.scroll + vis {
                    app.scroll += 1;
                }
            }
            KeyCode::PageUp => {
                app.selected = app.selected.saturating_sub(10);
                app.scroll = app.selected;
            }
            KeyCode::PageDown => {
                let max = app.results.len().saturating_sub(1);
                app.selected = app.selected.saturating_add(10).min(max);
                let vis = app.visible_count.get().max(3);
                if app.selected >= app.scroll + vis {
                    app.scroll = app.selected.saturating_sub(vis.saturating_sub(1));
                }
            }
            KeyCode::Home => {
                app.selected = 0;
                app.scroll = 0;
            }
            KeyCode::End => {
                app.selected = app.results.len().saturating_sub(1);
                app.scroll = app.selected.saturating_sub(9);
            }
            _ => {}
        },
    }
}

async fn handle_apikey_key(app: &mut App, key: KeyCode, cfg: &mut Config, _tx: &tokio::sync::mpsc::Sender<AppEvent>) {
    match key {
        KeyCode::Esc => {
            app.status = Status::Idle;
            app.api_key_input.clear();
        }
        KeyCode::Char(c) => {
            app.api_key_input.push(c);
        }
        KeyCode::Backspace => {
            app.api_key_input.pop();
        }
        KeyCode::Delete => {
            app.api_key_input.pop();
        }
        KeyCode::Enter => {
            let key = app.api_key_input.trim().to_owned();
            if !key.is_empty() {
                // Build new config with the key
                let new_cfg = Config {
                    curseforge_api_key: Some(key),
                };
                if new_cfg.save().is_ok() {
                    // Reload config so subsequent CurseForge calls work
                    if let Ok(loaded) = Config::load() {
                        *cfg = loaded;
                    }
                    app.status = Status::Idle;
                    app.api_key_input.clear();
                } else {
                    app.status = Status::Error("Failed to save API key to ~/.easypacker.json".into());
                }
            }
        }
        _ => {}
    }
}

async fn start_search(app: &mut App, cfg: &Config, tx: &tokio::sync::mpsc::Sender<AppEvent>) {
    if app.query.is_empty() {
        return;
    }


    app.status = Status::Searching;

    let filters = SearchFilters {
        query: app.query.clone(),
        version: if app.filters.version.is_empty() { None } else { Some(app.filters.version.clone()) },
        loader: if app.filters.loader.is_empty() { None } else { Some(app.filters.loader.clone()) },
        project_type: if app.filters.project_type.is_empty() { Some("mod".into()) } else { Some(app.filters.project_type.clone()) },
        sort: "relevance".into(),
        limit: 25,
        offset: 0,
    };

    let tx = tx.clone();
    let platforms: Vec<String> = app.filters.platform.split(", ").map(|s| s.to_string()).collect();

    // Prompt for API key if CurseForge selected but no key configured
    if cfg.get_api_key(None).is_err() && platforms.iter().any(|p| p == "curseforge") {
        app.status = Status::ApiKeyPrompt;
        return;
    }
    let api_key = cfg.get_api_key(None).ok();
    tokio::spawn(async move {
        let mut all: Vec<SearchResult> = Vec::new();
        for p in &platforms {
            let event = match p.as_str() {
                "modrinth" => match ModrinthClient::search(&filters).await {
                    Ok(results) => Some(results),
                    Err(e) => {
                        let _ = tx.send(AppEvent::Error(format!("Modrinth error: {e}"))).await;
                        continue;
                    }
                },
                "curseforge" => {
                    let key = match &api_key {
                        Some(k) => k,
                        None => {
                            let _ = tx.send(AppEvent::Error("CurseForge API key not configured.".into())).await;
                            continue;
                        }
                    };
                    match CurseForgeClient::new(key).search(&filters).await {
                        Ok(results) => Some(results),
                        Err(e) => {
                            let _ = tx.send(AppEvent::Error(format!("CurseForge error: {e}"))).await;
                            continue;
                        }
                    }
                }
                _ => None,
            };
            if let Some(mut results) = event {
                // Keep only results matching the user's chosen type (mandatory single-select)
                if let Some(ref pt) = filters.project_type {
                    let allowed: Vec<&str> = pt.split(", ").map(|s| s.trim()).collect();
                    results.retain(|r| allowed.contains(&r.project_type.to_lowercase().as_str()));
                }
                // Merge: if same title exists, fold in platform-specific data
                for r in results {
                    let lower = r.title.to_lowercase();
                    if let Some(existing) = all.iter_mut().find(|e| e.title.to_lowercase() == lower) {
                        match r.platform {
                            Platform::Modrinth => {
                                existing.cross.modrinth_downloads = r.downloads;
                                existing.cross.modrinth_url = r.url;
                            }
                            Platform::CurseForge => {
                                existing.cross.curseforge_downloads = r.downloads;
                                existing.cross.curseforge_url = r.url;
                            }
                        }
                    } else {
                        let mut entry = r;
                        match entry.platform {
                            Platform::Modrinth => {
                                entry.cross.modrinth_downloads = entry.downloads;
                                entry.cross.modrinth_url = entry.url.clone();
                            }
                            Platform::CurseForge => {
                                entry.cross.curseforge_downloads = entry.downloads;
                                entry.cross.curseforge_url = entry.url.clone();
                            }
                        }
                        all.push(entry);
                    }
                }
            }
        }
        if all.is_empty() && !platforms.is_empty() {
            // errors already sent via tx
            return;
        }
        let _ = tx.send(AppEvent::Results(all)).await;
    });
}

fn render(frame: &mut ratatui::Frame, app: &App) {

    let area = frame.area();

    let constraints = if app.status == Status::ApiKeyPrompt {
        vec![
            Constraint::Length(3),
            Constraint::Length(3),
            Constraint::Length(7),
            Constraint::Min(1),
            Constraint::Length(1),
        ]
    } else {
        vec![
            Constraint::Length(3),
            Constraint::Length(3),
            Constraint::Length(7),
            Constraint::Min(1),
            Constraint::Length(1),
        ]
    };

    let main_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints(constraints)
        .split(area);

    render_header(frame, main_chunks[0], app);
    render_search(frame, main_chunks[1], app);
    render_filters(frame, main_chunks[2], app);

    if app.status == Status::ApiKeyPrompt {
        render_apikey_prompt(frame, main_chunks[3], app);
    } else if let Some(ref browse) = app.browse_mode {
        render_browse(frame, main_chunks[3], browse);
    } else if let Status::Error(msg) = &app.status {
        render_error(frame, main_chunks[3], msg.as_str());
    } else {
        render_results(frame, main_chunks[3], app);
    }
    render_status(frame, main_chunks[4], app);
}

fn render_apikey_prompt(frame: &mut ratatui::Frame, area: Rect, app: &App) {
    let block = Block::default()
        .borders(Borders::ALL)
        .title(" CurseForge API Key ")
        .border_style(Style::default().fg(Color::Yellow));

    let text = vec![
        Line::from(Span::styled(
            "Get a free API key at: https://console.curseforge.com/",
            Style::default().fg(Color::Cyan),
        )),
        Line::from(""),
        Line::from(vec![
            Span::styled("Key: ", Style::default().fg(Color::Gray)),
            Span::styled(
                "*".repeat(app.api_key_input.len()),
                Style::default().fg(Color::White),
            ),
            Span::styled("█", Style::default().fg(Color::Yellow)),
        ]),
        Line::from(""),
        Line::from(Span::styled(
            "Enter: save & continue   Esc: cancel",
            Style::default().fg(Color::DarkGray),
        )),
    ];

    frame.render_widget(Paragraph::new(text).block(block).wrap(Wrap { trim: false }), area);
}

fn render_browse(frame: &mut ratatui::Frame, area: Rect, browse: &BrowseState) {
    let title = format!(
        " {} — type:filter, Enter:toggle, Tab:apply, Esc:cancel ",
        browse.kind.label()
    );

    if browse.options.is_empty() {
        let p = Paragraph::new(" Loading options… ").block(
            Block::default().borders(Borders::ALL).title(title.as_str()),
        );
        frame.render_widget(p, area);
        return;
    }

    let _filter_display = if browse.filter_text.is_empty() {
        " type to filter…"
    } else {
        Box::leak(Box::new(format!(" filter: {} ", browse.filter_text)))
    };

    let list_h = area.height.saturating_sub(2) as usize;
    let slice_start = browse.scroll.min(browse.filtered.len().saturating_sub(1));
    let visible: &[usize] = &browse.filtered[slice_start..];
    let vis_count = visible.len().min(list_h);

    let items: Vec<ListItem> = visible[..vis_count]
        .iter()
        .enumerate()
        .map(|(i, opt_idx)| {
            let opt = &browse.options[*opt_idx];
            let checked = if browse.toggled.contains(opt_idx) { "[x]" } else { "[ ]" };
            let selected = slice_start + i == browse.selected;
            let prefix = if selected { "▸ " } else { "  " };
            let style = if selected {
                Style::default().bg(Color::Blue).fg(Color::White)
            } else {
                Style::default()
            };
            ListItem::new(Line::from(Span::styled(
                format!("{prefix}{checked} {opt}"),
                style,
            )))
        })
        .collect();

    let list_area = Rect::new(area.x, area.y + 1, area.width, area.height.saturating_sub(1));
    let list = List::new(items).block(Block::default().borders(Borders::ALL).title(title.as_str()));
    frame.render_widget(list, list_area);
}

fn render_error(frame: &mut ratatui::Frame, area: Rect, msg: &str) {
    let paragraph = Paragraph::new(msg)
        .style(Style::default().fg(Color::Red))
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(" Error ")
                .border_style(Style::default().fg(Color::Red)),
        )
        .wrap(Wrap { trim: false });
    frame.render_widget(paragraph, area);
}

fn render_header(frame: &mut ratatui::Frame, area: Rect, app: &App) {
    let neutral = app.focus == Focus::Neutral;
    let neutral_indicator = if neutral { " ◇".to_owned() } else { String::new() };
    let mut header_parts = vec![
        Span::styled(
            format!(" easypacker{} ", neutral_indicator),
            Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
        ),
        Span::styled(" │ ", Style::default().fg(Color::DarkGray)),
    ];
    if neutral {
        header_parts.push(Span::styled("s:search f:filters r:results ", Style::default().fg(Color::DarkGray)));
        header_parts.push(Span::styled("q:quit", Style::default().fg(Color::Red).add_modifier(Modifier::BOLD)));
    } else {
        header_parts.push(Span::styled("Esc:neutral", Style::default().fg(Color::DarkGray)));
    }
    let header = Line::from(header_parts);
    frame.render_widget(
        Paragraph::new(header).block(Block::default().borders(Borders::BOTTOM)),
        area,
    );
}

fn render_search(frame: &mut ratatui::Frame, area: Rect, app: &App) {
    let focused = app.focus == Focus::Query;
    let border_style = if focused {
        Style::default().fg(Color::Yellow)
    } else {
        Style::default()
    };

    let input = Paragraph::new(app.query.as_str())
        .style(if focused {
            Style::default().fg(Color::White)
        } else {
            Style::default().fg(Color::Gray)
        })
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(" Search ")
                .border_style(border_style),
        );

    frame.render_widget(input, area);

    if focused {
        let x = area.x + 1 + app.cursor_pos as u16;
        let y = area.y + 1;
        frame.set_cursor_position((x, y));
    }
}

fn render_filters(frame: &mut ratatui::Frame, area: Rect, app: &App) {
    let focused = app.focus == Focus::Filters;

    let border_style = if focused {
        Style::default().fg(Color::Yellow)
    } else {
        Style::default().fg(Color::DarkGray)
    };

    let hint = if focused { " Enter:browse" } else { "" };

    let lines = vec![
        filter_line("Version", &app.filters.version, focused, app.filter_selected == 0),
        filter_line("Loader", &app.filters.loader, focused, app.filter_selected == 1),
        filter_line("Type", &app.filters.project_type, focused, app.filter_selected == 2),
        filter_line("Platform", &app.filters.platform, focused, app.filter_selected == 3),
    ];

    frame.render_widget(
        Paragraph::new(lines)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(format!(" Filters{hint} "))
                    .border_style(border_style),
            ),
        area,
    );
}

fn filter_line(label: &str, value: &str, focused: bool, selected: bool) -> Line<'static> {
    let display: String = if value.is_empty() {
        "any".into()
    } else {
        value.to_owned()
    };
    if focused && selected {
        Line::from(vec![
            Span::styled("▸ ", Style::default().fg(Color::Yellow)),
            Span::styled(
                format!("{label}: "),
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(display, Style::default().fg(Color::White)),
        ])
    } else {
        Line::from(vec![
            Span::raw("  "),
            Span::styled(format!("{label}: "), Style::default().fg(Color::Gray)),
            Span::styled(
                display,
                if value.is_empty() {
                    Style::default().fg(Color::DarkGray)
                } else {
                    Style::default().fg(Color::White)
                },
            ),
        ])
    }
}


fn render_results(frame: &mut ratatui::Frame, area: Rect, app: &App) {
    let focused = app.focus == Focus::Results;
    let border_style = if focused {
        Style::default().fg(Color::Yellow)
    } else {
        Style::default().fg(Color::DarkGray)
    };

    let title = match app.status {
        Status::Idle => " Results ".into(),
        Status::Searching => " Searching… ".into(),
        Status::Done => format!(" Results ({}) ", app.results.len()),
        _ => unreachable!(),
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .title(title)
        .border_style(border_style);
    let inner = block.inner(area);
    frame.render_widget(&block, area);

    let max_items = (inner.height as usize).saturating_div(3).min(app.results.len().saturating_sub(app.scroll));
    app.visible_count.set(max_items);
    let mut current_y = inner.y;
    for vis in 0..max_items {
        let idx = app.scroll + vis;
        if idx >= app.results.len() { break; }
        let r = &app.results[idx];
        let selected = idx == app.selected && focused;

        // Dynamic height: 3 normally, 4 when selected with URLs
        let has_url = r.cross.modrinth_url.is_some() || r.cross.curseforge_url.is_some() || r.url.is_some();
        let item_lines: usize = if selected && has_url { 4 } else { 3 };
        let y = current_y;
        current_y += item_lines as u16;

        // Icon
        let icon_area = Rect::new(inner.x + 1, y, 6, 2);
        if let Some(ref proto) = app.proto_cache.get(idx).and_then(|p| p.as_ref()) {
            frame.render_widget(Image::new(proto), icon_area);
        }

        // Text
        let text_x = inner.x + 8;
        let text_w = inner.width.saturating_sub(9);
        let sel_style = if selected {
            Style::default().bg(Color::Blue).fg(Color::White)
        } else {
            Style::default()
        };
        let prefix = if selected { "▸ " } else { "  " };
        let license = r.license.as_deref().unwrap_or("-");
        let latest = r.latest_version.as_deref().unwrap_or("-");
        let loaders_str = if r.loaders.is_empty() { "-".to_owned() } else { r.loaders.join(", ") };

        let (icon_char, icon_color) = match r.project_type.to_lowercase().as_str() {
            "mod" => ('M', Color::Blue),
            "resourcepack" | "resource-pack" | "texture-pack" => ('R', Color::Green),
            "shader" => ('S', Color::Yellow),
            "datapack" | "data-pack" => ('D', Color::Magenta),
            _ => ('?', Color::DarkGray),
        };

        let mut badges = String::new();
        if r.cross.modrinth_downloads > 0 || r.cross.modrinth_url.is_some() { badges.push_str("[M] "); }
        if r.cross.curseforge_downloads > 0 || r.cross.curseforge_url.is_some() { badges.push_str("[C] "); }
        let mut stats_parts: Vec<String> = Vec::new();
        if r.cross.modrinth_downloads > 0 { stats_parts.push(format!("M:{}↓", r.cross.modrinth_downloads)); }
        if r.cross.curseforge_downloads > 0 { stats_parts.push(format!("C:{}↓", r.cross.curseforge_downloads)); }
        if stats_parts.is_empty() { stats_parts.push(format!("{}↓", r.downloads)); }
        if r.follows > 0 { stats_parts.push(format!("{}★", r.follows)); }
        let stats_str = stats_parts.join("  ");
        let title_line = Line::from(vec![
            Span::styled(format!("{}{}{}", prefix, r.title, badges), sel_style.add_modifier(Modifier::BOLD)),
            Span::raw("  "),
            Span::styled(format!("[{}]", icon_char), Style::default().fg(icon_color)),
            Span::raw("  "),
            Span::styled(license, Style::default().fg(Color::Cyan)),
            Span::raw("  "),
            Span::styled(stats_str, Style::default().fg(Color::Green)),
        ]);
        let meta_line = Line::from(vec![
            Span::styled(format!("  {}  ", r.author), Style::default().fg(Color::Gray)),
            Span::styled(format!("MC: {latest}"), Style::default().fg(Color::Yellow)),
            Span::styled(format!("  Loader: {loaders_str}"), Style::default().fg(Color::Magenta)),
        ]);
        let mut url_lines: Vec<Line> = Vec::new();
        if selected {
            if let Some(ref u) = r.cross.modrinth_url {
                url_lines.push(Line::from(vec![Span::styled(format!("   M: {u}"), Style::default().fg(Color::Blue).underlined())]));
            }
            if let Some(ref u) = r.cross.curseforge_url {
                url_lines.push(Line::from(vec![Span::styled(format!("   C: {u}"), Style::default().fg(Color::Blue).underlined())]));
            }
            if url_lines.is_empty() {
                if let Some(ref u) = r.url {
                    url_lines.push(Line::from(vec![Span::styled(format!("   {u}"), Style::default().fg(Color::Blue).underlined())]));
                }
            }
        }
        let mut all_lines = vec![title_line, meta_line];
        all_lines.extend(url_lines);
        while all_lines.len() < item_lines {
            all_lines.push(Line::from(""));
        }
        let text_area = Rect::new(text_x, y, text_w, item_lines as u16);
        frame.render_widget(Paragraph::new(all_lines).wrap(Wrap { trim: false }), text_area);
    }
}

fn render_status(frame: &mut ratatui::Frame, area: Rect, app: &App) {
    let (text, style) = match &app.status {
        Status::Idle => (
            " Type a query and press Enter to search. s:search f:filters r:results  ↑↓ scroll  Tab:nav ".to_owned(),
            Style::default().fg(Color::DarkGray),
        ),
        Status::Searching => (" Searching... ".to_owned(), Style::default().fg(Color::Yellow)),
        Status::Done => (
            format!(" {} results — s:search f:filters r:results  ↑↓ scroll  Tab:nav ", app.results.len()),
            Style::default().fg(Color::DarkGray),
        ),
        Status::ApiKeyPrompt => (
            " Paste your CurseForge API key above and press Enter ".to_owned(),
            Style::default().fg(Color::Yellow),
        ),
        Status::Error(_) => (
            " Fix the error above and search again ".to_owned(),
            Style::default().fg(Color::Red),
        ),
    };
    frame.render_widget(Paragraph::new(Line::from(Span::styled(text, style))), area);
}
