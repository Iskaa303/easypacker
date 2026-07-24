use super::{Platform, SearchFilters, SearchResult};
use color_eyre::eyre::Result;
use serde::Deserialize;

const BASE: &str = "https://api.curseforge.com/v1";
const MINECRAFT_GAME_ID: i32 = 432;

pub struct CurseForgeClient {
    #[allow(dead_code)]
    api_key: String,
    client: reqwest::Client,
}

#[derive(Deserialize)]
struct SearchResponse {
    data: Vec<ModData>,
    #[allow(dead_code)]
    pagination: Pagination,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ModData {
    #[allow(dead_code)]
    id: u64,
    name: String,
    slug: String,
    summary: String,
    #[serde(default, rename = "downloadCount")]
    downloads: u64,
    #[serde(default)]
    authors: Vec<Author>,
    #[serde(default, rename = "gameVersions")]
    game_versions: Vec<String>,
    #[serde(default)]
    categories: Vec<Category>,
    logo: Option<Logo>,
    links: Links,
    #[serde(default, rename = "modLoaders")]
    mod_loaders: Vec<ModLoader>,
}


#[derive(Deserialize)]
struct Author {
    name: String,
}


#[derive(Deserialize)]
struct Category {
    name: String,
}

#[derive(Deserialize)]
struct ModLoader {
    name: String,
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

#[derive(Deserialize)]
struct Pagination {
    #[allow(dead_code)]
    total_count: u64,
}

impl CurseForgeClient {
    pub fn new(api_key: &str) -> Self {
        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert(
            reqwest::header::HeaderName::from_static("x-api-key"),
            reqwest::header::HeaderValue::from_str(api_key).unwrap(),
        );
        let client = reqwest::Client::builder()
            .default_headers(headers)
            .build()
            .unwrap();
        Self {
            api_key: api_key.to_owned(),
            client,
        }
    }

    pub async fn search(&self, filters: &SearchFilters) -> Result<Vec<SearchResult>> {
        let mut params: Vec<(&str, String)> = vec![
            ("gameId", MINECRAFT_GAME_ID.to_string()),
            ("pageSize", filters.limit.to_string()),
            ("index", filters.offset.to_string()),
        ];

        if !filters.query.is_empty() {
            params.push(("searchFilter", filters.query.clone()));
        }
        if let Some(ref v) = filters.version {
            params.push(("gameVersion", v.clone()));
        }
        if let Some(ref t) = filters.project_type {
            if let Some(class_id) = class_id_for_type(t) {
                params.push(("classId", class_id.to_string()));
            }
        }
        if let Some(ref l) = filters.loader {
            if let Some(loader_id) = loader_id(l) {
                params.push(("modLoaderType", loader_id.to_string()));
            }
        }

        let sort_field = match filters.sort.to_lowercase().as_str() {
            "popularity" => "2",
            "downloads" => "6",
            "updated" => "3",
            "name" => "4",
            "author" => "5",
            _ => "3",  // last updated fallback
        };
        params.push(("sortField", sort_field.to_owned()));

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

        let resp: SearchResponse = response.json().await?;

        Ok(resp
            .data
            .into_iter()
            .map(|m| {
                let author = m
                    .authors
                    .first()
                    .map(|a| a.name.clone())
                    .unwrap_or_default();
                let versions: Vec<String> = m.game_versions.clone();
                let categories: Vec<String> = m.categories.iter().map(|c| c.name.clone()).collect();
                let loaders: Vec<String> = m
                    .mod_loaders
                    .iter()
                    .map(|ml| {
                        ml.name.split('-').next().unwrap_or(&ml.name).to_lowercase()
                    })
                    .collect();

                SearchResult {
                    title: m.name,
                    author,
                    description: m.summary,
                    downloads: m.downloads,
                    follows: 0,
                    versions: versions.clone(),
                    platform: Platform::CurseForge,
                    url: m.links.website_url.or(Some(format!(
                        "https://www.curseforge.com/minecraft/mc-mods/{}",
                        m.slug
                    ))),
                    icon_url: m.logo.and_then(|l| l.url),
                    project_type: categories.first().cloned().unwrap_or_default(),
                    license: None,
                    latest_version: versions.first().cloned(),
                    loaders,
                }
            })
            .collect())
    }
}

fn class_id_for_type(t: &str) -> Option<i32> {
    match t.to_lowercase().as_str() {
        "mod" | "mc-mods" => Some(6),
        "resourcepack" | "resource-pack" | "resource_pack" | "texture-pack" => Some(12),
        "world" | "worlds" => Some(14),
        "datapack" | "data-pack" | "data_pack" => Some(17),
        "addon" | "add-on" => Some(4550),
        _ => None,
    }
}

fn loader_id(l: &str) -> Option<i32> {
    match l.to_lowercase().as_str() {
        "forge" => Some(1),
        "fabric" => Some(2),
        "neoforge" | "neo" => Some(5),
        "quilt" => Some(6),
        _ => None,
    }
}
