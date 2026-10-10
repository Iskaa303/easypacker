use clap::{Args, Parser, Subcommand};
use color_eyre::eyre::{Result, bail};
use std::path::Path;

use crate::api::types::Platform;
use crate::config::Config;
use crate::project::{self, Manifest, ModSpec};
use crate::{api, lock, search};

#[derive(Parser, Debug)]
#[command(
    name = "easypacker",
    version,
    about = "Minecraft Modpack creator",
    disable_version_flag = true
)]
pub struct Cli {
    /// Print easypacker version
    #[arg(short = 'V', long = "ver", action = clap::ArgAction::Version)]
    pub app_version: Option<bool>,

    /// Run in this directory instead of the current one
    #[arg(short = 'C', long, global = true)]
    pub dir: Option<String>,

    #[command(subcommand)]
    pub cmd: Option<Cmd>,
}

#[derive(Subcommand, Debug)]
pub enum Cmd {
    /// Search projects (same filters as the TUI search screen)
    Search(SearchArgs),
    /// Create a new project (Easypacker.toml) in the current directory
    Init(ProjectArgs),
    /// Add a mod/resourcepack/datapack/shader to the pack
    Add(AddArgs),
    /// Remove an entry from the pack
    Remove(RemoveArgs),
    /// List the versions a project has (newest first)
    Versions(VersionsArgs),
    /// Pin an entry to a version (or back to the newest matching one)
    Pin(PinArgs),
    /// Link an entry to the same project on the other platform
    Link(LinkArgs),
    /// Edit project settings
    Set(ProjectArgs),
    /// List the pack contents
    List(ListArgs),
    /// Export the pack: curseforge, modrinth or prism
    Export(ExportArgs),
}

#[derive(Args, Debug)]
pub struct SearchArgs {
    /// Search query
    pub query: String,

    /// Platform to search: modrinth, curseforge, both (default: both)
    #[arg(long, default_value = "both")]
    pub platform: String,

    /// Minecraft version filter (e.g. 1.20.1)
    #[arg(short, long)]
    pub version: Option<String>,

    /// Mod loader filter (fabric, forge, neoforge, quilt)
    #[arg(short, long)]
    pub loader: Option<String>,

    /// Project type (mod, resourcepack, datapack, shader)
    #[arg(short = 'y', long, default_value = "mod")]
    pub r#type: String,

    /// Sort order (relevance, downloads, follows, updated)
    #[arg(short, long, default_value = "relevance")]
    pub sort: String,

    /// Max results
    #[arg(short = 'n', long, default_value_t = 25)]
    pub limit: usize,

    /// Result offset
    #[arg(long, default_value_t = 0)]
    pub offset: usize,

    /// `CurseForge` API key (overrides config file / env)
    #[arg(long)]
    pub api_key: Option<String>,
}

/// Project fields shared by `init` and `set`.
#[derive(Args, Debug, Default)]
pub struct ProjectArgs {
    #[arg(long)]
    pub name: Option<String>,
    #[arg(long)]
    pub version: Option<String>,
    #[arg(long)]
    pub authors: Option<String>,
    #[arg(long)]
    pub credits: Option<String>,
    #[arg(long)]
    pub description: Option<String>,
    #[arg(long)]
    pub minecraft: Option<String>,
    #[arg(long)]
    pub loader: Option<String>,
    /// Comma-separated platform list (modrinth, curseforge)
    #[arg(long)]
    pub platforms: Option<String>,
    #[arg(long)]
    pub website: Option<String>,
    #[arg(long)]
    pub discord: Option<String>,
    #[arg(long)]
    pub github: Option<String>,
}

#[derive(Args, Debug)]
pub struct AddArgs {
    /// Modrinth slug, `CurseForge` slug, or `CurseForge` project id
    pub id: String,

    /// Project type (`mod`, `resourcepack`, `datapack`, `shader`)
    #[arg(short = 'y', long, default_value = "mod")]
    pub r#type: String,

    /// Pin a version (platform display name, e.g. `"1.21.1-3.16.2"`)
    #[arg(long)]
    pub version: Option<String>,

    /// Platform to add on: modrinth, curseforge, both (default: pack targets)
    #[arg(long)]
    pub platform: Option<String>,

    /// Force the side this project runs on: client, server, both
    #[arg(long)]
    pub side: Option<String>,
}

