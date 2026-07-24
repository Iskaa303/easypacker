mod curseforge;
mod modrinth;
pub mod types;
pub mod filters;

use crate::cli::Cli;
use crate::config::Config;
use color_eyre::eyre::Result;

pub use curseforge::CurseForgeClient;
pub use modrinth::ModrinthClient;
pub use types::{Platform, SearchFilters, SearchResult};

pub async fn run_cli(args: &Cli, cfg: &Config) -> Result<()> {
    let platform = if args.cf {
        Platform::CurseForge
    } else {
        Platform::Modrinth
    };

    let filters = SearchFilters {
        query: args.query.clone().unwrap_or_default(),
        version: args.version.clone(),
        loader: args.loader.clone(),
        project_type: args.r#type.clone().or(Some("mod".into())),
        sort: args.sort.clone(),
        limit: args.limit,
        offset: args.offset,
    };

    let results = match platform {
        Platform::Modrinth => ModrinthClient::search(&filters).await?,
        Platform::CurseForge => {
            match cfg.get_api_key(args.api_key.as_deref()) {
                Ok(key) => CurseForgeClient::new(&key).search(&filters).await?,
                Err(_) => {
                    eprintln!("CurseForge API key needed.");
                    eprintln!("Get a free key at: https://console.curseforge.com/");
                    eprint!("Paste your API key: ");
                    use std::io::Write;
                    std::io::stderr().flush().ok();
                    let key = rpassword::read_password()?;
                    if key.is_empty() {
                        return Err(color_eyre::eyre::eyre!("No API key entered."));
                    }
                    if let Err(e) = (Config { curseforge_api_key: Some(key.clone()) }).save() {
                        eprintln!("Warning: could not save API key: {e}");
                    }
                    CurseForgeClient::new(&key).search(&filters).await?
                }
            }
        }
    };
    let results: Vec<_> = results
        .into_iter()
        .filter(|r| {
            let t = r.project_type.to_lowercase();
            let pt = filters.project_type.as_deref().unwrap_or("mod");
            // Keep only results matching the user's chosen type (mandatory single-select)
            let allowed = pt.split(", ").map(|s| s.trim()).collect::<Vec<_>>();
            allowed.contains(&t.as_str())
        })
        .collect();


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
        println!(
            "     License: {}  Loaders: {}  MC: {}",
            license, loaders, latest
        );
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

fn truncate(s: &str, max: usize) -> String {
    if s.len() > max {
        format!("{}…", &s[..max.saturating_sub(1)])
    } else {
        s.to_owned()
    }
}
