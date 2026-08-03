use super::types::{CrossPlatform, Platform, SearchFilters, SearchResult};
use color_eyre::eyre::Result;
use serde::Deserialize;
use std::time::Duration;

const BASE: &str = "https://api.curseforge.com/v1";
const MINECRAFT_GAME_ID: i32 = 432;

pub struct CurseForgeClient {
    client: reqwest::Client,
}

#[derive(Deserialize)]
struct SearchResponse {
    data: Vec<ModData>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ModData {
    id: i32,
    name: String,
    slug: String,
    summary: String,
    #[serde(default, rename = "downloadCount")]
    downloads: u64,
    #[serde(default)]
    authors: Vec<Author>,
    #[serde(default, rename = "gameVersions")]
    game_versions: Option<Vec<String>>,
    #[serde(default)]
    categories: Vec<Category>,
    logo: Option<Logo>,
    #[serde(default)]
    links: Option<Links>,
    #[serde(default, rename = "classId")]
    class_id: Option<i32>,
    #[serde(default, rename = "modLoaders")]
    mod_loaders: Option<Vec<ModLoader>>,
    #[serde(default, rename = "latestFilesIndexes")]
    latest_files_indexes: Option<Vec<LatestFileIndex>>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CurseForgeFile {
    id: i32,
    display_name: String,
    #[serde(default)]
    file_name: String,
    #[serde(default)]
    file_length: u64,
    game_versions: Vec<String>,
    file_date: String,
    download_count: u64,
    #[serde(default)]
    download_url: Option<String>,
    #[serde(default)]
    hashes: Vec<FileHash>,
}

#[derive(Deserialize)]
struct FileHash {
    algo: i32,
    value: String,
}

#[derive(Deserialize)]
struct FilesResponse {
    data: Vec<CurseForgeFile>,
}

#[derive(Debug, Clone)]
pub struct CurseForgeFileInfo {
    pub id: i32,
    pub display_name: String,
    pub file_name: String,
    pub file_length: u64,
    pub game_versions: Vec<String>,
    pub file_date: String,
    pub download_count: u64,
    pub download_url: Option<String>,
    pub sha1: Option<String>,
    pub md5: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Author {
    name: String,
}

#[derive(Deserialize)]
struct Category {
    name: String,
}

#[derive(Deserialize)]
struct ModLoader {
    name: Option<String>,
}

#[derive(Deserialize)]
struct LatestFileIndex {
    #[serde(rename = "gameVersion")]
    game_version: Option<String>,
    #[serde(default, rename = "modLoader")]
    mod_loader: Option<i32>,
}

#[derive(Deserialize)]
struct Logo {
    url: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Links {
    website_url: Option<String>,
}

impl CurseForgeClient {
    pub fn new(api_key: &str) -> Self {
        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert(
            reqwest::header::HeaderName::from_static("x-api-key"),
            reqwest::header::HeaderValue::from_str(api_key).unwrap(),
        );
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(15))
            .default_headers(headers)
            .build()
            .unwrap();
        Self { client }
    }

    pub async fn search(&self, filters: &SearchFilters) -> Result<Vec<SearchResult>> {
        let mut params: Vec<(&str, String)> = vec![
            ("gameId", MINECRAFT_GAME_ID.to_string()),
            ("pageSize", filters.limit.to_string()),
            ("index", filters.offset.to_string()),
            ("sortField", "6".to_owned()),
            ("sortOrder", "desc".to_owned()),
        ];
        if !filters.query.is_empty() {
            params.push(("searchFilter", filters.query.clone()));
        }
        if let Some(ref v) = filters.version {
            params.push(("gameVersion", v.clone()));
        }
        if let Some(ref l) = filters.loader {
            if let Some(loader_id) = loader_id(l) {
                params.push(("modLoaderType", loader_id.to_string()));
            }
        }
        if let Some(ref pt) = filters.project_type {
            if let Some(cid) = project_type_to_class_id(pt) {
                params.push(("classId", cid.to_string()));
            }
        }
        let response = self
            .client
            .get(format!("{}/mods/search", BASE))
            .query(&params)
            .send()
            .await?;
        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(color_eyre::eyre::eyre!(
                "CurseForge API error {status}: {body}"
            ));
        }
        let body = response.text().await.unwrap_or_default();
        let parsed: SearchResponse = match serde_json::from_str(&body) {
            Ok(r) => r,
            Err(e) => {
                let preview = if body.len() > 200 {
                    format!("{}…", &body[..200])
                } else {
                    body.clone()
                };
                return Err(color_eyre::eyre::eyre!(
                    "CurseForge decode error: {e}\nResponse preview: {preview}"
                ));
            }
        };
        let mut results: Vec<SearchResult> = Vec::new();
        for m in parsed.data {
            let author = m
                .authors
                .first()
                .map(|a| a.name.clone())
                .unwrap_or_default();
            let categories: Vec<String> = m.categories.iter().map(|c| c.name.clone()).collect();
            let latest_file = m.latest_files_indexes.iter().flatten().next();
            let versions: Vec<String> = m.game_versions.clone().unwrap_or_default();
            let mut loaders: Vec<String> = m
                .mod_loaders
                .iter()
                .flatten()
                .filter_map(|ml| {
                    ml.name
                        .as_ref()
                        .map(|n| n.split('-').next().unwrap_or(n).to_lowercase())
                })
                .collect();
            if loaders.is_empty() {
                if let Some(id) = latest_file.and_then(|f| f.mod_loader) {
                    if let Some(name) = mod_loader_id_to_name(id) {
                        loaders.push(name.into());
                    }
                }
            }
            results.push(SearchResult {
                title: m.name,
                author,
                description: m.summary,
                downloads: m.downloads,
                follows: 0,
                platform: Platform::CurseForge,
                url: m.links.and_then(|l| l.website_url).or(Some(format!(
                    "https://www.curseforge.com/minecraft/mc-mods/{}",
                    m.slug
                ))),
                icon_url: m.logo.and_then(|l| l.url),
                project_type: project_type_from_class_id(m.class_id, &categories),
                license: None,
                latest_version: latest_file
                    .and_then(|f| f.game_version.clone())
                    .or_else(|| versions.first().cloned()),
                loaders,
                cross: CrossPlatform {
                    curseforge_id: Some(m.id),
                    ..CrossPlatform::default()
                },
            });
        }
        results.sort_by(|a, b| b.downloads.cmp(&a.downloads));
        Ok(results)
    }

    /// Find a project by its slug. Returns (mod id, display name).
    pub async fn find_by_slug(
        &self,
        slug: &str,
        class_id: Option<i32>,
    ) -> Result<Option<(i32, String)>> {
        let mut params: Vec<(&str, String)> = vec![
            ("gameId", MINECRAFT_GAME_ID.to_string()),
            ("slug", slug.to_owned()),
            ("pageSize", "1".to_owned()),
        ];
        if let Some(cid) = class_id {
            params.push(("classId", cid.to_string()));
        }
        let resp: SearchResponse = self
            .client
            .get(format!("{}/mods/search", BASE))
            .query(&params)
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;
        Ok(resp.data.into_iter().next().map(|m| (m.id, m.name)))
    }

    pub async fn get_files(
        &self,
        mod_id: i32,
        game_version: Option<&str>,
        loader: Option<&str>,
    ) -> Result<Vec<CurseForgeFileInfo>> {
        let mut params: Vec<(&str, String)> = Vec::new();
        if let Some(gv) = game_version {
            params.push(("gameVersion", gv.to_owned()));
        }
        if let Some(l) = loader {
            if let Some(id) = loader_id(l) {
                params.push(("modLoaderType", id.to_string()));
            }
        }
        let resp: FilesResponse = self
            .client
            .get(format!("{}/mods/{}/files", BASE, mod_id))
            .query(&params)
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;
        Ok(resp
            .data
            .into_iter()
            .map(|f| {
                let hash = |algo: i32| {
                    f.hashes
                        .iter()
                        .find(|h| h.algo == algo)
                        .map(|h| h.value.clone())
                };
                CurseForgeFileInfo {
                    id: f.id,
                    display_name: f.display_name,
                    file_name: f.file_name,
                    file_length: f.file_length,
                    game_versions: f.game_versions,
                    file_date: f.file_date,
                    download_count: f.download_count,
                    download_url: f.download_url,
                    sha1: hash(1),
                    md5: hash(2),
                }
            })
            .collect())
    }
}

fn project_type_from_class_id(class_id: Option<i32>, categories: &[String]) -> String {
    match class_id {
        Some(6) => "mod".into(),
        Some(12) => "resourcepack".into(),
        Some(6945) => "datapack".into(),
        Some(6552) => "shader".into(),
        _ => categories
            .first()
            .cloned()
            .unwrap_or_else(|| "unknown".into()),
    }
}

pub(crate) fn project_type_to_class_id(pt: &str) -> Option<i32> {
    match pt {
        "mod" => Some(6),
        "resourcepack" => Some(12),
        "datapack" => Some(6945),
        "shader" => Some(6552),
        _ => None,
    }
}

fn loader_id(l: &str) -> Option<i32> {
    match l.to_lowercase().as_str() {
        "forge" => Some(1),
        "fabric" => Some(4),
        "quilt" => Some(5),
        "neoforge" | "neo" => Some(6),
        _ => None,
    }
}

fn mod_loader_id_to_name(id: i32) -> Option<&'static str> {
    match id {
        1 => Some("forge"),
        4 => Some("fabric"),
        5 => Some("quilt"),
        6 => Some("neoforge"),
        _ => None,
    }
}
