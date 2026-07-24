#[derive(Debug, Clone, PartialEq)]
pub enum Platform {
    Modrinth,
    CurseForge,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct CrossPlatform {
    pub modrinth_downloads: u64,
    pub modrinth_url: Option<String>,
    pub curseforge_downloads: u64,
    pub curseforge_url: Option<String>,
}

#[derive(Debug, Clone)]
pub struct SearchFilters {
    pub query: String,
    pub version: Option<String>,
    pub loader: Option<String>,
    pub project_type: Option<String>,
    pub sort: String,
    pub limit: usize,
    pub offset: usize,
}

pub struct SearchResult {
    pub title: String,
    pub author: String,
    pub description: String,
    pub downloads: u64,
    pub follows: u64,
    pub platform: Platform,
    pub url: Option<String>,
    pub icon_url: Option<String>,
    pub project_type: String,
    pub license: Option<String>,
    pub latest_version: Option<String>,
    pub loaders: Vec<String>,
    pub cross: CrossPlatform,
}
