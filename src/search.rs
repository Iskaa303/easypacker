use crate::api::filters;
use crate::api::{
    CurseForgeClient, ModrinthClient, Platform, ProjectFile, SearchFilters, SearchResult,
};
use crate::app::App;
use crate::config::Config;
use crate::types::*;
use crossterm::event::KeyCode;
use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, Paragraph, Wrap};
use ratatui_image::Image;
use std::collections::HashSet;
use std::time::Duration;

pub(crate) const TICK_RATE: Duration = Duration::from_millis(100);

pub(crate) fn handle_browse_key(browse: &mut BrowseState, key: KeyCode) -> BrowseAction {
    match key {
        KeyCode::Esc => BrowseAction::Close,
        KeyCode::Char('\n') | KeyCode::Enter => {
            if browse.filtered.is_empty() {
                return BrowseAction::None;
            }
            let idx = browse.filtered[browse.selected];
            if browse.kind == FilterKind::Version || browse.kind == FilterKind::Type {
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
            let _values: Vec<String> = {
                let mut v: Vec<String> = browse
                    .toggled
                    .iter()
                    .map(|&i| browse.options[i].clone())
                    .collect();
                v.sort();
                v
            };
            let kind = browse.kind.clone();
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
            .filter(|(_, opt)| {
                opt.to_lowercase()
                    .contains(&browse.filter_text.to_lowercase())
            })
            .map(|(i, _)| i)
            .collect();
    }
    if browse.selected >= browse.filtered.len() {
        browse.selected = browse.filtered.len().saturating_sub(1);
    }
}

pub(crate) async fn handle_key(
    app: &mut App,
    key: KeyCode,
    cfg: &Config,
    tx: &tokio::sync::mpsc::Sender<AppEvent>,
) {
    match app.focus {
        Focus::Neutral => match key {
            KeyCode::Char('q') => {}
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
            KeyCode::Enter => {
                app.search_offset = 0;
                start_search(app, cfg, tx).await;
            }
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
                let current_val = match &kind {
                    FilterKind::Version => &app.filters.version,
                    FilterKind::Loader => &app.filters.loader,
                    FilterKind::Type => &app.filters.project_type,
                    FilterKind::Platform => &app.filters.platform,
                };
                let saved: HashSet<String> = current_val
                    .split(", ")
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
                    .collect();
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
                    FilterKind::Version => {
                        filters::VERSIONS.iter().map(|s| s.to_string()).collect()
                    }
                    FilterKind::Loader => filters::LOADERS.iter().map(|s| s.to_string()).collect(),
                    FilterKind::Type => filters::TYPES.iter().map(|s| s.to_string()).collect(),
                    FilterKind::Platform => vec!["modrinth".into(), "curseforge".into()],
                };
                let _ = tx.try_send(AppEvent::BrowseOptions {
                    kind,
                    options,
                    saved,
                });
            }
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
                if app.selected + 5 >= app.results.len() && app.status != Status::Searching {
                    start_search(app, cfg, tx).await;
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
            KeyCode::Enter => {
                let idx = app.selected;
                if idx < app.results.len() {
                    start_file_fetch(app, cfg, tx).await;
                }
            }
            _ => {}
        },
    }
}

pub(crate) async fn handle_apikey_key(
    app: &mut App,
    key: KeyCode,
    cfg: &mut Config,
    _tx: &tokio::sync::mpsc::Sender<AppEvent>,
) {
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
                let new_cfg = Config {
                    curseforge_api_key: Some(key),
                };
                if new_cfg.save().is_ok() {
                    if let Ok(loaded) = Config::load() {
                        *cfg = loaded;
                    }
                    app.status = Status::Idle;
                    app.api_key_input.clear();
                } else {
                    app.status =
                        Status::Error("Failed to save API key to ~/.easypacker.json".into());
                }
            }
        }
        _ => {}
    }
}

