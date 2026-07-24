use super::{Platform, SearchFilters, SearchResult};
use color_eyre::eyre::Result;
use serde::Deserialize;

const BASE: &str = "https://api.modrinth.com/v2";

pub struct ModrinthClient;

#[derive(Deserialize)]
struct SearchResponse {
    hits: Vec<Hit>,
    #[allow(dead_code)]
    total_hits: u64,
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
    icon_url: Option<String>,
    #[allow(dead_code)]
    client_side: String,
    #[allow(dead_code)]
    server_side: String,
    license: Option<String>,
    #[allow(dead_code)]
    latest_version: Option<String>,
    categories: Vec<String>,
}

impl ModrinthClient {
    pub async fn search(filters: &SearchFilters) -> Result<Vec<SearchResult>> {
        let client = reqwest::Client::new();
        let mut query = vec![
            ("query".to_owned(), filters.query.clone()),
            ("limit".to_owned(), filters.limit.to_string()),
            ("offset".to_owned(), filters.offset.to_string()),
            ("index".to_owned(), normalize_sort(&filters.sort)),
        ];

        // Build facets
        let mut facets: Vec<Vec<String>> = Vec::new();

        fn push_facet(facets: &mut Vec<Vec<String>>, prefix: &str, val: &str) {
            let parts: Vec<String> = val.split(", ").filter(|s| !s.is_empty()).map(|s| format!("{prefix}:{s}")).collect();
            if !parts.is_empty() {
                facets.push(parts);
            }
        }

        if let Some(ref v) = filters.version { push_facet(&mut facets, "versions", v); }
        if let Some(ref c) = filters.category { push_facet(&mut facets, "categories", c); }
        if let Some(ref l) = filters.loader { push_facet(&mut facets, "loaders", l); }
        if let Some(ref t) = filters.project_type { push_facet(&mut facets, "project_type", t); }

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
                // categories includes both loaders and categories
                let (loaders, _cats): (Vec<_>, Vec<_>) = h.categories.into_iter().partition(|c| {
                    matches!(
                        c.to_lowercase().as_str(),
                        "fabric" | "forge" | "neoforge" | "quilt" | "rift"
                    )
                });
                let latest = h.versions.last().cloned();
                SearchResult {
                    title: h.title,
                    author: h.author,
                    description: h.description,
                    downloads: h.downloads,
                    follows: h.follows,
                    versions: h.versions,
                    platform: Platform::Modrinth,
                    url: Some(format!("https://modrinth.com/{}/{}", h.project_type, h.slug)),
                    icon_url: h.icon_url,
                    project_type: h.project_type,
                    license: h.license,
                    latest_version: latest,
                    loaders,
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
