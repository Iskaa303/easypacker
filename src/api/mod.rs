mod curseforge;
mod modrinth;

use crate::cli::Cli;
use crate::config::Config;
use color_eyre::eyre::Result;

pub use curseforge::CurseForgeClient;
pub use modrinth::ModrinthClient;

#[derive(Debug, Clone, PartialEq)]
pub enum Platform {
    Modrinth,
    CurseForge,
}

#[derive(Debug, Clone)]
pub struct SearchFilters {
    pub query: String,
    pub version: Option<String>,
    pub category: Option<String>,
    pub loader: Option<String>,
    pub project_type: Option<String>,
    pub sort: String,
    pub limit: usize,
    pub offset: usize,
}

#[derive(Debug, Clone)]
pub struct SearchResult {
    pub title: String,
    pub author: String,
    pub description: String,
    pub downloads: u64,
    pub follows: u64,
    #[allow(dead_code)]
    pub versions: Vec<String>,
    #[allow(dead_code)]
    pub platform: Platform,
    pub url: Option<String>,
    #[allow(dead_code)]
    pub icon_url: Option<String>,
    pub project_type: String,
    pub license: Option<String>,
    pub latest_version: Option<String>,
    pub loaders: Vec<String>,
}

pub async fn run_cli(args: &Cli, cfg: &Config) -> Result<()> {
    let platform = if args.cf {
        Platform::CurseForge
    } else {
        Platform::Modrinth
    };

    let filters = SearchFilters {
        query: args.query.clone().unwrap_or_default(),
        version: args.version.clone(),
        category: args.category.clone(),
        loader: args.loader.clone(),
        project_type: args.r#type.clone(),
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
                    eprintln!("CurseForge API key not found.");
                    eprintln!("Set it via:");
                    eprintln!("  --api-key <key>");
                    eprintln!("  export CURSEFORGE_API_KEY=<key>");
                    eprintln!("  or echo '{{\"curseforge_api_key\":\"...\"}}' > ~/.easypacker.json");
                    eprintln!();
                    eprintln!("Get a key at: https://console.curseforge.com/");
                    std::process::exit(1);
                }
            }
        }
    };

    println!("─── {} results ───", results.len());
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
        println!("     {}↓ {}★", r.downloads, r.follows);
        println!("     {}", url);
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
