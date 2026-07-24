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
    category: String,
    loader: String,
    project_type: String,
    platform: String,
}

impl Default for FiltersState {
    fn default() -> Self {
        Self {
            version: String::new(),
            category: String::new(),
            loader: String::new(),
            project_type: "mod".into(),
            platform: "modrinth, curseforge".into(),
        }
    }
}

#[derive(Clone, PartialEq)]
enum FilterKind {
    Version,
    Category,
    Loader,
    Type,
    Platform,
}

impl FilterKind {
    fn label(&self) -> &str {
        match self {
            FilterKind::Version => "Version",
            FilterKind::Category => "Category",
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
                                    FilterKind::Category => app.filters.category = val,
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
            // Toggle selection
            if browse.toggled.contains(&idx) {
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
            let val = values.join(", ");
            let kind = browse.kind.clone();
            if val.is_empty() {
                BrowseAction::None
            } else {
                BrowseAction::Toggle(kind)
            }
        }
        KeyCode::Tab => BrowseAction::Close,
        KeyCode::Up => {
            if browse.selected > 0 {
                browse.selected -= 1;
            }
            BrowseAction::None
        }
        KeyCode::Down => {
            let max = browse.filtered.len().saturating_sub(1);
            if browse.selected < max {
                browse.selected += 1;
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
            KeyCode::Down => app.filter_selected = app.filter_selected.saturating_add(1).min(4),
            KeyCode::Up => app.filter_selected = app.filter_selected.saturating_sub(1),
            KeyCode::Enter => {
                let kind = match app.filter_selected {
                    0 => FilterKind::Version,
                    1 => FilterKind::Category,
                    2 => FilterKind::Loader,
                    3 => FilterKind::Type,
                    _ => FilterKind::Platform,
                };
                // Pre-populate toggled from current filter value
                let current_val = match &kind {
                    FilterKind::Version => &app.filters.version,
                    FilterKind::Category => &app.filters.category,
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
                });
                let tx = tx.clone();
                let platforms = app.filters.platform.clone();
                let api_key = cfg.get_api_key(None).ok().map(|k| k.clone());
                tokio::spawn(async move {
                    let options = fetch_options(&kind, &platforms, api_key.as_deref()).await.unwrap_or_default();
                    let _ = tx.send(AppEvent::BrowseOptions { kind, options, saved }).await;
                });
            }
            KeyCode::Char(c) => match app.filter_selected {
                0 => app.filters.version.push(c),
                1 => app.filters.category.push(c),
                2 => app.filters.loader.push(c),
                3 => app.filters.project_type.push(c),
                4 => app.filters.platform.push(c),
                _ => {}
            },
            KeyCode::Backspace => match app.filter_selected {
                0 => { let _ = app.filters.version.pop(); }
                1 => { let _ = app.filters.category.pop(); }
                2 => { let _ = app.filters.loader.pop(); }
                3 => { let _ = app.filters.project_type.pop(); }
                4 => { let _ = app.filters.platform.pop(); }
                _ => {}
            },
            KeyCode::Delete => match app.filter_selected {
                0 => { let _ = app.filters.version.pop(); }
                1 => { let _ = app.filters.category.pop(); }
                2 => { let _ = app.filters.loader.pop(); }
                3 => { let _ = app.filters.project_type.pop(); }
                4 => { let _ = app.filters.platform.pop(); }
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
                if app.selected >= app.scroll + 10 {
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
                if app.selected >= app.scroll + 10 {
                    app.scroll = app.selected.saturating_sub(9);
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
        category: if app.filters.category.is_empty() { None } else { Some(app.filters.category.clone()) },
        loader: if app.filters.loader.is_empty() { None } else { Some(app.filters.loader.clone()) },
        project_type: if app.filters.project_type.is_empty() { Some("mod".into()) } else { Some(app.filters.project_type.clone()) },
        sort: "relevance".into(),
        limit: 25,
        offset: 0,
    };

    let tx = tx.clone();
    let api_key = cfg.get_api_key(None).ok();
    let platforms: Vec<String> = app.filters.platform.split(", ").map(|s| s.to_string()).collect();

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
                results.retain(|r| {
                    let t = r.project_type.to_lowercase();
                    let allowed = ["mod", "resourcepack", "shader", "datapack", "world"];
                    allowed.contains(&t.as_str())
                        && !r.title.to_lowercase().contains("modpack")
                        && !r.description.to_lowercase().contains("modpack")
                });
                // Merge: if same title exists, fold in platform-specific data
                for r in results {
                    let lower = r.title.to_lowercase();
                    if let Some(existing) = all.iter_mut().find(|e| e.title.to_lowercase() == lower) {
                        match r.platform {
                            Platform::Modrinth => {
                                existing.modrinth_downloads = r.downloads;
                                existing.modrinth_url = r.url;
                            }
                            Platform::CurseForge => {
                                existing.curseforge_downloads = r.downloads;
                                existing.curseforge_url = r.url;
                            }
                        }
                    } else {
                        let mut entry = r;
                        match entry.platform {
                            Platform::Modrinth => {
                                entry.modrinth_downloads = entry.downloads;
                                entry.modrinth_url = entry.url.clone();
                            }
                            Platform::CurseForge => {
                                entry.curseforge_downloads = entry.downloads;
                                entry.curseforge_url = entry.url.clone();
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

async fn fetch_options(kind: &FilterKind, _platforms: &str, api_key: Option<&str>) -> Result<Vec<String>> {
    match kind {
        FilterKind::Platform => Ok(vec!["modrinth".into(), "curseforge".into()]),
        FilterKind::Version | FilterKind::Category | FilterKind::Loader | FilterKind::Type => {
            // Try each active platform for options
            for p in _platforms.split(", ") {
                match p {
                    "modrinth" => match fetch_modrinth_options(kind).await {
                        Ok(opts) if !opts.is_empty() => return Ok(opts),
                        _ => {}
                    },
                    "curseforge" => {
                        let opts = fetch_curseforge_options(kind, api_key).await;
                        if !opts.is_empty() {
                            return Ok(opts);
                        }
                    }
                    _ => {}
                }
            }
            Ok(vec![])
        }
    }
}

async fn fetch_modrinth_options(kind: &FilterKind) -> Result<Vec<String>> {
    let client = reqwest::Client::new();
    match kind {
        FilterKind::Version => {
            let resp: Vec<serde_json::Value> = client
                .get("https://api.modrinth.com/v2/tag/game_version")
                .send()
                .await?
                .json()
                .await?;
            let mut versions: Vec<String> = resp
                .into_iter()
                .filter_map(|v| v["version"].as_str().map(String::from))
                .collect();
            versions.reverse();
            Ok(versions)
        }
        FilterKind::Category => {
            let resp: Vec<serde_json::Value> = client
                .get("https://api.modrinth.com/v2/tag/category")
                .send()
                .await?
                .json()
                .await?;
            let mut seen = std::collections::HashSet::new();
            let cats: Vec<String> = resp
                .into_iter()
                .filter_map(|c| c["name"].as_str().map(String::from))
                .filter(|n| seen.insert(n.clone()))
                .collect();
            Ok(cats)
        }
        FilterKind::Loader => {
            let resp: Vec<serde_json::Value> = client
                .get("https://api.modrinth.com/v2/tag/loader")
                .send()
                .await?
                .json()
                .await?;
            Ok(resp
                .into_iter()
                .filter_map(|l| l["name"].as_str().map(String::from))
                .collect())
        }
        FilterKind::Type => Ok(vec![
            "mod".into(),
            "resourcepack".into(),
            "shader".into(),
            "datapack".into(),
        ]),
        FilterKind::Platform => Ok(vec![]),
    }
}

async fn fetch_curseforge_options(kind: &FilterKind, _api_key: Option<&str>) -> Vec<String> {
    match kind {
        FilterKind::Version => {
            vec!["26.2", "26.1.2", "26.1.1", "26.1", "1.21.11", "1.21.10", "1.21.9", "1.21.8", "1.21.7", "1.21.6", "1.21.5", "1.21.4", "1.21.3", "1.21.2", "1.21.1", "1.21", "1.20.6", "1.20.5", "1.20.4", "1.20.3", "1.20.2", "1.20.1", "1.20", "1.19.4", "1.19.3", "1.19.2", "1.19.1", "1.19", "1.18.2", "1.18.1", "1.18", "1.17.1", "1.17", "1.16.5", "1.16.4", "1.16.3", "1.16.2", "1.16.1", "1.16", "1.15.2", "1.15.1", "1.15", "1.14.4", "1.14.3", "1.14.2", "1.14.1", "1.14", "1.13.2", "1.13.1", "1.13", "1.12.2", "1.12.1", "1.12", "1.11.2", "1.11.1", "1.11", "1.10.2", "1.10.1", "1.10", "1.9.4", "1.9.3", "1.9.2", "1.9.1", "1.9", "1.8.9", "1.8.8", "1.8.7", "1.8.6", "1.8.5", "1.8.4", "1.8.3", "1.8.2", "1.8.1", "1.8", "1.7.10", "1.7.9", "1.7.8", "1.7.7", "1.7.6", "1.7.5", "1.7.4", "1.7.3", "1.7.2", "1.7.1", "1.7", "1.6.4", "1.6.2", "1.6.1", "1.6", "1.5.3", "1.5.2", "1.5.1", "1.5.0", "1.4.7", "1.4.6", "1.4.5", "1.4.4", "1.4.2", "1.3.2", "1.3.1", "1.2.8", "1.2.5", "1.2.4", "1.2.3", "1.2.2", "1.2.1", "1.2", "1.1", "1.0.0", "1.0", "0.16"]
.iter().map(|s| s.to_string()).collect()
        }
        FilterKind::Category => {
            vec!["Addons", "Applied Energistics 2", "Blood Magic", "Buildcraft", "CraftTweaker", "Create", "Farmer\'s Delight", "Forestry", "Galacticraft", "Industrial Craft", "Integrated Dynamics", "KubeJS", "Refined Storage", "Skyblock", "Thaumcraft", "Thermal Expansion", "Tinker\'s Construct", "Twilight Forest", "Adventure and RPG", "API and Library", "Armor Tools and Weapons", "Bug Fixes", "Cosmetic", "CreativeMode", "Education", "Food", "Horror", "Magic", "Map and Information", "MCreator", "Miscellaneous", "ModJam 2025", "Performance", "Redstone", "Server Utility", "Storage", "Technology", "Automation", "Energy", "Energy Fluid and Item Transport", "Farming", "Genetics", "Player Transport", "Processing", "Twitch Integration", "Utility and QoL", "World Gen", "Biomes", "Dimensions", "Mobs", "Ores and Resources", "Structures"]
.iter().map(|s| s.to_string()).collect()
        }
        FilterKind::Loader => vec!["forge".into(), "fabric".into(), "neoforge".into(), "quilt".into()],
        FilterKind::Type => vec!["mod".into(), "resourcepack".into(), "datapack".into()],
        FilterKind::Platform => vec![],
    }
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

    let filter_display = if browse.filter_text.is_empty() {
        " type to filter…"
    } else {
        // Need to stash the string
        Box::leak(Box::new(format!(" filter: {} ", browse.filter_text)))
    };

    let items: Vec<ListItem> = browse
        .filtered
        .iter()
        .enumerate()
        .map(|(i, opt_idx)| {
            let opt = &browse.options[*opt_idx];
            let checked = if browse.toggled.contains(opt_idx) { "[x]" } else { "[ ]" };
            let selected = i == browse.selected;
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

    // Show header with filter text
    let top_area = Rect::new(area.x, area.y, area.width, 1);
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            filter_display,
            Style::default().fg(Color::Yellow),
        )))
        .block(Block::default()),
        top_area,
    );

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
    let header = Line::from(vec![
        Span::styled(
            format!(" easypacker{} ", neutral_indicator),
            Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
        ),
        Span::styled(" │ Tab:nav Enter:search ", Style::default().fg(Color::DarkGray)),
        if neutral {
            Span::styled("q:quit", Style::default().fg(Color::Red).add_modifier(Modifier::BOLD))
        } else {
            Span::styled("Esc:neutral", Style::default().fg(Color::DarkGray))
        },
    ]);
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
        filter_line("Category", &app.filters.category, focused, app.filter_selected == 1),
        filter_line("Loader", &app.filters.loader, focused, app.filter_selected == 2),
        filter_line("Type", &app.filters.project_type, focused, app.filter_selected == 3),
        filter_line("Platform", &app.filters.platform, focused, app.filter_selected == 4),
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

    let max_visible = (inner.height as usize).saturating_div(3).min(app.results.len().saturating_sub(app.scroll));
    for vis in 0..max_visible {
        let idx = app.scroll + vis;
        if idx >= app.results.len() { break; }
        let r = &app.results[idx];
        let y = inner.y + (vis as u16) * 3;

        // Icon area
        let icon_area = Rect::new(inner.x + 1, y, 6, 2);
        if let Some(ref proto) = app.proto_cache.get(idx).and_then(|p| p.as_ref()) {
            let image = Image::new(proto);
            frame.render_widget(image, icon_area);
        }

        // Text
        let text_x = inner.x + 8;
        let text_w = inner.width.saturating_sub(9);
        let selected = idx == app.selected && focused;
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

        // Build platform badges
        let mut badges = String::new();
        if r.modrinth_downloads > 0 || r.modrinth_url.is_some() { badges.push_str("[M] "); }
        if r.curseforge_downloads > 0 || r.curseforge_url.is_some() { badges.push_str("[C] "); }
        // Build stats line
        let mut stats_parts: Vec<String> = Vec::new();
        if r.modrinth_downloads > 0 {
            stats_parts.push(format!("M:{}↓", r.modrinth_downloads));
        }
        if r.curseforge_downloads > 0 {
            stats_parts.push(format!("C:{}↓", r.curseforge_downloads));
        }
        if stats_parts.is_empty() {
            stats_parts.push(format!("{}↓", r.downloads));
        }
        if r.follows > 0 {
            stats_parts.push(format!("{}★", r.follows));
        }
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
        // Build URL line — show both platform URLs if available
        let mut url_parts: Vec<String> = Vec::new();
        if let Some(ref u) = r.modrinth_url { url_parts.push(format!("M: {u}")); }
        if let Some(ref u) = r.curseforge_url { url_parts.push(format!("C: {u}")); }
        if url_parts.is_empty() {
            if let Some(ref u) = r.url { url_parts.push(u.clone()); }
        }
        let url_line = if selected && !url_parts.is_empty() {
            Line::from(vec![Span::styled(format!("   {}", url_parts.join("  ")), Style::default().fg(Color::Blue).underlined())])
        } else {
            Line::from("")
        };

        let text_area = Rect::new(text_x, y, text_w, 3);
        frame.render_widget(Paragraph::new(vec![title_line, meta_line, url_line]), text_area);
    }
}

fn render_status(frame: &mut ratatui::Frame, area: Rect, app: &App) {
    let (text, style) = match &app.status {
        Status::Idle => (
            " Type a query and press Enter to search. ↑↓ scroll, Tab:nav ".to_owned(),
            Style::default().fg(Color::DarkGray),
        ),
        Status::Searching => (" Searching... ".to_owned(), Style::default().fg(Color::Yellow)),
        Status::Done => (
            format!(" {} results — ↑↓ scroll, Tab:nav ", app.results.len()),
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
