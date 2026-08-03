use crate::app::App;
use crate::config::Config;
use crate::project;
use crate::search;
use crate::types::*;
use crate::ui;
use color_eyre::eyre::Result;
use crossterm::ExecutableCommand;
use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use ratatui::Frame;
use ratatui_image::picker::Picker;
use std::io::stdout;

pub async fn run_tui(_cfg: Config, project: Option<project::Manifest>) -> Result<()> {
    enable_raw_mode()?;
    let mut stdout = stdout();
    stdout.execute(EnterAlternateScreen)?;
    let mut terminal = ratatui::Terminal::new(ratatui::backend::CrosstermBackend::new(stdout))?;

    let (tx, mut rx) = tokio::sync::mpsc::channel(32);

    let has_project = project.is_some();
    let mode = if has_project {
        AppMode::MainMenu
    } else {
        AppMode::Welcome
    };

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
        search_offset: 0,
        visible_count: std::cell::Cell::new(0),
        filter_selected: 0,
        api_key_input: String::new(),
        picker: Picker::from_query_stdio().unwrap_or_else(|_| Picker::halfblocks()),
        proto_cache: Vec::new(),
        mode,
        project,
        welcome_state: ui::WelcomeState::default(),
        menu_selection: 0,
        form: None,
        form_field_idx: None,
        file_browse: None,
        quit_requested: false,
    };

    let res = run(&mut terminal, &mut app, &mut rx, &tx).await;

    disable_raw_mode()?;
    terminal.backend_mut().execute(LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    if let Err(e) = res {
        eprintln!("{e:?}");
    }
    Ok(())
}

