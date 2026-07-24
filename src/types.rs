use crate::api::types::SearchResult;
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

pub(crate) enum BrowseAction {
    Toggle(FilterKind),
    Close,
    None,
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
    Error(String),
    BrowseOptions {
        kind: FilterKind,
        options: Vec<String>,
        saved: HashSet<String>,
    },
    IconLoaded(usize, Vec<u8>),
}

#[derive(Clone, PartialEq)]
pub(crate) enum AppMode {
    Welcome,
    MainMenu,
    Search,
    Settings,
    CreateProject,
}
