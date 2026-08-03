use super::types::{CrossPlatform, Platform, SearchFilters, SearchResult};
use crate::api::filters;
use color_eyre::eyre::Result;
use serde::Deserialize;
use std::time::Duration;

const BASE: &str = "https://api.modrinth.com/v2";

pub struct ModrinthClient;

#[derive(Deserialize)]
struct SearchResponse {
    hits: Vec<Hit>,
}

#[derive(Deserialize)]
struct Hit {
    title: String,
    author: String,
    description: String,
    downloads: u64,
    follows: u64,
    versions: Vec<String>,
    slug: String,
    project_type: String,
    #[serde(default)]
    all_project_types: Vec<String>,
    icon_url: Option<String>,
    license: Option<String>,
    categories: Vec<String>,
}

#[derive(Deserialize)]
struct ModrinthVersion {
    id: String,
    name: String,
    #[serde(default)]
    version_number: String,
    #[serde(rename = "game_versions")]
    game_versions: Vec<String>,
    loaders: Vec<String>,
    #[serde(rename = "date_published")]
    date_published: String,
    downloads: u64,
    #[serde(default)]
    files: Vec<ModrinthVersionFile>,
}

#[derive(Deserialize)]
struct ModrinthVersionFile {
    url: Option<String>,
    #[serde(default)]
    filename: String,
    #[serde(default)]
    size: u64,
    #[serde(default)]
    primary: Option<bool>,
    #[serde(default)]
    hashes: std::collections::HashMap<String, String>,
}

#[derive(Deserialize)]
pub struct ModrinthProjectInfo {
    pub id: String,
    pub title: String,
    pub slug: String,
}

#[derive(Debug, Clone)]
pub struct ModrinthVersionInfo {
    pub id: String,
    pub name: String,
    pub version_number: String,
    pub game_versions: Vec<String>,
    pub loaders: Vec<String>,
    pub date_published: String,
    pub downloads: u64,
    pub url: Option<String>,
    pub filename: String,
    pub size: u64,
    pub sha1: Option<String>,
    pub sha512: Option<String>,
}

impl ModrinthClient {
    pub async fn search(filters: &SearchFilters) -> Result<Vec<SearchResult>> {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(15))
            .build()?;
        let mut query = vec![
            ("query".to_owned(), filters.query.clone()),
            ("limit".to_owned(), filters.limit.to_string()),
            ("offset".to_owned(), filters.offset.to_string()),
            ("index".to_owned(), normalize_sort(&filters.sort)),
        ];

        let mut facets: Vec<Vec<String>> = Vec::new();

        fn push_facet(facets: &mut Vec<Vec<String>>, prefix: &str, val: &str) {
            let parts: Vec<String> = val
                .split(", ")
                .filter(|s| !s.is_empty())
                .map(|s| format!("{prefix}:{s}"))
                .collect();
            if !parts.is_empty() {
                facets.push(parts);
            }
        }

        if let Some(ref v) = filters.version {
            push_facet(&mut facets, "versions", v);
        }
        if let Some(ref l) = filters.loader {
            push_facet(&mut facets, "categories", l);
        }
        if let Some(ref t) = filters.project_type {
            push_facet(&mut facets, "project_type", t);
        }

        if !facets.is_empty() {
            let encoded = serde_json::to_string(&facets)?;
            query.push(("facets".to_owned(), encoded));
        }

        let resp: SearchResponse = client
            .get(format!("{}/search", BASE))
            .query(&query)
            .send()
            .await?
            .json()
            .await?;

        Ok(resp
            .hits
            .into_iter()
            .map(|h| {
                let loaders: Vec<String> = h
                    .categories
                    .into_iter()
                    .filter(|c| filters::LOADERS.contains(&c.to_lowercase().as_str()))
                    .collect();
                let latest = h.versions.last().cloned();
                // Use the type that matches the user's filter, or fall back to primary
                let pt = filters.project_type.as_deref().unwrap_or("mod");
                let display_type = if h.all_project_types.iter().any(|t| t == pt) {
                    pt
                } else {
                    &h.project_type
                };
                SearchResult {
                    title: h.title,
                    author: h.author,
                    description: h.description,
                    downloads: h.downloads,
                    follows: h.follows,
                    platform: Platform::Modrinth,
                    url: Some(format!("https://modrinth.com/{}/{}", display_type, h.slug)),
                    icon_url: h.icon_url,
                    project_type: display_type.to_string(),
                    license: h.license,
                    latest_version: latest,
                    loaders,
                    cross: CrossPlatform {
                        curseforge_id: None,
                        modrinth_slug: Some(h.slug.clone()),
                        ..CrossPlatform::default()
                    },
                }
            })
            .collect())
    }

    pub async fn get_project(slug: &str) -> Result<ModrinthProjectInfo> {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(15))
            .build()?;
        Ok(client
            .get(format!("{}/project/{}", BASE, slug))
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?)
    }

    pub async fn get_versions(
        slug: &str,
        game_version: Option<&str>,
        loader: Option<&str>,
    ) -> Result<Vec<ModrinthVersionInfo>> {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(15))
            .build()?;
        let mut query: Vec<(&str, String)> = vec![("include_changelog", "false".into())];
        if let Some(gv) = game_version {
            query.push(("game_versions", serde_json::to_string(&[gv])?));
        }
        if let Some(l) = loader {
            query.push(("loaders", serde_json::to_string(&[l])?));
        }
        let resp: Vec<ModrinthVersion> = client
            .get(format!("{}/project/{}/version", BASE, slug))
            .query(&query)
            .send()
            .await?
            .json()
            .await?;
        Ok(resp
            .into_iter()
            .map(|v| {
                let primary = v
                    .files
                    .iter()
                    .find(|f| f.primary.unwrap_or(false))
                    .or_else(|| v.files.first());
                ModrinthVersionInfo {
                    id: v.id,
                    name: v.name,
                    version_number: v.version_number,
                    game_versions: v.game_versions,
                    loaders: v.loaders,
                    date_published: v.date_published,
                    downloads: v.downloads,
                    url: primary.and_then(|f| f.url.clone()),
                    filename: primary.map_or_else(String::new, |f| f.filename.clone()),
                    size: primary.map_or(0, |f| f.size),
                    sha1: primary.and_then(|f| f.hashes.get("sha1").cloned()),
                    sha512: primary.and_then(|f| f.hashes.get("sha512").cloned()),
                }
            })
            .collect())
    }
}

fn normalize_sort(s: &str) -> String {
    match s.to_lowercase().as_str() {
        "downloads" => "downloads".into(),
        "follows" => "follows".into(),
        "updated" => "updated".into(),
        "created" | "newest" => "created".into(),
        _ => "relevance".into(),
    }
}
