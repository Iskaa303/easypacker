use crate::api::filters;
use crate::types::{BrowseState, FilterKind};
use crossterm::event::KeyCode;
use ratatui::Frame;
use ratatui::layout::{Alignment, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph, Wrap};
use std::collections::HashSet;

// ── Welcome Screen ────────────────────────────────────────

#[derive(Default)]
pub struct WelcomeState {
    pub path_input: String,
    pub error: Option<String>,
    pub show_path_input: bool,
}

#[derive(Debug, PartialEq)]
pub enum WelcomeAction {
    Create,
    Open(std::path::PathBuf),
    Quit,
    None,
}

pub fn render_welcome(frame: &mut Frame, area: Rect, state: &WelcomeState, selected: usize) {
    let lines = if state.show_path_input {
        let err = state.error.as_deref().unwrap_or("");
        vec![
            Line::from(Span::styled(
                " Enter path to modpack.json or project folder:",
                Style::default().fg(Color::Gray),
            )),
            Line::from(""),
            Line::from(vec![
                Span::styled(" Path: ", Style::default().fg(Color::Gray)),
                Span::styled(&state.path_input, Style::default().fg(Color::White)),
                Span::styled("█", Style::default().fg(Color::Yellow)),
            ]),
            Line::from(""),
            Line::from(Span::styled(err, Style::default().fg(Color::Red))),
            Line::from(""),
            Line::from(Span::styled(
                " Enter: confirm   Esc: back",
                Style::default().fg(Color::DarkGray),
            )),
        ]
    } else {
        vec![
            Line::from(Span::styled(
                " No modpack project found in this directory.",
                Style::default().fg(Color::Gray),
            )),
            Line::from(""),
            Line::from(Span::styled(
                format!(
                    " {}  Create new project{}",
                    if selected == 0 { "▸" } else { " " },
                    if selected == 0 { " ◄" } else { "" },
                ),
                if selected == 0 {
                    Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(Color::White)
                },
            )),
            Line::from(Span::styled(
                format!(
                    " {}  Open existing project{}",
                    if selected == 1 { "▸" } else { " " },
                    if selected == 1 { " ◄" } else { "" },
                ),
                if selected == 1 {
                    Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(Color::White)
                },
            )),
            Line::from(""),
            Line::from(Span::styled(
                " Use ↑↓ to navigate, Enter to select, q to quit",
                Style::default().fg(Color::DarkGray),
            )),
        ]
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .title(" easypacker ")
        .border_style(Style::default().fg(Color::Cyan));
    let paragraph = Paragraph::new(lines)
        .block(block)
        .wrap(Wrap { trim: false })
        .alignment(if state.show_path_input {
            Alignment::Left
        } else {
            Alignment::Center
        });
    frame.render_widget(paragraph, area);
}

pub fn handle_welcome_key(
    state: &mut WelcomeState,
    key: KeyCode,
    selected: &mut usize,
) -> WelcomeAction {
    if state.show_path_input {
        match key {
            KeyCode::Esc => {
                state.show_path_input = false;
                state.path_input.clear();
                state.error = None;
                WelcomeAction::None
            }
            KeyCode::Enter => {
                let p = std::path::PathBuf::from(state.path_input.trim());
                if p.exists() {
                    let dir = if p.is_dir() {
                        p
                    } else {
                        p.parent().unwrap_or(&p).to_path_buf()
                    };
                    WelcomeAction::Open(dir)
                } else {
                    state.error = Some("Path does not exist".into());
                    WelcomeAction::None
                }
            }
            KeyCode::Char(c) => {
                state.path_input.push(c);
                WelcomeAction::None
            }
            KeyCode::Backspace | KeyCode::Delete => {
                state.path_input.pop();
                WelcomeAction::None
            }
            _ => WelcomeAction::None,
        }
    } else {
        match key {
            KeyCode::Up => {
                *selected = selected.saturating_sub(1);
                WelcomeAction::None
            }
            KeyCode::Down => {
                *selected = selected.saturating_add(1).min(1);
                WelcomeAction::None
            }
            KeyCode::Char('\n') | KeyCode::Enter => match *selected {
                0 => WelcomeAction::Create,
                1 => {
                    state.show_path_input = true;
                    state.path_input.clear();
                    state.error = None;
                    WelcomeAction::None
                }
                2 => WelcomeAction::Quit,
                _ => WelcomeAction::None,
            },
            KeyCode::Char('q') => WelcomeAction::Quit,
            _ => WelcomeAction::None,
        }
    }
}

// ── Main Menu ──────────────────────────────────────────────

pub fn render_main_menu(frame: &mut Frame, area: Rect, project_name: &str, selected: usize) {
    let lines = vec![
        Line::from(Span::styled(
            format!(" Project: {project_name}"),
            Style::default()
                .fg(Color::Green)
                .add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
        Line::from(Span::styled(
            format!(
                " {}  Search projects{}",
                if selected == 0 { "▸" } else { " " },
                if selected == 0 { " ◄" } else { "" },
            ),
            if selected == 0 {
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::White)
            },
        )),
        Line::from(Span::styled(
            format!(
                " {}  Project settings{}",
                if selected == 1 { "▸" } else { " " },
                if selected == 1 { " ◄" } else { "" },
            ),
            if selected == 1 {
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::White)
            },
        )),
        Line::from(""),
        Line::from(vec![
            Span::styled(
                "Use ↑↓ to navigate, Enter to select, ",
                Style::default().fg(Color::DarkGray),
            ),
            Span::styled(
                "q to quit",
                Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
            ),
        ]),
    ];

    let block = Block::default()
        .borders(Borders::ALL)
        .title(" easypacker — Main Menu ")
        .border_style(Style::default().fg(Color::Cyan));
    let paragraph = Paragraph::new(lines)
        .block(block)
        .alignment(Alignment::Center)
        .wrap(Wrap { trim: false });
    frame.render_widget(paragraph, area);
}

pub fn handle_main_menu_key(key: KeyCode, selected: &mut usize) -> Option<usize> {
    match key {
        KeyCode::Up => {
            *selected = selected.saturating_sub(1);
            None
        }
        KeyCode::Down => {
            *selected = selected.saturating_add(1).min(1);
            None
        }
        KeyCode::Char('\n') | KeyCode::Enter => Some(*selected),
        KeyCode::Char('q') => Some(2), // quit
        _ => None,
    }
}

// ── Form system (shared by Settings + Create Project) ─────

#[derive(Clone)]
pub enum FieldKind {
    Text,
    Pick {
        options: Vec<String>,
        kind: FilterKind,
    },
}

pub struct FormField {
    pub label: &'static str,
    pub value: String,
    pub kind: FieldKind,
}

pub struct FormState {
    pub fields: Vec<FormField>,
    pub selected: usize,
    pub focused: bool,
    pub cursor_pos: usize,
}

pub enum FormAction {
    PickField(usize),
    Save,
    Cancel,
    None,
}

// ── Form constructors ──────────────────────────────────────

pub fn new_create_form() -> FormState {
    FormState {
        fields: vec![
            FormField {
                label: "Name",
                value: String::new(),
                kind: FieldKind::Text,
            },
            FormField {
                label: "Version",
                value: "0.0.1".into(),
                kind: FieldKind::Text,
            },
            FormField {
                label: "Authors",
                value: String::new(),
                kind: FieldKind::Text,
            },
            FormField {
                label: "Credits",
                value: String::new(),
                kind: FieldKind::Text,
            },
            FormField {
                label: "Description",
                value: String::new(),
                kind: FieldKind::Text,
            },
            FormField {
                label: "Minecraft",
                value: String::new(),
                kind: FieldKind::Pick {
                    options: filters::VERSIONS.iter().map(|s| s.to_string()).collect(),
                    kind: FilterKind::Version,
                },
            },
            FormField {
                label: "Loader",
                value: String::new(),
                kind: FieldKind::Pick {
                    options: filters::LOADERS.iter().map(|s| s.to_string()).collect(),
                    kind: FilterKind::Loader,
                },
            },
            FormField {
                label: "Platforms",
                value: "modrinth".into(),
                kind: FieldKind::Pick {
                    options: vec!["modrinth".into(), "curseforge".into()],
                    kind: FilterKind::Platform,
                },
            },
            FormField {
                label: "Website",
                value: String::new(),
                kind: FieldKind::Text,
            },
            FormField {
                label: "Discord",
                value: String::new(),
                kind: FieldKind::Text,
            },
            FormField {
                label: "GitHub",
                value: String::new(),
                kind: FieldKind::Text,
            },
        ],
        selected: 0,
        focused: false,
        cursor_pos: 0,
    }
}

pub fn new_settings_form(
    name: &str,
    version: &str,
    authors: &str,
    credits: &str,
    description: &str,
    minecraft: &str,
    loader: &str,
    platforms: &[String],
    website: &Option<String>,
    discord: &Option<String>,
    github: &Option<String>,
) -> FormState {
    FormState {
        fields: vec![
            FormField {
                label: "Name",
                value: name.to_owned(),
                kind: FieldKind::Text,
            },
            FormField {
                label: "Version",
                value: version.to_owned(),
                kind: FieldKind::Text,
            },
            FormField {
                label: "Authors",
                value: authors.to_owned(),
                kind: FieldKind::Text,
            },
            FormField {
                label: "Credits",
                value: credits.to_owned(),
                kind: FieldKind::Text,
            },
            FormField {
                label: "Description",
                value: description.to_owned(),
                kind: FieldKind::Text,
            },
            FormField {
                label: "Minecraft",
                value: minecraft.to_owned(),
                kind: FieldKind::Pick {
                    options: filters::VERSIONS.iter().map(|s| s.to_string()).collect(),
                    kind: FilterKind::Version,
                },
            },
            FormField {
                label: "Loader",
                value: loader.to_owned(),
                kind: FieldKind::Pick {
                    options: filters::LOADERS.iter().map(|s| s.to_string()).collect(),
                    kind: FilterKind::Loader,
                },
            },
            FormField {
                label: "Platforms",
                value: platforms.join(", "),
                kind: FieldKind::Pick {
                    options: vec!["modrinth".into(), "curseforge".into()],
                    kind: FilterKind::Platform,
                },
            },
            FormField {
                label: "Website",
                value: website.clone().unwrap_or_default(),
                kind: FieldKind::Text,
            },
            FormField {
                label: "Discord",
                value: discord.clone().unwrap_or_default(),
                kind: FieldKind::Text,
            },
            FormField {
                label: "GitHub",
                value: github.clone().unwrap_or_default(),
                kind: FieldKind::Text,
            },
        ],
        selected: 0,
        focused: false,
        cursor_pos: 0,
    }
}

// ── Form rendering ─────────────────────────────────────────

pub fn render_form(frame: &mut Frame, area: Rect, title: &str, form: &FormState) {
    let mut lines: Vec<Line> = Vec::new();

    for (i, field) in form.fields.iter().enumerate() {
        let is_sel = i == form.selected;
        let prefix = if is_sel { "▸ " } else { "  " };
        let label_style = if is_sel {
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::Gray)
        };
        let val_style = if is_sel {
            Style::default().fg(Color::White)
        } else if field.value.is_empty() {
            Style::default().fg(Color::DarkGray)
        } else {
            Style::default().fg(Color::White)
        };

        let display = if field.value.is_empty() {
            match field.kind {
                FieldKind::Text => "".into(),
                FieldKind::Pick { .. } => "— pick —".into(),
            }
        } else {
            field.value.clone()
        };

        let suffix = match field.kind {
            FieldKind::Pick { .. } if is_sel => "  [Enter: pick]".to_owned(),
            _ => String::new(),
        };

        match field.kind {
            _ => {
                lines.push(Line::from(vec![
                    Span::styled(prefix, Style::default().fg(Color::Yellow)),
                    Span::styled(format!("{}: ", field.label), label_style),
                    Span::styled(display, val_style),
                    Span::styled(suffix, Style::default().fg(Color::DarkGray)),
                ]));
            }
        }
    }

    lines.push(Line::from(""));
    if form.focused {
        lines.push(Line::from(vec![Span::styled(
            "Esc:stop editing  ↑↓:navigate  ←→:cursor",
            Style::default().fg(Color::DarkGray),
        )]));
    } else {
        lines.push(Line::from(vec![
            Span::styled("s:save ", Style::default().fg(Color::Green)),
            Span::styled("q:cancel ", Style::default().fg(Color::Red)),
            Span::styled(
                "↑↓:navigate  Enter:edit field",
                Style::default().fg(Color::DarkGray),
            ),
        ]));
    }

    let block = Block::default()
        .borders(Borders::ALL)
        .title(format!(
            " {} {} ",
            if form.focused { "▸" } else { "◇" },
            title
        ))
        .border_style(Style::default().fg(Color::Cyan));
    let paragraph = Paragraph::new(lines)
        .block(block)
        .wrap(Wrap { trim: false });
    frame.render_widget(paragraph, area);
    if form.focused {
        if let Some(field) = form.fields.get(form.selected) {
            if matches!(field.kind, FieldKind::Text) {
                let cx = area.x + 1 + 2 + field.label.len() as u16 + 2 + form.cursor_pos as u16;
                let cy = area.y + 1 + form.selected as u16;
                frame.set_cursor_position((cx, cy));
            }
        }
    }
}