#[derive(Args, Debug)]
pub struct RemoveArgs {
    /// Manifest key (or modrinth slug / curseforge project id)
    pub id: String,

    /// Project type (mod, resourcepack, datapack, shader)
    #[arg(short = 'y', long, default_value = "mod")]
    pub r#type: String,
}

#[derive(Args, Debug)]
pub struct VersionsArgs {
    /// Modrinth slug, `CurseForge` slug, or `CurseForge` project id
    pub id: String,

    /// Project type (`mod`, `resourcepack`, `datapack`, `shader`)
    #[arg(short = 'y', long, default_value = "mod")]
    pub r#type: String,

    /// Which platform to list: modrinth, curseforge, both (default: pack targets)
    #[arg(long)]
    pub platform: Option<String>,

    /// How many versions per platform
    #[arg(short = 'n', long, default_value_t = 15)]
    pub limit: usize,

    /// `CurseForge` API key (overrides config file / env)
    #[arg(long)]
    pub api_key: Option<String>,
}

#[derive(Args, Debug)]
pub struct PinArgs {
    /// Manifest key (or modrinth slug / curseforge project id) of the entry
    pub id: String,

    /// Version to pin (omit or `latest` for the newest matching one)
    pub version: Option<String>,

    /// Project type (`mod`, `resourcepack`, `datapack`, `shader`)
    #[arg(short = 'y', long, default_value = "mod")]
    pub r#type: String,

    /// Only pin this platform's version (modrinth, curseforge)
    #[arg(long)]
    pub platform: Option<String>,
}

#[derive(Args, Debug)]
pub struct LinkArgs {
    /// Manifest key (or modrinth slug / curseforge project id) of the entry
    pub id: String,

    /// The counterpart: Modrinth slug, `CurseForge` slug, or `CurseForge` project id
    pub counterpart: String,

    /// Project type (mod, resourcepack, datapack, shader)
    #[arg(short = 'y', long, default_value = "mod")]
    pub r#type: String,

    /// Which side the counterpart is: modrinth, curseforge (default: the missing side)
    #[arg(long)]
    pub platform: Option<String>,

    /// Pin the counterpart's version (its platform display name)
    #[arg(long)]
    pub version: Option<String>,
}

#[derive(Args, Debug)]
pub struct ListArgs {
    /// Only list this category (mod, resourcepack, datapack, shader)
    #[arg(short = 'y', long)]
    pub r#type: Option<String>,
}

#[derive(Args, Debug)]
pub struct ExportArgs {
    /// Export target: curseforge, modrinth or prism
    pub target: String,

    /// Server export: client-only mods are dropped (default: client)
    #[arg(long)]
    pub server: bool,

    /// Directory to write the exported pack to (default: current dir)
    #[arg(long)]
    pub output: Option<String>,
}

fn parse_side(s: Option<&str>) -> Result<Option<project::Side>> {
    match s {
        None => Ok(None),
        Some("client") => Ok(Some(project::Side::Client)),
        Some("server") => Ok(Some(project::Side::Server)),
        Some("both") => Ok(Some(project::Side::Both)),
        Some(other) => bail!("unknown side \"{other}\" (client, server, both)"),
    }
}

/// Which platforms an `add` should target: the --platform flag, or the
/// pack's own targets (default modrinth when the pack targets nothing).
fn add_platforms(args: &AddArgs, manifest: &Manifest) -> Vec<Platform> {
    target_platforms(args.platform.as_deref(), manifest)
}

/// The platform set a `--platform` value selects: `both`, a single platform,
/// or (with no flag) the pack's own targets.
fn target_platforms(requested: Option<&str>, manifest: &Manifest) -> Vec<Platform> {
    let want = |p: Platform| match requested {
        None => manifest.project.platforms.iter().any(|t| match p {
            Platform::Modrinth => t.eq_ignore_ascii_case("modrinth"),
            Platform::CurseForge => t.eq_ignore_ascii_case("curseforge"),
        }),
        Some("both") => true,
        Some("modrinth") => p == Platform::Modrinth,
        Some("curseforge") => p == Platform::CurseForge,
        _ => false,
    };
    let mut out = Vec::new();
    for p in [Platform::Modrinth, Platform::CurseForge] {
        if want(p) {
            out.push(p);
        }
    }
    if out.is_empty() {
        out.push(Platform::Modrinth);
    }
    out
}

