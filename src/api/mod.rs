mod curseforge;
pub mod filters;
mod modrinth;
pub mod types;

use crate::cli::SearchArgs;
use crate::config::Config;
use color_eyre::eyre::Result;
use std::io::Write;
use std::sync::OnceLock;
use std::time::Duration;

pub use curseforge::CurseForgeClient;
pub use curseforge::CurseForgeFileInfo;
pub use modrinth::ModrinthClient;
pub use modrinth::ModrinthProjectInfo;
pub use modrinth::ModrinthVersionInfo;
pub use types::{Platform, ProjectFile, SearchFilters, SearchResult};

/// One pooled HTTP client for the whole process.
///
/// Every API call and icon download used to build its own client, which meant
/// a fresh TCP connection and TLS handshake per request; sharing one client
/// reuses the connections instead.
pub fn http() -> &'static reqwest::Client {
    static CLIENT: OnceLock<reqwest::Client> = OnceLock::new();
    CLIENT.get_or_init(|| {
        reqwest::Client::builder()
            .timeout(Duration::from_secs(15))
            .build()
            .expect("building the HTTP client cannot fail")
    })
}

pub async fn run_cli(args: &SearchArgs, cfg: &Config) -> Result<()> {
    let filters = SearchFilters {
        query: args.query.clone(),
        version: args.version.clone(),
        loader: (args.r#type == "mod")
            .then(|| args.loader.clone())
            .flatten(),
        project_type: Some(args.r#type.clone()),
        sort: args.sort.clone(),
        limit: args.limit,
        offset: args.offset,
    };
    let platforms: Vec<String> = match args.platform.to_lowercase().as_str() {
        "modrinth" => vec!["modrinth".into()],
        "curseforge" => vec!["curseforge".into()],
        _ => vec!["modrinth".into(), "curseforge".into()],
    };
    let api_key = if platforms.iter().any(|p| p == "curseforge") {
        if let Ok(k) = cfg.get_api_key(args.api_key.as_deref(), None) {
            Some(k)
        } else {
            eprintln!("CurseForge API key needed.");
            eprintln!("Get a free key at: https://console.curseforge.com/");
            eprint!("Paste your API key: ");
            std::io::stderr().flush().ok();
            let key = rpassword::read_password()?;
            if key.is_empty() {
                return Err(color_eyre::eyre::eyre!("No API key entered."));
            }
            if let Err(e) = (Config {
                curseforge_api_key: Some(key.clone()),
            })
            .save()
            {
                eprintln!("Warning: could not save API key: {e}");
            }
            Some(key)
        }
    } else {
        None
    };

    let (results, errors) = search_all(&filters, &platforms, api_key.as_deref()).await;
    for e in errors {
        eprintln!("{e}");
    }

    for (i, r) in results.iter().enumerate() {
        let url = r.url.as_deref().unwrap_or("-");
        let license = r.license.as_deref().unwrap_or("-");
        let latest = r.latest_version.as_deref().unwrap_or("-");
        let loaders = if r.loaders.is_empty() {
            "-".into()
        } else {
            r.loaders.join(", ")
        };
        println!("{:>3}. {}", i + 1, r.title);
        println!("     by {}  [{}]", r.author, r.project_type);
        println!("     License: {license}  Loaders: {loaders}  MC: {latest}");
        let stats = if r.follows > 0 {
            format!("{}↓ {}★", r.downloads, r.follows)
        } else {
            format!("{}↓", r.downloads)
        };
        println!("     {stats}");
        println!("     {url}");
        if !r.description.is_empty() {
            println!("     {}", truncate(&r.description, 70));
        }
        println!();
    }

    Ok(())
}

/// Search the given platforms with the same filters and merge equal project
/// titles into one row (each platform's downloads/url land in `cross`).
/// Returns (merged results, per-platform errors). Shared by the CLI search
/// and the TUI search screen so both behave identically.
pub async fn search_all(
    filters: &SearchFilters,
    platforms: &[String],
    api_key: Option<&str>,
) -> (Vec<SearchResult>, Vec<String>) {
    let mut all: Vec<SearchResult> = Vec::new();
    let mut errors: Vec<String> = Vec::new();
    for p in platforms {
        let results = match p.as_str() {
            "modrinth" => match ModrinthClient::search(filters).await {
                Ok(r) => r,
                Err(e) => {
                    errors.push(format!("Modrinth error: {e}"));
                    continue;
                }
            },
            "curseforge" => {
                let Some(k) = api_key else {
                    errors.push("CurseForge API key not configured.".into());
                    continue;
                };
                match CurseForgeClient::new(k).search(filters).await {
                    Ok(r) => r,
                    Err(e) => {
                        errors.push(format!("CurseForge error: {e}"));
                        continue;
                    }
                }
            }
            _ => continue,
        };
        let mut results = results;
        if let Some(ref pt) = filters.project_type {
            let allowed: Vec<&str> = pt.split(", ").map(str::trim).collect();
            results.retain(|r| allowed.contains(&r.project_type.to_lowercase().as_str()));
        }
        for r in results {
            let lower = r.title.to_lowercase();
            if let Some(existing) = all.iter_mut().find(|e| e.title.to_lowercase() == lower) {
                match r.platform {
                    Platform::Modrinth => {
                        existing.cross.modrinth_downloads = r.downloads;
                        existing.cross.modrinth_url = r.url;
                        if r.cross.modrinth_slug.is_some() {
                            existing
                                .cross
                                .modrinth_slug
                                .clone_from(&r.cross.modrinth_slug);
                        }
                    }
                    Platform::CurseForge => {
                        existing.cross.curseforge_downloads = r.downloads;
                        existing.cross.curseforge_url = r.url;
                        // Keep the CF project id: without it the file browse
                        // cannot fetch this project's CF files, and adding it
                        // would silently drop the curseforge side.
                        if r.cross.curseforge_id.is_some() {
                            existing.cross.curseforge_id = r.cross.curseforge_id;
                        }
                    }
                }
            } else {
                let mut entry = r;
                match entry.platform {
                    Platform::Modrinth => {
                        entry.cross.modrinth_downloads = entry.downloads;
                        entry.cross.modrinth_url.clone_from(&entry.url);
                    }
                    Platform::CurseForge => {
                        entry.cross.curseforge_downloads = entry.downloads;
                        entry.cross.curseforge_url.clone_from(&entry.url);
                    }
                }
                all.push(entry);
            }
        }
    }
    (all, errors)
}

fn truncate(s: &str, max: usize) -> String {
    if s.len() > max {
        format!("{}…", &s[..max.saturating_sub(1)])
    } else {
        s.to_owned()
    }
}