async fn run(
    terminal: &mut ratatui::Terminal<ratatui::backend::CrosstermBackend<std::io::Stdout>>,
    app: &mut App,
    rx: &mut tokio::sync::mpsc::Receiver<AppEvent>,
    tx: &tokio::sync::mpsc::Sender<AppEvent>,
) -> Result<()> {
    loop {
        terminal.draw(|f| render(f, app))?;

        if event::poll(search::TICK_RATE)? {
            if let Event::Key(key) = event::read()? {
                if key.kind != KeyEventKind::Press {
                    continue;
                }

                match app.mode {
                    AppMode::Welcome => handle_welcome(app, key.code),
                    AppMode::MainMenu => handle_menu(app, key.code),
                    AppMode::Search => handle_search_mode(app, key.code, rx, tx).await,
                    AppMode::Settings | AppMode::CreateProject => {
                        handle_form_mode(app, key.code, tx).await
                    }
                    AppMode::FileBrowse => match key.code {
                        KeyCode::Up => {
                            if let Some(ref mut fb) = app.file_browse {
                                if fb.selected > 0 {
                                    fb.selected -= 1;
                                    if fb.selected < fb.scroll {
                                        fb.scroll = fb.selected;
                                    }
                                }
                            }
                        }
                        KeyCode::Down => {
                            if let Some(ref mut fb) = app.file_browse {
                                let max = fb.files.len().saturating_sub(1);
                                if fb.selected < max {
                                    fb.selected += 1;
                                }
                                let vis = 15.min(max.saturating_add(1));
                                if fb.selected >= fb.scroll + vis.saturating_sub(1) {
                                    fb.scroll += 1;
                                }
                            }
                        }
                        KeyCode::Esc => {
                            app.mode = AppMode::Search;
                            app.file_browse = None;
                        }
                        KeyCode::Enter => {
                            if let Some(ref mut fb) = app.file_browse {
                                if fb.selected < fb.files.len() {
                                    let f = &fb.files[fb.selected];
                                    let cwd = std::env::current_dir().unwrap_or_default();
                                    if let Ok(mut manifest) = project::Manifest::load(&cwd) {
                                        if !manifest.contains(
                                            &fb.project_type,
                                            fb.modrinth_slug.as_deref(),
                                            fb.curseforge_id.map(i64::from),
                                        ) {
                                            let has_mr = f.modrinth_version_id.is_some()
                                                && fb.modrinth_slug.is_some();
                                            let has_cf = f.curseforge_file_id.is_some()
                                                && fb.curseforge_id.is_some();
                                            let both = has_mr && has_cf;
                                            // Same file on both platforms => shared
                                            // `version`, inherited per platform.
                                            let spec =
                                                project::ModSpec::Detailed(project::DetailedSpec {
                                                    version: both.then(|| f.name.clone()),
                                                    modrinth: has_mr.then(|| {
                                                        project::ModrinthSpec {
                                                            slug: fb.modrinth_slug.clone(),
                                                            version: (!both)
                                                                .then(|| f.name.clone()),
                                                        }
                                                    }),
                                                    curseforge: has_cf.then(|| {
                                                        project::CurseForgeSpec {
                                                            project_id: fb
                                                                .curseforge_id
                                                                .map(i64::from),
                                                            version: (!both)
                                                                .then(|| f.name.clone()),
                                                        }
                                                    }),
                                                });
                                            let key = fb
                                                .modrinth_slug
                                                .clone()
                                                .unwrap_or_else(|| {
                                                    project::slugify(&fb.project_title)
                                                });
                                            manifest
                                                .cat_mut(&fb.project_type)
                                                .insert(key, spec);
                                            if manifest.save(&cwd).is_ok() {
                                                app.project = Some(manifest);
                                            }
                                            fb.already_added = true;
                                        }
                                    }
                                }
                            }
                        }
                        _ => {}
                    },
                }
                if app.quit_requested {
                    break Ok(());
                }
            }
        }

        if let Ok(event) = rx.try_recv() {
            match event {
                AppEvent::Results { results, offset } => {
                    if app.mode != AppMode::Search {
                        continue;
                    }
                    if offset == 0 {
                        app.results = results;
                        app.scroll = 0;
                        app.selected = 0;
                        app.proto_cache = vec![None; app.results.len()];
                    } else {
                        let start = app.results.len();
                        app.results.extend(results);
                        app.proto_cache
                            .extend(vec![None; app.results.len() - start]);
                    }
                    app.status = Status::Done;
                    app.focus = Focus::Results;
                    let tx = tx.clone();
                    let urls: Vec<Option<String>> =
                        app.results.iter().map(|r| r.icon_url.clone()).collect();
                    tokio::spawn(async move {
                        for (i, url) in urls.iter().enumerate() {
                            if let Some(url) = url {
                                if let Ok(resp) = reqwest::get(url).await {
                                    if let Ok(bytes) = resp.bytes().await {
                                        let _ =
                                            tx.send(AppEvent::IconLoaded(i, bytes.to_vec())).await;
                                    }
                                }
                            }
                        }
                    });
                }
                AppEvent::Error(msg) => {
                    if app.mode == AppMode::Search {
                        app.status = Status::Error(msg);
                    }
                }
                AppEvent::FileResults {
                    files,
                    project_title,
                    modrinth_slug,
                    curseforge_id,
                    project_type,
                } => {
                    let cwd = std::env::current_dir().unwrap_or_default();
                    let already = project::Manifest::load(&cwd)
                        .map(|m| {
                            m.contains(
                                &project_type,
                                modrinth_slug.as_deref(),
                                curseforge_id.map(i64::from),
                            )
                        })
                        .unwrap_or(false);
                    app.file_browse = Some(FileBrowseState {
                        project_title,
                        modrinth_slug,
                        curseforge_id,
                        project_type,
                        files,
                        scroll: 0,
                        selected: 0,
                        already_added: already,
                    });
                    app.mode = AppMode::FileBrowse;
                }
                AppEvent::BrowseOptions {
                    kind,
                    options,
                    saved,
                } => {
                    if let Some(ref mut browse) = app.browse_mode {
                        if browse.kind == kind {
                            browse.options = options;
                            browse.filtered = (0..browse.options.len()).collect();
                            browse.selected = 0;
                            browse.scroll = 0;
                            browse.toggled = saved
                                .iter()
                                .filter_map(|s| browse.options.iter().position(|o| o == s))
                                .collect();
                        }
                    }
                }
                AppEvent::IconLoaded(i, bytes) => {
                    if i < app.proto_cache.len() {
                        match image::load_from_memory(&bytes) {
                            Ok(dyn_img) => {
                                let size = ratatui::prelude::Size::new(6, 2);
                                match app.picker.new_protocol(
                                    dyn_img,
                                    size,
                                    ratatui_image::Resize::Fit(None),
                                ) {
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

// ── Render dispatch ────────────────────────────────────────

fn render(frame: &mut Frame, app: &App) {
    match app.mode {
        AppMode::Welcome => {
            ui::render_welcome(frame, frame.area(), &app.welcome_state, app.menu_selection);
        }
        AppMode::MainMenu => {
            let name = app
                .project
                .as_ref()
                .map(|p| p.project.name.as_str())
                .unwrap_or("(no project)");
            ui::render_main_menu(frame, frame.area(), name, app.menu_selection);
        }
        AppMode::FileBrowse => search::render_file_browse(frame, app),
        AppMode::Search => search::render_search_screen(frame, app),
        AppMode::Settings | AppMode::CreateProject => {
            if let Some(ref browse) = app.browse_mode {
                // re-use browse rendering from search module
                // search::render_browse is private; render inline
                render_browse_simple(frame, frame.area(), browse);
            } else if let Some(ref form) = app.form {
                let title = match app.mode {
                    AppMode::Settings => "Project Settings",
                    AppMode::CreateProject => "Create Project",
                    _ => "",
                };
                ui::render_form(frame, frame.area(), title, form);
            }
        }
    }
}

fn render_browse_simple(frame: &mut Frame, area: ratatui::layout::Rect, browse: &BrowseState) {
    let title = format!(
        " {} — type:filter, Enter:toggle, Tab:apply, Esc:cancel ",
        browse.kind.label()
    );
    if browse.options.is_empty() {
        let p = ratatui::widgets::Paragraph::new(" Loading options… ").block(
            ratatui::widgets::Block::default()
                .borders(ratatui::widgets::Borders::ALL)
                .title(title.as_str()),
        );
        frame.render_widget(p, area);
        return;
    }
    let list_h = area.height.saturating_sub(2) as usize;
    let slice_start = browse.scroll.min(browse.filtered.len().saturating_sub(1));
    let visible: &[usize] = &browse.filtered[slice_start..];
    let vis_count = visible.len().min(list_h);
    use ratatui::style::Color;
    use ratatui::text::{Line, Span};
    use ratatui::widgets::{List, ListItem};
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
                ratatui::style::Style::default()
                    .bg(Color::Blue)
                    .fg(Color::White)
            } else {
                ratatui::style::Style::default()
            };
            ListItem::new(Line::from(Span::styled(
                format!("{prefix}{checked} {opt}"),
                style,
            )))
        })
        .collect();
    let list_area = ratatui::layout::Rect::new(
        area.x,
        area.y + 1,
        area.width,
        area.height.saturating_sub(1),
    );
    let list = List::new(items).block(
        ratatui::widgets::Block::default()
            .borders(ratatui::widgets::Borders::ALL)
            .title(title.as_str()),
    );
    frame.render_widget(list, list_area);
}

// ── Mode handlers ──────────────────────────────────────────

fn handle_welcome(app: &mut App, key: KeyCode) {
    let action = ui::handle_welcome_key(&mut app.welcome_state, key, &mut app.menu_selection);
    match action {
        ui::WelcomeAction::Create => {
            app.form = Some(ui::new_create_form());
            app.mode = AppMode::CreateProject;
        }
        ui::WelcomeAction::Open(path) => {
            if let Some(manifest) = project::Manifest::detect(&path) {
                app.project = Some(manifest);
                app.mode = AppMode::MainMenu;
                app.menu_selection = 0;
            } else {
                app.welcome_state.error = Some("No Easypacker.toml found at that path".into());
            }
        }
        ui::WelcomeAction::Quit => app.quit_requested = true,
        ui::WelcomeAction::None => {}
    }
}

// HACK: we need to break from the outer run loop on quit. Using a separate variable.
// Actually let's just use a return on quit. The outer loop will see the mode change.

fn handle_menu(app: &mut App, key: KeyCode) {
    let choice = ui::handle_main_menu_key(key, &mut app.menu_selection);
    match choice {
        Some(0) => {
            app.query.clear();
            app.cursor_pos = 0;
            app.results.clear();
            app.focus = Focus::Neutral;
            app.status = Status::Idle;
            app.search_offset = 0;
            app.scroll = 0;
            app.selected = 0;
            if let Some(ref m) = app.project {
                let proj = &m.project;
                if !proj.minecraft.is_empty() {
                    app.filters.version = proj.minecraft.clone();
                }
                if !proj.loader.is_empty() {
                    app.filters.loader = proj.loader.clone();
                }
                if !proj.platforms.is_empty() {
                    app.filters.platform = proj.platforms.join(", ");
                }
            }
            app.mode = AppMode::Search;
        }
        Some(1) => {
            if let Some(ref m) = app.project {
                let proj = &m.project;
                let form = ui::new_settings_form(
                    &proj.name,
                    &proj.version,
                    &proj.authors,
                    &proj.credits,
                    &proj.description,
                    &proj.minecraft,
                    &proj.loader,
                    &proj.platforms,
                    &proj.links.website,
                    &proj.links.discord,
                    &proj.links.github,
                );
                app.form = Some(form);
                app.mode = AppMode::Settings;
            }
        }
        Some(2) => app.quit_requested = true,
        _ => {}
    }
}

async fn handle_search_mode(
    app: &mut App,
    key: KeyCode,
    _rx: &mut tokio::sync::mpsc::Receiver<AppEvent>,
    tx: &tokio::sync::mpsc::Sender<AppEvent>,
) {
    if app.browse_mode.is_some() {
        let action = {
            let b = app.browse_mode.as_mut().unwrap();
            search::handle_browse_key(b, key)
        };
        match action {
            BrowseAction::Toggle(kind) => {
                if let Some(ref browse) = app.browse_mode {
                    let val: String = {
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
                    if !app.query.is_empty() {
                        app.search_offset = 0;
                        search::start_search(app, &Config::load().unwrap_or_default(), tx).await;
                    }
                }
            }
            BrowseAction::Close => app.browse_mode = None,
            BrowseAction::None => {}
        }
    } else {
        if app.focus == Focus::Neutral {
            match key {
                KeyCode::Char('q') => {
                    app.mode = AppMode::MainMenu;
                    app.menu_selection = 0;
                }
                KeyCode::Char('s') => app.focus = Focus::Query,
                KeyCode::Char('f') => app.focus = Focus::Filters,
                KeyCode::Char('r') => app.focus = Focus::Results,
                KeyCode::Tab => app.focus = Focus::Filters,
                KeyCode::BackTab => app.focus = Focus::Results,
                KeyCode::Esc => {}
                _ => app.focus = Focus::Query,
            }
        } else if app.status == Status::ApiKeyPrompt {
            let mut cfg = Config::load().unwrap_or_default();
            search::handle_apikey_key(app, key, &mut cfg, tx).await;
        } else {
            match key {
                KeyCode::Esc => app.focus = Focus::Neutral,
                KeyCode::Tab => {
                    app.focus = match app.focus {
                        Focus::Query => Focus::Filters,
                        Focus::Filters => Focus::Results,
                        Focus::Results => Focus::Query,
                        _ => Focus::Query,
                    };
                }
                KeyCode::BackTab => {
                    app.focus = match app.focus {
                        Focus::Query => Focus::Results,
                        Focus::Filters => Focus::Query,
                        Focus::Results => Focus::Filters,
                        _ => Focus::Query,
                    };
                }
                _ => {
                    search::handle_key(app, key, &Config::load().unwrap_or_default(), tx).await;
                }
            }
        }
    }
}

async fn handle_form_mode(app: &mut App, key: KeyCode, _tx: &tokio::sync::mpsc::Sender<AppEvent>) {
    if app.browse_mode.is_some() {
        let action = {
            let b = app.browse_mode.as_mut().unwrap();
            search::handle_browse_key(b, key)
        };
        match action {
            BrowseAction::Toggle(_kind) => {
                if let Some(ref browse) = app.browse_mode {
                    let val: String = {
                        let mut v: Vec<String> = browse
                            .toggled
                            .iter()
                            .map(|&i| browse.options[i].clone())
                            .collect();
                        v.sort();
                        v.join(", ")
                    };
                    if let Some(idx) = app.form_field_idx {
                        if let Some(ref mut form) = app.form {
                            if idx < form.fields.len() {
                                form.fields[idx].value = val;
                            }
                        }
                    }
                }
                app.browse_mode = None;
            }
            BrowseAction::Close => app.browse_mode = None,
            BrowseAction::None => {}
        }
    } else if let Some(ref mut form) = app.form {
        let action = ui::handle_form_key(form, key);
        match action {
            ui::FormAction::PickField(idx) => {
                if let Some(field) = form.fields.get(idx) {
                    if let Some((browse, _saved)) = ui::open_browse_for_field(field) {
                        app.browse_mode = Some(browse);
                        app.form_field_idx = Some(idx);
                    }
                }
            }
            ui::FormAction::Save => {
                let is_settings = app.mode == AppMode::Settings;
                if is_settings {
                    if let Some(ref mut m) = app.project {
                        apply_form_to_project(&mut m.project, form);
                        let cwd = std::env::current_dir().unwrap_or_default();
                        if let Err(e) = m.save(&cwd) {
                            eprintln!("Failed to save project: {e}");
                        }
                    }
                } else {
                    let proj = form_to_project(form);
                    let dir = std::env::current_dir().unwrap_or_default();
                    match project::Manifest::init_project(&dir, proj) {
                        Err(e) => eprintln!("Failed to create project: {e}"),
                        Ok(manifest) => app.project = Some(manifest),
                    }
                }
                app.mode = AppMode::MainMenu;
                app.menu_selection = 0;
                app.form = None;
            }
            ui::FormAction::Cancel => {
                app.mode = if app.project.is_some() {
                    AppMode::MainMenu
                } else {
                    AppMode::Welcome
                };
                app.menu_selection = 0;
                app.form = None;
            }
            ui::FormAction::None => {}
        }
    }
}

// ── Form↔Project helpers ───────────────────────────────────

fn apply_form_to_project(proj: &mut project::ModpackProject, form: &ui::FormState) {
    if let Some(f) = form.fields.get(0) {
        proj.name = f.value.clone();
    }
    if let Some(f) = form.fields.get(1) {
        proj.version = f.value.clone();
    }
    if let Some(f) = form.fields.get(2) {
        proj.authors = f.value.clone();
    }
    if let Some(f) = form.fields.get(3) {
        proj.credits = f.value.clone();
    }
    if let Some(f) = form.fields.get(4) {
        proj.description = f.value.clone();
    }
    if let Some(f) = form.fields.get(5) {
        proj.minecraft = f.value.clone();
    }
    if let Some(f) = form.fields.get(6) {
        proj.loader = f.value.clone();
    }
    if let Some(f) = form.fields.get(7) {
        proj.platforms = f
            .value
            .split(", ")
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();
    }
    let website = form.fields.get(8).and_then(|f| {
        if f.value.is_empty() {
            None
        } else {
            Some(f.value.clone())
        }
    });
    let discord = form.fields.get(9).and_then(|f| {
        if f.value.is_empty() {
            None
        } else {
            Some(f.value.clone())
        }
    });
    let github = form.fields.get(10).and_then(|f| {
        if f.value.is_empty() {
            None
        } else {
            Some(f.value.clone())
        }
    });
    proj.links.website = website;
    proj.links.discord = discord;
    proj.links.github = github;
}

fn form_to_project(form: &ui::FormState) -> project::ModpackProject {
    let name = form
        .fields
        .get(0)
        .map(|f| f.value.clone())
        .unwrap_or_default();
    let version = form
        .fields
        .get(1)
        .map(|f| f.value.clone())
        .unwrap_or_else(|| "1.0.0".into());
    let authors = form
        .fields
        .get(2)
        .map(|f| f.value.clone())
        .unwrap_or_default();
    let credits = form
        .fields
        .get(3)
        .map(|f| f.value.clone())
        .unwrap_or_default();
    let description = form
        .fields
        .get(4)
        .map(|f| f.value.clone())
        .unwrap_or_default();
    let minecraft = form
        .fields
        .get(5)
        .map(|f| f.value.clone())
        .unwrap_or_default();
    let loader = form
        .fields
        .get(6)
        .map(|f| f.value.clone())
        .unwrap_or_default();
    let platforms_str = form
        .fields
        .get(7)
        .map(|f| f.value.clone())
        .unwrap_or_default();
    let platforms: Vec<String> = platforms_str
        .split(", ")
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();
    let website = form.fields.get(8).and_then(|f| {
        if f.value.is_empty() {
            None
        } else {
            Some(f.value.clone())
        }
    });
    let discord = form.fields.get(9).and_then(|f| {
        if f.value.is_empty() {
            None
        } else {
            Some(f.value.clone())
        }
    });
    let github = form.fields.get(10).and_then(|f| {
        if f.value.is_empty() {
            None
        } else {
            Some(f.value.clone())
        }
    });
    project::ModpackProject {
        name,
        version,
        authors,
        credits,
        description,
        links: project::ProjectLinks {
            website,
            discord,
            github,
        },
        minecraft,
        loader,
        platforms,
    }
}
