#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Platform {
    Modrinth,
    CurseForge,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DepKind {
    Required,
    Optional,
    Incompatible,
}

/// A dependency declared by a version/file. `project_id` is the Modrinth
/// project id OR the `CurseForge` modId (as a string); `version_id` is the
/// specific version/file id the parent requires, when it pins one.
#[derive(Debug, Clone)]
pub struct Dependency {
    pub project_id: String,
    #[allow(dead_code)]
    pub version_id: Option<String>,
    pub kind: DepKind,
    pub platform: Platform,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct CrossPlatform {
    pub curseforge_id: Option<i32>,
    pub modrinth_slug: Option<String>,
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

#[derive(Debug, Clone)]
pub struct ProjectFile {
    pub name: String,
    pub mc_versions: Vec<String>,
    pub loaders: Vec<String>,
    pub date_published: String,
    pub downloads: u64,
    pub size: u64,
    pub url: Option<String>,
    pub platforms: Vec<Platform>,
    pub modrinth_version_id: Option<String>,
    pub modrinth_url: Option<String>,
    pub curseforge_file_id: Option<i32>,
    pub curseforge_url: Option<String>,
    /// `CurseForge` project id (modId) — needed to write a valid manifest
    /// entry when the dep was declared on modrinth.
    pub curseforge_project_id: Option<i64>,
    /// sha1 of the file, present on both platforms — used to treat two files
    /// with different names (modrinth vs curseforge) as the same version.
    pub sha1: Option<String>,
    /// Per-platform version names, kept when merging across platforms so the
    /// manifest can pin each platform's own name.
    pub modrinth_name: Option<String>,
    pub curseforge_name: Option<String>,
    pub dependencies: Vec<Dependency>,
}

impl ProjectFile {
    /// True if this file is the same release as `other` on a different
    /// platform. Compared by sha1 when both sides have it (names may differ
    /// between modrinth and curseforge); falls back to size+name.
    pub fn same_file_as(&self, other: &Self) -> bool {
        match (&self.sha1, &other.sha1) {
            (Some(a), Some(b)) => a.eq_ignore_ascii_case(b),
            _ => self.size == other.size && self.name.to_lowercase() == other.name.to_lowercase(),
        }
    }
}