/// Truncate to `max` characters (char-safe) with an ellipsis.
fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_owned();
    }
    let keep: String = s.chars().take(max.saturating_sub(1)).collect();
    format!("{keep}…")
}

/// Create a new project in `dir`. Errors if a manifest already exists.
pub fn run_init(args: &ProjectArgs, dir: &Path) -> Result<()> {
    let dir_name = dir
        .file_name()
        .map_or_else(String::new, |n| n.to_string_lossy().into_owned());
    let project = project::ModpackProject {
        name: args
            .name
            .clone()
            .unwrap_or_else(|| project::slugify(&dir_name)),
        version: args.version.clone().unwrap_or_else(|| "0.0.1".into()),
        authors: args.authors.clone().unwrap_or_default(),
        credits: args.credits.clone().unwrap_or_default(),
        description: args.description.clone().unwrap_or_default(),
        minecraft: args.minecraft.clone().unwrap_or_default(),
        loader: args.loader.clone().unwrap_or_default(),
        platforms: split_platforms(args.platforms.as_deref()),
        links: project::ProjectLinks {
            website: args.website.clone(),
            discord: args.discord.clone(),
            github: args.github.clone(),
        },
    };
    Manifest::init_project(dir, project)?;
    println!("Created {}/{}", dir.display(), project::MANIFEST_FILE);
    Ok(())
}

/// Apply only the provided fields to the existing project, then rebuild the
/// lockfile (minecraft/loader/platforms all feed resolution).
pub async fn run_set(args: &ProjectArgs, dir: &Path, cfg: &Config) -> Result<()> {
    let mut manifest = load_manifest(dir)?;
    let p = &mut manifest.project;
    if let Some(v) = &args.name {
        p.name.clone_from(v);
    }
    if let Some(v) = &args.version {
        p.version.clone_from(v);
    }
    if let Some(v) = &args.authors {
        p.authors.clone_from(v);
    }
    if let Some(v) = &args.credits {
        p.credits.clone_from(v);
    }
    if let Some(v) = &args.description {
        p.description.clone_from(v);
    }
    if let Some(v) = &args.minecraft {
        p.minecraft.clone_from(v);
    }
    if let Some(v) = &args.loader {
        p.loader.clone_from(v);
    }
    if let Some(v) = args.platforms.as_deref() {
        p.platforms = split_platforms(Some(v));
    }
    if let Some(v) = &args.website {
        p.links.website = Some(v.clone());
    }
    if let Some(v) = &args.discord {
        p.links.discord = Some(v.clone());
    }
    if let Some(v) = &args.github {
        p.links.github = Some(v.clone());
    }
    manifest.save(dir)?;
    regen_lock(dir, cfg).await?;
    println!("Updated {}/{}", dir.display(), project::MANIFEST_FILE);
    Ok(())
}