// ── Form key handling ──────────────────────────────────────

pub fn handle_form_key(form: &mut FormState, key: KeyCode) -> FormAction {
    let max = form.fields.len().saturating_sub(1);
    if form.focused {
        match key {
            KeyCode::Esc | KeyCode::Down | KeyCode::Up => {
                form.focused = false;
                match key {
                    KeyCode::Down => form.selected = form.selected.saturating_add(1).min(max),
                    KeyCode::Up => form.selected = form.selected.saturating_sub(1),
                    _ => {}
                }
                FormAction::None
            }
            KeyCode::Char('\n') | KeyCode::Enter => match form.fields[form.selected].kind {
                FieldKind::Pick { .. } => FormAction::PickField(form.selected),
                _ => FormAction::None,
            },
            KeyCode::Left if form.cursor_pos > 0 => {
                form.cursor_pos -= 1;
                FormAction::None
            }
            KeyCode::Right => {
                let val = &form.fields[form.selected].value;
                if form.cursor_pos < val.len() {
                    form.cursor_pos += 1;
                }
                FormAction::None
            }
            KeyCode::Char(c) => {
                if matches!(form.fields[form.selected].kind, FieldKind::Text) {
                    form.fields[form.selected].value.insert(form.cursor_pos, c);
                    form.cursor_pos += 1;
                }
                FormAction::None
            }
            KeyCode::Backspace => {
                if matches!(form.fields[form.selected].kind, FieldKind::Text) && form.cursor_pos > 0
                {
                    form.cursor_pos -= 1;
                    form.fields[form.selected].value.remove(form.cursor_pos);
                }
                FormAction::None
            }
            KeyCode::Delete => {
                if matches!(form.fields[form.selected].kind, FieldKind::Text) {
                    let len = form.fields[form.selected].value.len();
                    if form.cursor_pos < len {
                        form.fields[form.selected].value.remove(form.cursor_pos);
                    }
                }
                FormAction::None
            }
            _ => FormAction::None,
        }
    } else {
        match key {
            KeyCode::Up | KeyCode::BackTab => {
                form.selected = form.selected.saturating_sub(1);
                form.cursor_pos = 0;
                FormAction::None
            }
            KeyCode::Down | KeyCode::Tab => {
                form.selected = form.selected.saturating_add(1).min(max);
                form.cursor_pos = 0;
                FormAction::None
            }
            KeyCode::Char('\n') | KeyCode::Enter => {
                match form.fields[form.selected].kind {
                    FieldKind::Pick { .. } => return FormAction::PickField(form.selected),
                    _ => {}
                }
                form.focused = true;
                form.cursor_pos = form.fields[form.selected].value.len();
                FormAction::None
            }
            KeyCode::Char('s') => FormAction::Save,
            KeyCode::Char('q') => FormAction::Cancel,
            KeyCode::Esc => FormAction::None,
            _ => FormAction::None,
        }
    }
}

// ── Helpers to open browse from a form Pick field ──────────

pub fn open_browse_for_field(field: &FormField) -> Option<(BrowseState, HashSet<String>)> {
    match &field.kind {
        FieldKind::Pick { options, kind } => {
            let saved: HashSet<String> = field
                .value
                .split(", ")
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect();
            let toggled: HashSet<usize> = saved
                .iter()
                .filter_map(|s| options.iter().position(|o| o == s))
                .collect();
            let browse = BrowseState {
                kind: kind.clone(),
                options: options.clone(),
                filtered: (0..options.len()).collect(),
                filter_text: String::new(),
                selected: 0,
                toggled,
                scroll: 0,
            };
            Some((browse, saved))
        }
        FieldKind::Text => None,
    }
}
