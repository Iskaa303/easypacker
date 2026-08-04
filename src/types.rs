use crate::api::types::{Platform, ProjectFile, SearchResult};
use std::collections::HashSet;

#[derive(PartialEq)]
pub(crate) enum Focus {
    Neutral,
    Query,
    Filters,
    Results,
}

#[derive(Clone, PartialEq)]
pub(crate) enum FilterKind {
    Version,
    Loader,
    Type,
    Platform,
}

impl FilterKind {
    pub(crate) fn label(&self) -> &str {
        match self {
            FilterKind::Version => "Version",
            FilterKind::Loader => "Loader",
            FilterKind::Type => "Type",
            FilterKind::Platform => "Platform",
        }
    }
}

pub(crate) struct BrowseState {
    pub kind: FilterKind,
    pub options: Vec<String>,
    pub filtered: Vec<usize>,
    pub filter_text: String,
    pub selected: usize,
    pub toggled: HashSet<usize>,
    pub scroll: usize,
}
pub(crate) struct FileBrowseState {
    pub project_title: String,
    pub modrinth_slug: Option<String>,
    pub curseforge_id: Option<i32>,
    pub project_type: String,
    pub files: Vec<ProjectFile>,
    pub scroll: usize,
    pub selected: usize,
    pub already_added: bool,
    pub added_index: Option<usize>,
}
pub(crate) enum BrowseAction {
    Toggle(FilterKind),
    Close,
    None,
}

/// Overlay popup for linking the same version on the other platform.
/// `picked.is_none()` => Query stage (search results).
/// `picked.is_some()` => Versions stage (the picked project's files).
pub(crate) struct LinkVersionState {
    pub platform: Platform,
    pub query: String,
    pub cursor: usize,
    pub selected: usize,
    pub scroll: usize,
    pub results: Vec<SearchResult>,
    pub files: Vec<ProjectFile>,
    pub picked: Option<PickedProject>,
    pub status: Option<String>,
    pub searched_query: Option<String>,
}

/// Project chosen in the link popup, to fetch versions for.
#[derive(Clone)]
pub(crate) struct PickedProject {
    pub modrinth_slug: Option<String>,
    pub curseforge_id: Option<i32>,
}

/// One discovered dependency for the dependency popup.
#[derive(Clone)]
pub(crate) struct DepRow {
    /// easypacker id = manifest key = modrinth slug (or cf slug/name).
    pub id: String,
    /// Raw platform project id (modrinth project id / cf modId) for API calls.
    pub project_id: String,
    /// Platform the parent declared this dep on.
    pub platform: Platform,
    pub title: String,
    /// true if the parent declared it optional. Optional deps start disabled;
    /// the player toggles them on. Required deps start enabled.
    pub optional: bool,
    pub enabled: bool,
    /// Player's chosen version display name (default = first/latest).
    pub version: Option<String>,
    /// Available versions, rendered like a normal file-browse list.
    pub versions: Vec<ProjectFile>,
}

/// Dependency popup: lists required + optional deps for the browsed mod,
/// lets the player enable optional deps and pin specific versions.
pub(crate) struct DependencyState {
    pub rows: Vec<DepRow>,
    pub selected: usize,
    pub scroll: usize,
    pub status: Option<String>,
    /// When Some, a sub-popup lists versions for `rows[selected]`.
    pub version_picker: Option<DepVersionPicker>,
}

#[derive(Clone)]
pub(crate) struct DepVersionPicker {
    pub row: usize,
    pub selected: usize,
    pub scroll: usize,
}

pub(crate) struct FiltersState {
    pub version: String,
    pub loader: String,
    pub project_type: String,
    pub platform: String,
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

#[derive(PartialEq)]
pub(crate) enum Status {
    Idle,
    Searching,
    Error(String),
    ApiKeyPrompt,
    Done,
}

pub(crate) enum AppEvent {
    Results {
        results: Vec<SearchResult>,
        offset: usize,
    },
    FileResults {
        files: Vec<ProjectFile>,
        project_title: String,
        modrinth_slug: Option<String>,
        curseforge_id: Option<i32>,
        project_type: String,
    },
    Error(String),
    BrowseOptions {
        kind: FilterKind,
        options: Vec<String>,
        saved: HashSet<String>,
    },
    IconLoaded(usize, Vec<u8>),
    LinkResults {
        results: Vec<SearchResult>,
    },
    LinkFiles {
        files: Vec<ProjectFile>,
    },
    DepsLoaded {
        rows: Vec<DepRow>,
    },
}
#[derive(Clone, PartialEq)]
pub(crate) enum AppMode {
    Welcome,
    MainMenu,
    Search,
    FileBrowse,
    Settings,
    CreateProject,
}