/// Add one entry: resolve its identity on the requested platforms, write a
/// manifest entry (the player's version pin goes in the shared `version` so
/// both platforms inherit it), then rebuild the lockfile. A platform where
/// the id doesn't exist is skipped with a `link` hint — the same project
/// often has different slugs/ids on modrinth vs curseforge. Re-adding an
/// existing entry only updates what resolved: platforms that failed to
/// resolve (or weren't requested) keep their recorded identity.
pub async fn run_add(args: &AddArgs, dir: &Path, cfg: &Config) -> Result<()> {
    let mut manifest = load_manifest(dir)?;
    let id = args.id.trim();
    let (key, mr_slug, cf_id, failed) =
        resolve_entry_identity(id, &add_platforms(args, &manifest), &args.r#type, dir, cfg).await;
    let Some(key) = key else {
        bail!(
            "{}",
            failed
                .iter()
                .map(|(_, m)| m.as_str())
                .collect::<Vec<_>>()
                .join("\n")
        );
    };

    // Merge into an existing entry instead of replacing it: re-adding must
    // not clobber a platform identity that a previous `link` recorded.
    let (mut version, mut side, mut modrinth, mut curseforge) =
        match manifest.cat(&args.r#type).get(&key).cloned() {
            Some(ModSpec::Simple(v)) => (
                Some(v),
                None,
                Some(project::ModrinthSpec::default()),
                Some(project::CurseForgeSpec::default()),
            ),
            Some(ModSpec::Detailed(d)) => (d.version, d.side, d.modrinth, d.curseforge),
            None => (None, None, None, None),
        };
    if args.version.is_some() {
        version = args.version.clone();
    }
    if let Some(s) = parse_side(args.side.as_deref())? {
        side = Some(s);
    }
    if let Some(slug) = mr_slug {
        modrinth = Some(project::ModrinthSpec {
            slug: Some(slug),
            version: None,
        });
    }
    if let Some(project_id) = cf_id {
        curseforge = Some(project::CurseForgeSpec {
            project_id: Some(project_id),
            version: None,
            ..Default::default()
        });
    }
    let spec = ModSpec::Detailed(project::DetailedSpec {
        version,
        modrinth,
        curseforge,
        side,
    });
    manifest.cat_mut(&args.r#type).insert(key.clone(), spec);
    manifest.save(dir)?;
    regen_lock(dir, cfg).await?;
    println!("Added {key} to [{}]", args.r#type);
    // Only warn about failures that left a platform genuinely unlinked.
    for (platform, msg) in failed {
        let still_missing = match platform {
            Platform::Modrinth => manifest
                .cat(&args.r#type)
                .get(&key)
                .is_some_and(|s| !s.has_modrinth()),
            Platform::CurseForge => manifest
                .cat(&args.r#type)
                .get(&key)
                .is_some_and(|s| !s.has_curseforge()),
        };
        if still_missing {
            eprintln!("easypacker: skipped a platform: {msg}");
        }
    }
    Ok(())
}

/// Resolve `id` on each requested platform for `run_add`. Returns the
/// manifest key candidate, the platform identities that resolved, and the
/// failures (each with a `link` hint) — a failed platform is not fatal when
/// the id resolves somewhere else.
async fn resolve_entry_identity(
    id: &str,
    platforms: &[Platform],
    project_type: &str,
    dir: &Path,
    cfg: &Config,
) -> (
    Option<String>,
    Option<String>,
    Option<i64>,
    Vec<(Platform, String)>,
) {
    let mut key: Option<String> = None;
    let mut mr_slug = None;
    let mut cf_id = None;
    let mut failed: Vec<(Platform, String)> = Vec::new();
    for &platform in platforms {
        match platform {
            Platform::Modrinth => match api::ModrinthClient::get_project(id).await {
                Ok(proj) => {
                    key = Some(proj.slug.clone());
                    mr_slug = Some(proj.slug);
                }
                Err(e) => failed.push((
                    Platform::Modrinth,
                    format!(
                        "modrinth: no project \"{id}\" ({e}) — link it later with:\n\
                         easypacker link <key> <modrinth-slug> --platform modrinth"
                    ),
                )),
            },
            Platform::CurseForge => {
                let api_key = match cfg.get_api_key(None, Some(dir)) {
                    Ok(k) => k,
                    Err(e) => {
                        failed.push((Platform::CurseForge, e.to_string()));
                        continue;
                    }
                };
                let client = api::CurseForgeClient::new(&api_key);
                let found = match id.parse::<i32>() {
                    Ok(pid) => client
                        .get_mod(pid)
                        .await
                        .map(|(mid, title, _)| (mid, title))
                        .map_err(|e| format!("curseforge: no project {pid}: {e}")),
                    Err(_) => client
                        .find_by_slug(id, Some(class_id(project_type)))
                        .await
                        .map_err(|e| format!("curseforge: {e}"))
                        .and_then(|found| {
                            found
                                .ok_or_else(|| format!("curseforge: no project with slug \"{id}\""))
                        }),
                };
                match found {
                    Ok((mod_id, title)) => {
                        cf_id = Some(i64::from(mod_id));
                        key.get_or_insert_with(|| project::slugify(&title));
                    }
                    Err(e) => failed.push((
                        Platform::CurseForge,
                        format!(
                            "{e} — link it later with:\n\
                             easypacker link <key> <curseforge-project-id> --platform curseforge"
                        ),
                    )),
                }
            }
        }
    }
    (key, mr_slug, cf_id, failed)
}

/// Link an existing entry to the same project on the other platform — the
/// case where the mod has different slugs/ids/names on modrinth vs
/// curseforge. Mirrors the TUI's link-version popup (`l` in file browse):
/// record the counterpart identity so the lockfile can pin both sides; the
/// sha1 cross-match attaches the right file even when names differ.
pub async fn run_link(args: &LinkArgs, dir: &Path, cfg: &Config) -> Result<()> {
    let mut manifest = load_manifest(dir)?;
    let id = args.id.trim();
    let cat = &args.r#type;
    let key = if manifest.cat(cat).contains_key(id) {
        Some(id.to_owned())
    } else {
        manifest.key_for(cat, Some(id), id.parse::<i64>().ok())
    };
    let Some(key) = key else {
        bail!("\"{id}\" is not in the pack's [{cat}]");
    };

    // Simple entries target all platforms; convert to Detailed so the
    // linked platform can carry its own identity while the rest inherits.
    let d = match manifest.cat(cat).get(&key).cloned() {
        Some(ModSpec::Simple(v)) => {
            let mut d = project::DetailedSpec {
                version: Some(v),
                ..project::DetailedSpec::default()
            };
            // Simple meant "both platforms, slug = key" — keep modrinth on.
            d.modrinth = Some(project::ModrinthSpec::default());
            d
        }
        Some(ModSpec::Detailed(d)) => d,
        None => bail!("\"{id}\" is not in the pack's [{cat}]"),
    };
    let has_mr = d.modrinth.is_some();
    let has_cf = d.curseforge.is_some();
    let platform = match args.platform.as_deref() {
        Some("modrinth") => Platform::Modrinth,
        Some("curseforge") => Platform::CurseForge,
        Some(other) => bail!("unknown platform \"{other}\" (modrinth, curseforge)"),
        None => match (!has_mr, !has_cf) {
            (true, false) => Platform::Modrinth,
            (false, true) => Platform::CurseForge,
            _ => bail!(
                "{key} already has identities on both platforms — pass --platform to replace one side"
            ),
        },
    };

    let mut d = d;
    match platform {
        Platform::Modrinth => {
            let proj = api::ModrinthClient::get_project(&args.counterpart)
                .await
                .map_err(|e| {
                    color_eyre::eyre::eyre!("modrinth: no project \"{}\": {e}", args.counterpart)
                })?;
            d.modrinth = Some(project::ModrinthSpec {
                slug: Some(proj.slug),
                version: args.version.clone(),
            });
        }
        Platform::CurseForge => {
            let client = api::CurseForgeClient::new(&cfg.get_api_key(None, Some(dir))?);
            let mod_id = match args.counterpart.parse::<i32>() {
                Ok(pid) => pid,
                Err(_) => {
                    client
                        .find_by_slug(&args.counterpart, Some(class_id(cat)))
                        .await?
                        .ok_or_else(|| {
                            color_eyre::eyre::eyre!(
                                "curseforge: no project with slug \"{}\"",
                                args.counterpart
                            )
                        })?
                        .0
                }
            };
            d.curseforge = Some(project::CurseForgeSpec {
                project_id: Some(i64::from(mod_id)),
                version: args.version.clone(),
                ..Default::default()
            });
        }
    }
    manifest
        .cat_mut(cat)
        .insert(key.clone(), ModSpec::Detailed(d));
    manifest.save(dir)?;
    regen_lock(dir, cfg).await?;
    println!(
        "Linked {key} to {} ({})",
        match platform {
            Platform::Modrinth => "modrinth",
            Platform::CurseForge => "curseforge",
        },
        args.counterpart
    );
    Ok(())
}

/// List the versions a project has, newest first, with the exact id the
/// lockfile would pin — the counterpart to choosing a version in the TUI.
pub async fn run_versions(args: &VersionsArgs, dir: &Path, cfg: &Config) -> Result<()> {
    let manifest = load_manifest(dir)?;
    let mc = manifest.project.minecraft.clone();
    let loader = manifest.project.loader.clone();
    let is_mod = args.r#type == "mod";
    let mc_filter = (!mc.is_empty()).then_some(mc.as_str());
    let loader_filter = (is_mod && !loader.is_empty()).then_some(loader.as_str());
    let class_id = class_id(&args.r#type);
    let id = args.id.trim();
    let mut listed = 0usize;

    for platform in target_platforms(args.platform.as_deref(), &manifest) {
        match platform {
            Platform::Modrinth => {
                let project = api::ModrinthClient::get_project(id)
                    .await
                    .map_err(|e| color_eyre::eyre::eyre!("modrinth: no project \"{id}\": {e}"))?;
                let versions =
                    api::ModrinthClient::get_versions(&project.slug, mc_filter, loader_filter)
                        .await?;
                println!(
                    "Modrinth {} ({}) — {} version(s)",
                    project.title,
                    project.slug,
                    versions.len()
                );
                for v in versions.iter().take(args.limit) {
                    listed += 1;
                    println!(
                        "  {:>3}. {:<44} MC:{} {:<10} {} {:>10}↓  id {}",
                        listed,
                        truncate(&v.name, 44),
                        v.game_versions.first().map_or("-", String::as_str),
                        v.loaders.join(","),
                        &v.date_published[..v.date_published.len().min(10)],
                        v.downloads,
                        v.id
                    );
                }
            }
            Platform::CurseForge => {
                let client = api::CurseForgeClient::new(
                    &cfg.get_api_key(args.api_key.as_deref(), Some(dir))?,
                );
                let (mod_id, title) = match id.parse::<i32>() {
                    Ok(pid) => {
                        let (mid, title, _) = client.get_mod(pid).await?;
                        (mid, title)
                    }
                    Err(_) => client
                        .find_by_slug(id, Some(class_id))
                        .await?
                        .ok_or_else(|| {
                            color_eyre::eyre::eyre!("curseforge: no project with slug \"{id}\"")
                        })?,
                };
                let files = client.get_files(mod_id, mc_filter, loader_filter).await?;
                println!("CurseForge {title} ({mod_id}) — {} file(s)", files.len());
                for f in files.iter().take(args.limit) {
                    listed += 1;
                    let (mc_versions, loaders) = search::split_cf_versions(&f.game_versions);
                    println!(
                        "  {:>3}. {:<44} MC:{} {:<10} {} {:>10}↓  id {}",
                        listed,
                        truncate(&f.display_name, 44),
                        mc_versions.first().map_or("-", String::as_str),
                        loaders.join(","),
                        &f.file_date[..f.file_date.len().min(10)],
                        f.download_count,
                        f.id
                    );
                }
            }
        }
    }
    if listed == 0 {
        println!("No matching versions (check the pack's minecraft/loader).");
    }
    Ok(())
}

/// Pin an existing entry to one version, on every platform or just one.
/// Pinning the shared version clears platform overrides that would otherwise
/// silently win — `easypacker versions` lists the names to use.
pub async fn run_pin(args: &PinArgs, dir: &Path, cfg: &Config) -> Result<()> {
    let mut manifest = load_manifest(dir)?;
    let id = args.id.trim();
    let cat = &args.r#type;
    let key = if manifest.cat(cat).contains_key(id) {
        Some(id.to_owned())
    } else {
        manifest.key_for(cat, Some(id), id.parse::<i64>().ok())
    };
    let Some(key) = key else {
        bail!("\"{id}\" is not in the pack's [{cat}]");
    };
    // A pin of "latest" is the same as no pin at all.
    let version = args
        .version
        .clone()
        .filter(|v| !project::is_latest(v.trim()))
        .map(|v| v.trim().to_owned());

    let mut d = match manifest.cat(cat).get(&key).cloned() {
        Some(ModSpec::Simple(v)) => {
            let mut d = project::DetailedSpec {
                version: Some(v),
                ..project::DetailedSpec::default()
            };
            d.modrinth = Some(project::ModrinthSpec::default());
            d.curseforge = Some(project::CurseForgeSpec::default());
            d
        }
        Some(ModSpec::Detailed(d)) => d,
        None => bail!("\"{id}\" is not in the pack's [{cat}]"),
    };

    match args.platform.as_deref() {
        None => {
            if d.modrinth.as_ref().is_some_and(|m| m.version.is_some())
                || d.curseforge.as_ref().is_some_and(|c| c.version.is_some())
            {
                println!("note: clearing per-platform version overrides for {key}");
            }
            if let Some(m) = d.modrinth.as_mut() {
                m.version = None;
            }
            if let Some(c) = d.curseforge.as_mut() {
                c.version = None;
            }
            d.version = version.clone();
        }
        Some("modrinth") => {
            let Some(m) = d.modrinth.as_mut() else {
                bail!("{key} is not on modrinth");
            };
            m.version.clone_from(&version);
        }
        Some("curseforge") => {
            let Some(c) = d.curseforge.as_mut() else {
                bail!("{key} is not on curseforge");
            };
            c.version.clone_from(&version);
        }
        Some(other) => bail!("unknown platform \"{other}\" (modrinth, curseforge)"),
    }
    manifest
        .cat_mut(cat)
        .insert(key.clone(), ModSpec::Detailed(d));
    manifest.save(dir)?;
    regen_lock(dir, cfg).await?;
    println!(
        "Pinned {key} to {}",
        version.as_deref().unwrap_or(project::LATEST)
    );
    Ok(())
}

/// Remove an entry by manifest key, modrinth slug, or curseforge project id.
pub async fn run_remove(args: &RemoveArgs, dir: &Path, cfg: &Config) -> Result<()> {
    let mut manifest = load_manifest(dir)?;
    let id = args.id.trim();
    let key = if manifest.cat(&args.r#type).contains_key(id) {
        Some(id.to_owned())
    } else {
        manifest.key_for(&args.r#type, Some(id), id.parse::<i64>().ok())
    };
    let Some(key) = key else {
        bail!("\"{id}\" is not in the pack's [{}]", args.r#type);
    };
    manifest.cat_mut(&args.r#type).remove(&key);
    manifest.save(dir)?;
    regen_lock(dir, cfg).await?;
    println!("Removed {id} from [{}]", args.r#type);
    Ok(())
}

/// Print the manifest contents: key, pinned version (or "latest"), and the
/// platforms each entry targets.
pub fn run_list(args: &ListArgs, dir: &Path) -> Result<()> {
    let manifest = load_manifest(dir)?;
    let project = &manifest.project;
    println!(
        "{} {} (minecraft {}, loader {}, platforms [{}])",
        project.name,
        project.version,
        if project.minecraft.is_empty() {
            "-"
        } else {
            &project.minecraft
        },
        if project.loader.is_empty() {
            "-"
        } else {
            &project.loader
        },
        project.platforms.join(", ")
    );
    for (name, map) in [
        ("mods", &manifest.mods),
        ("resourcepacks", &manifest.resourcepacks),
        ("datapacks", &manifest.datapacks),
        ("shaders", &manifest.shaders),
    ] {
        if args.r#type.as_deref().is_some_and(|t| t != name) || map.is_empty() {
            continue;
        }
        println!();
        println!("[{name}]");
        for (k, v) in map.iter() {
            let mut platforms = Vec::new();
            if v.has_modrinth() {
                platforms.push("modrinth");
            }
            if v.has_curseforge() {
                platforms.push("curseforge");
            }
            let version = v
                .version_for(true)
                .or_else(|| v.version_for(false))
                .unwrap_or_else(|| "latest".to_owned());
            println!("  {k} = \"{version}\"  ({})", platforms.join(", "));
        }
    }
    Ok(())
}

fn split_platforms(s: Option<&str>) -> Vec<String> {
    s.map_or_else(Vec::new, |s| {
        s.split(',')
            .map(str::trim)
            .filter(|p| !p.is_empty())
            .map(str::to_owned)
            .collect()
    })
}

fn class_id(project_type: &str) -> i32 {
    project::CATEGORIES
        .iter()
        .find(|(name, _)| *name == project_type)
        .map_or(6, |(_, id)| *id)
}

fn load_manifest(dir: &Path) -> Result<Manifest> {
    Manifest::load(dir).map_err(|e| {
        color_eyre::eyre::eyre!(
            "no {}/{} to work with: {e}",
            dir.display(),
            project::MANIFEST_FILE
        )
    })
}

/// Regenerate Easypacker.lock; the run fails only if every entry fails to
/// resolve (an untouched good lockfile is kept otherwise).
async fn regen_lock(dir: &Path, cfg: &Config) -> Result<()> {
    let report = lock::generate(dir, cfg).await?;
    println!(
        "Easypacker.lock: {} pinned, {} failed",
        report.resolved, report.failed
    );
    for warning in &report.warnings {
        eprintln!("easypacker: {warning}");
    }
    Ok(())
}
