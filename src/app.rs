use crate::api::types::SearchResult;
use crate::project;
use crate::types::*;
use crate::ui;
use ratatui_image::picker::Picker;
use ratatui_image::protocol::Protocol;
use std::cell::Cell;

pub(crate) struct App {
    // Search state
    pub query: String,
    pub cursor_pos: usize,
    pub filters: FiltersState,
    pub results: Vec<SearchResult>,
    pub focus: Focus,
    pub status: Status,
    pub scroll: usize,
    pub selected: usize,
    pub browse_mode: Option<BrowseState>,
    pub filter_selected: usize,
    pub api_key_input: String,
    pub search_offset: usize,
    pub picker: Picker,
    pub proto_cache: Vec<Option<Protocol>>,
    pub visible_count: Cell<usize>,
    // Mode + project
    pub mode: AppMode,
    pub project: Option<project::ModpackProject>,
    pub welcome_state: ui::WelcomeState,
    pub menu_selection: usize,
    pub form: Option<ui::FormState>,
    pub form_field_idx: Option<usize>,
    pub file_browse: Option<FileBrowseState>,
    pub quit_requested: bool,
}