pub(crate) async fn start_search(
    app: &mut App,
    cfg: &Config,
    tx: &tokio::sync::mpsc::Sender<AppEvent>,
) {
    if app.query.is_empty() {
        return;
    }

    app.status = Status::Searching;

    let filters = SearchFilters {
        query: app.query.clone(),
        version: if app.filters.version.is_empty() {
            None
        } else {
            Some(app.filters.version.clone())
        },
        loader: {
            let pt = if app.filters.project_type.is_empty() {
                "mod"
            } else {
                &app.filters.project_type
            };
            if pt == "mod" && !app.filters.loader.is_empty() {
                Some(app.filters.loader.clone())
            } else {
                None
            }
        },
        project_type: if app.filters.project_type.is_empty() {
            Some("mod".into())
        } else {
            Some(app.filters.project_type.clone())
        },
        sort: "relevance".into(),
        limit: 25,
        offset: app.search_offset,
    };

    let tx = tx.clone();
    let platforms: Vec<String> = app
        .filters
        .platform
        .split(", ")
        .map(|s| s.to_string())
        .collect();

    if cfg.get_api_key(None).is_err() && platforms.iter().any(|p| p == "curseforge") {
        app.status = Status::ApiKeyPrompt;
        return;
    }
    let api_key = cfg.get_api_key(None).ok();
    let current_offset = app.search_offset;
    app.search_offset += 25;
    tokio::spawn(async move {
        let mut all: Vec<SearchResult> = Vec::new();
        for p in &platforms {
            let event = match p.as_str() {
                "modrinth" => match ModrinthClient::search(&filters).await {
                    Ok(results) => Some(results),
                    Err(e) => {
                        let _ = tx
                            .send(AppEvent::Error(format!("Modrinth error: {e}")))
                            .await;
                        continue;
                    }
                },
                "curseforge" => {
                    let key = match &api_key {
                        Some(k) => k,
                        None => {
                            let _ = tx
                                .send(AppEvent::Error("CurseForge API key not configured.".into()))
                                .await;
                            continue;
                        }
                    };
                    match CurseForgeClient::new(key).search(&filters).await {
                        Ok(results) => Some(results),
                        Err(e) => {
                            let _ = tx
                                .send(AppEvent::Error(format!("CurseForge error: {e}")))
                                .await;
                            continue;
                        }
                    }
                }
                _ => None,
            };
            if let Some(mut results) = event {
                if let Some(ref pt) = filters.project_type {
                    let allowed: Vec<&str> = pt.split(", ").map(|s| s.trim()).collect();
                    results.retain(|r| allowed.contains(&r.project_type.to_lowercase().as_str()));
                }
                for r in results {
                    let lower = r.title.to_lowercase();
                    if let Some(existing) = all.iter_mut().find(|e| e.title.to_lowercase() == lower)
                    {
                        match r.platform {
                            Platform::Modrinth => {
                                existing.cross.modrinth_downloads = r.downloads;
                                existing.cross.modrinth_url = r.url;
                                if r.cross.modrinth_slug.is_some() {
                                    existing.cross.modrinth_slug = r.cross.modrinth_slug.clone();
                                }
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
            return;
        }
        let _ = tx
            .send(AppEvent::Results {
                results: all,
                offset: current_offset,
            })
            .await;
    });
}

async fn start_file_fetch(app: &mut App, cfg: &Config, tx: &tokio::sync::mpsc::Sender<AppEvent>) {
    let idx = app.selected;
    if idx >= app.results.len() {
        return;
    }
    let r = &app.results[idx];
    let project_title = r.title.clone();
    let version_filter = app.filters.version.clone();
    let loader_filter = if r.project_type == "mod" {
        app.filters.loader.clone()
    } else {
        String::new()
    };
    let modrinth_slug = r.cross.modrinth_slug.clone();
    let curseforge_id = r.cross.curseforge_id;
    let project_type = r.project_type.clone();
    let api_key = cfg.get_api_key(None).ok();
    let tx = tx.clone();
    tokio::spawn(async move {
        let mut all_files: Vec<ProjectFile> = Vec::new();
        if let Some(ref slug) = modrinth_slug {
            match ModrinthClient::get_versions(slug).await {
                Ok(files) => {
                    for mv in files {
                        let pf = ProjectFile {
                            name: mv.name,
                            mc_versions: mv.game_versions,
                            loaders: mv.loaders,
                            date_published: mv.date_published,
                            downloads: mv.downloads,
                            url: mv.url.clone(),
                            platforms: vec![Platform::Modrinth],
                            modrinth_version_id: Some(mv.id),
                            modrinth_url: mv.url,
                            curseforge_file_id: None,
                            curseforge_url: None,
                        };
                        let lower = pf.name.to_lowercase();
                        if let Some(existing) = all_files
                            .iter_mut()
                            .find(|e: &&mut ProjectFile| e.name.to_lowercase() == lower)
                        {
                            if !existing.platforms.contains(&Platform::Modrinth) {
                                existing.platforms.push(Platform::Modrinth);
                            }
                            if pf.downloads > existing.downloads {
                                existing.downloads = pf.downloads;
                            }
                            if pf.modrinth_version_id.is_some() {
                                existing.modrinth_version_id = pf.modrinth_version_id.clone();
                                existing.modrinth_url = pf.modrinth_url.clone();
                            }
                            if pf.url.is_some() {
                                existing.url = pf.url.clone();
                            }
                        } else {
                            all_files.push(pf);
                        }
                    }
                }
                Err(e) => {
                    eprintln!("Modrinth get_versions error: {e}");
                    let _ = tx.send(AppEvent::Error(format!("Modrinth: {e}"))).await;
                }
            }
        }
        if let Some(id) = curseforge_id {
            if let Some(ref key) = api_key {
                let client = CurseForgeClient::new(key);
                match client.get_files(id).await {
                    Ok(files) => {
                        for cf in files {
                            let known: Vec<&str> = filters::LOADERS.to_vec();
                            let loaders: Vec<String> = cf
                                .game_versions
                                .iter()
                                .filter(|v| known.iter().any(|l| v.eq_ignore_ascii_case(l)))
                                .map(|v| v.to_lowercase())
                                .collect();
                            let mc_versions: Vec<String> = cf
                                .game_versions
                                .into_iter()
                                .filter(|v| !known.iter().any(|l| v.eq_ignore_ascii_case(l)))
                                .collect();
                            let pf = ProjectFile {
                                name: cf.display_name,
                                mc_versions,
                                loaders,
                                date_published: cf.file_date,
                                downloads: cf.download_count,
                                url: cf.download_url.clone(),
                                platforms: vec![Platform::CurseForge],
                                modrinth_version_id: None,
                                modrinth_url: None,
                                curseforge_file_id: Some(cf.id),
                                curseforge_url: cf.download_url,
                            };
                            let lower = pf.name.to_lowercase();
                            if let Some(existing) = all_files
                                .iter_mut()
                                .find(|e: &&mut ProjectFile| e.name.to_lowercase() == lower)
                            {
                                if !existing.platforms.contains(&Platform::CurseForge) {
                                    existing.platforms.push(Platform::CurseForge);
                                }
                                if pf.downloads > existing.downloads {
                                    existing.downloads = pf.downloads;
                                }
                                if pf.curseforge_file_id.is_some() {
                                    existing.curseforge_file_id = pf.curseforge_file_id;
                                    existing.curseforge_url = pf.curseforge_url.clone();
                                }
                                if pf.url.is_some() {
                                    existing.url = pf.url.clone();
                                }
                            } else {
                                all_files.push(pf);
                            }
                        }
                    }
                    Err(e) => {
                        let _ = tx.send(AppEvent::Error(format!("CurseForge: {e}"))).await;
                    }
                }
            }
        }
        let filtered: Vec<ProjectFile> = all_files
            .into_iter()
            .filter(|f| {
                let mc_ok =
                    version_filter.is_empty() || f.mc_versions.iter().any(|v| v == &version_filter);
                let loader_ok =
                    loader_filter.is_empty() || f.loaders.iter().any(|l| l == &loader_filter);
                mc_ok && loader_ok
            })
            .collect();
        let _ = tx
            .send(AppEvent::FileResults {
                files: filtered,
                project_title,
                modrinth_slug,
                curseforge_id,
                project_type,
            })
            .await;
    });
}

// ── Search Screen Rendering ────────────────────────────────

pub(crate) fn render_search_screen(frame: &mut Frame, app: &App) {
    let area = frame.area();
    let constraints = vec![
        Constraint::Length(3),
        Constraint::Length(3),
        Constraint::Length(7),
        Constraint::Min(1),
        Constraint::Length(1),
    ];
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
    } else if let Status::Error(ref msg) = app.status {
        render_error(frame, main_chunks[3], msg.as_str());
    } else {
        render_results(frame, main_chunks[3], app);
    }
    render_status(frame, main_chunks[4], app);
}

fn render_header(frame: &mut Frame, area: Rect, app: &App) {
    let neutral = app.focus == Focus::Neutral;
    let indicator = if neutral {
        " ◇".to_owned()
    } else {
        String::new()
    };
    let mut parts = vec![
        Span::styled(
            format!(" easypacker{} ", indicator),
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(" │ ", Style::default().fg(Color::DarkGray)),
    ];
    if neutral {
        parts.push(Span::styled(
            "s:search f:filters r:results ",
            Style::default().fg(Color::DarkGray),
        ));
        parts.push(Span::styled(
            "q:back",
            Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
        ));
    } else {
        parts.push(Span::styled(
            "Esc:neutral",
            Style::default().fg(Color::DarkGray),
        ));
    }
    let line = Line::from(parts);
    frame.render_widget(
        Paragraph::new(line).block(Block::default().borders(Borders::BOTTOM)),
        area,
    );
}

fn render_search(frame: &mut Frame, area: Rect, app: &App) {
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

fn render_filters(frame: &mut Frame, area: Rect, app: &App) {
    let focused = app.focus == Focus::Filters;
    let border_style = if focused {
        Style::default().fg(Color::Yellow)
    } else {
        Style::default().fg(Color::DarkGray)
    };
    let hint = if focused { " Enter:browse" } else { "" };
    let lines = vec![
        filter_line(
            "Version",
            &app.filters.version,
            focused,
            app.filter_selected == 0,
        ),
        filter_line(
            "Loader",
            &app.filters.loader,
            focused,
            app.filter_selected == 1,
        ),
        filter_line(
            "Type",
            &app.filters.project_type,
            focused,
            app.filter_selected == 2,
        ),
        filter_line(
            "Platform",
            &app.filters.platform,
            focused,
            app.filter_selected == 3,
        ),
    ];
    frame.render_widget(
        Paragraph::new(lines).block(
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

fn render_apikey_prompt(frame: &mut Frame, area: Rect, app: &App) {
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
    frame.render_widget(
        Paragraph::new(text).block(block).wrap(Wrap { trim: false }),
        area,
    );
}

fn render_browse(frame: &mut Frame, area: Rect, browse: &BrowseState) {
    let title = format!(
        " {} — type:filter, Enter:toggle, Tab:apply, Esc:cancel ",
        browse.kind.label()
    );
    if browse.options.is_empty() {
        let p = Paragraph::new(" Loading options… ")
            .block(Block::default().borders(Borders::ALL).title(title.as_str()));
        frame.render_widget(p, area);
        return;
    }
    let list_h = area.height.saturating_sub(2) as usize;
    let slice_start = browse.scroll.min(browse.filtered.len().saturating_sub(1));
    let visible: &[usize] = &browse.filtered[slice_start..];
    let vis_count = visible.len().min(list_h);
    let items: Vec<ListItem> = visible[..vis_count]
        .iter()
        .enumerate()
        .map(|(i, opt_idx)| {
            let opt = &browse.options[*opt_idx];
            let checked = if browse.toggled.contains(opt_idx) {
                "[x]"
            } else {
                "[ ]"
            };
            let is_sel = slice_start + i == browse.selected;
            let prefix = if is_sel { "▸ " } else { "  " };
            let style = if is_sel {
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
    let list_area = Rect::new(
        area.x,
        area.y + 1,
        area.width,
        area.height.saturating_sub(1),
    );
    let list = List::new(items).block(Block::default().borders(Borders::ALL).title(title.as_str()));
    frame.render_widget(list, list_area);
}

fn render_error(frame: &mut Frame, area: Rect, msg: &str) {
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

fn render_results(frame: &mut Frame, area: Rect, app: &App) {
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

    let max_items = (inner.height as usize)
        .saturating_div(3)
        .min(app.results.len().saturating_sub(app.scroll));
    app.visible_count.set(max_items);
    let mut current_y = inner.y;
    for vis in 0..max_items {
        let idx = app.scroll + vis;
        if idx >= app.results.len() {
            break;
        }
        let r = &app.results[idx];
        let selected = idx == app.selected && focused;

        let has_url =
            r.cross.modrinth_url.is_some() || r.cross.curseforge_url.is_some() || r.url.is_some();
        let item_lines: usize = if selected && has_url { 4 } else { 3 };
        let y = current_y;
        current_y += item_lines as u16;

        let icon_area = Rect::new(inner.x + 1, y, 6, 2);
        if let Some(ref proto) = app.proto_cache.get(idx).and_then(|p| p.as_ref()) {
            frame.render_widget(Image::new(proto), icon_area);
        }

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
        let loaders_str = if r.loaders.is_empty() {
            "-".to_owned()
        } else {
            r.loaders.join(", ")
        };

        let (icon_char, icon_color) = match r.project_type.to_lowercase().as_str() {
            "mod" => ('M', Color::Blue),
            "resourcepack" | "resource-pack" | "texture-pack" => ('R', Color::Green),
            "shader" => ('S', Color::Yellow),
            "datapack" | "data-pack" => ('D', Color::Magenta),
            _ => ('?', Color::DarkGray),
        };

        let mut badges = String::new();
        if r.cross.modrinth_downloads > 0 || r.cross.modrinth_url.is_some() {
            badges.push_str("[M] ");
        }
        if r.cross.curseforge_downloads > 0 || r.cross.curseforge_url.is_some() {
            badges.push_str("[C] ");
        }
        let mut stats_parts: Vec<String> = Vec::new();
        if r.cross.modrinth_downloads > 0 {
            stats_parts.push(format!("M:{}↓", r.cross.modrinth_downloads));
        }
        if r.cross.curseforge_downloads > 0 {
            stats_parts.push(format!("C:{}↓", r.cross.curseforge_downloads));
        }
        if stats_parts.is_empty() {
            stats_parts.push(format!("{}↓", r.downloads));
        }
        if r.follows > 0 {
            stats_parts.push(format!("{}★", r.follows));
        }
        let stats_str = stats_parts.join("  ");

        let title_line = Line::from(vec![
            Span::styled(
                format!("{}{}{}", prefix, r.title, badges),
                sel_style.add_modifier(Modifier::BOLD),
            ),
            Span::raw("  "),
            Span::styled(format!("[{}]", icon_char), Style::default().fg(icon_color)),
            Span::raw("  "),
            Span::styled(license, Style::default().fg(Color::Cyan)),
            Span::raw("  "),
            Span::styled(stats_str, Style::default().fg(Color::Green)),
        ]);
        let meta_line = Line::from(vec![
            Span::styled(
                format!("  {}  ", r.author),
                Style::default().fg(Color::Gray),
            ),
            Span::styled(format!("MC: {latest}"), Style::default().fg(Color::Yellow)),
            Span::styled(
                format!("  Loader: {loaders_str}"),
                Style::default().fg(Color::Magenta),
            ),
        ]);
        let mut url_lines: Vec<Line> = Vec::new();
        if selected {
            if let Some(ref u) = r.cross.modrinth_url {
                url_lines.push(Line::from(vec![Span::styled(
                    format!("   M: {u}"),
                    Style::default().fg(Color::Blue).underlined(),
                )]));
            }
            if let Some(ref u) = r.cross.curseforge_url {
                url_lines.push(Line::from(vec![Span::styled(
                    format!("   C: {u}"),
                    Style::default().fg(Color::Blue).underlined(),
                )]));
            }
            if url_lines.is_empty() {
                if let Some(ref u) = r.url {
                    url_lines.push(Line::from(vec![Span::styled(
                        format!("   {u}"),
                        Style::default().fg(Color::Blue).underlined(),
                    )]));
                }
            }
        }
        let mut all_lines = vec![title_line, meta_line];
        all_lines.extend(url_lines);
        while all_lines.len() < item_lines {
            all_lines.push(Line::from(""));
        }
        let text_area = Rect::new(text_x, y, text_w, item_lines as u16);
        frame.render_widget(
            Paragraph::new(all_lines).wrap(Wrap { trim: false }),
            text_area,
        );
    }
}

fn render_status(frame: &mut Frame, area: Rect, app: &App) {
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

pub(crate) fn render_file_browse(frame: &mut Frame, app: &App) {
    let area = frame.area();
    let Some(ref fb) = app.file_browse else {
        return;
    };
    let added = if fb.already_added {
        " [ALREADY IN MODPACK]"
    } else {
        ""
    };
    let header = format!(
        " {}{} — Esc:back  ↑↓:scroll  Enter:add  ({})",
        fb.project_title,
        added,
        fb.files.len()
    );
    let lines: Vec<Line> = fb
        .files
        .iter()
        .enumerate()
        .skip(fb.scroll)
        .take(area.height.saturating_sub(2) as usize)
        .map(|(i, f)| {
            let is_sel = i == fb.selected;
            let prefix = if is_sel { "▸ " } else { "  " };
            let bg = if is_sel {
                Style::default().bg(Color::Blue)
            } else {
                Style::default()
            };
            let mc = f.mc_versions.first().map(|s| s.as_str()).unwrap_or("-");
            let loaders = f.loaders.join(", ");
            let date = &f.date_published[..f.date_published.len().min(10)];
            let dl = f.downloads;
            Line::from(vec![
                Span::styled(prefix, bg.fg(Color::Yellow)),
                Span::styled(
                    f.name.clone(),
                    bg.fg(Color::White).add_modifier(Modifier::BOLD),
                ),
                Span::styled(format!("  MC:{mc}"), bg.fg(Color::Yellow)),
                Span::styled(
                    if loaders.is_empty() {
                        String::new()
                    } else {
                        format!("  {loaders}")
                    },
                    bg.fg(Color::Magenta),
                ),
                Span::styled(format!("  {date}"), bg.fg(Color::DarkGray)),
                Span::styled(format!("  {dl}↓"), bg.fg(Color::Green)),
                Span::styled(
                    [
                        if f.platforms.contains(&Platform::Modrinth) {
                            " [M]"
                        } else {
                            ""
                        },
                        if f.platforms.contains(&Platform::CurseForge) {
                            " [C]"
                        } else {
                            ""
                        },
                    ]
                    .concat(),
                    bg.fg(Color::Cyan),
                ),
            ])
        })
        .collect();
    let block = Block::default()
        .borders(Borders::ALL)
        .title(header.as_str())
        .border_style(Style::default().fg(Color::Cyan));
    frame.render_widget(
        Paragraph::new(lines)
            .block(block)
            .wrap(Wrap { trim: false }),
        area,
    );
}
