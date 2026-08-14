//! Export the pack (Easypacker.toml + Easypacker.lock) to `CurseForge`,
//! Modrinth (.mrpack) or Prism Launcher (MultiMC-style zip).
//!
//! All three targets are plain zips:
//! - curseforge: `manifest.json` + `overrides/` (non-mod files bundled)
//! - modrinth:   `modrinth.index.json` (launcher downloads by URL+hash)
//! - prism:      `mmc-pack.json` + `instance.cfg` + `minecraft/` (self-contained)
//!
//! The loader *name* comes from the manifest/lock; the loader *version* is
//! resolved from the platform's meta endpoints (fabric/quilt JSON, forge
//! promotions JSON, neoforge maven-metadata XML) since neither the toml nor
//! the lockfile pins it.

use crate::cli::Cli;
use crate::project::{Manifest, Side, slugify, tstr};
use color_eyre::eyre::{Result, bail};
use serde::Deserialize;
use serde_json::{Value, json};
use std::fmt::Write as _;
use std::io::Write;
use std::path::{Path, PathBuf};
use zip::ZipWriter;
use zip::write::SimpleFileOptions;

// ── Lockfile model (read-only view of Easypacker.lock) ────────────────

#[derive(Debug, Deserialize)]
pub struct LockPack {
    pub minecraft: String,
    pub loader: String,
    #[serde(default)]
    pub targets: Vec<String>,
    #[serde(default, rename = "mod")]
    pub mod_: Vec<LockEntry>,
    #[serde(default)]
    pub resourcepack: Vec<LockEntry>,
    #[serde(default)]
    pub datapack: Vec<LockEntry>,
    #[serde(default)]
    pub shader: Vec<LockEntry>,
}

#[derive(Debug, Deserialize)]
pub struct LockEntry {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub modrinth: Option<LockModrinth>,
    #[serde(default)]
    pub curseforge: Option<LockCurseForge>,
}

#[derive(Debug, Deserialize)]
pub struct LockModrinth {
    pub slug: String,
    pub project_id: String,
    pub version_id: String,
    pub filename: String,
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default)]
    pub size: u64,
    #[serde(default)]
    pub hashes: Option<LockHashes>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct LockCurseForge {
    pub project_id: i64,
    pub file_id: i64,
    pub filename: String,
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default)]
    pub hashes: Option<LockHashes>,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct LockHashes {
    #[serde(default)]
    pub sha1: Option<String>,
    #[serde(default)]
    pub sha512: Option<String>,
}

impl LockPack {
    pub fn load(dir: &Path) -> Result<Self> {
        let raw = std::fs::read_to_string(dir.join(crate::lock::LOCK_FILE)).map_err(|e| {
            color_eyre::eyre::eyre!(
                "export: cannot read {}: {e} — run easypacker in the project directory so the lockfile regenerates",
                crate::lock::LOCK_FILE
            )
        })?;
        Self::parse(&raw)
    }

    pub fn parse(raw: &str) -> Result<Self> {
        Ok(toml::from_str(raw)?)
    }
}

/// (category, entries, minecraft folder name)
const fn categories(lock: &LockPack) -> [(&str, &Vec<LockEntry>, &str); 4] {
    [
        ("mod", &lock.mod_, "mods"),
        ("resourcepack", &lock.resourcepack, "resourcepacks"),
        ("datapack", &lock.datapack, "datapacks"),
        ("shader", &lock.shader, "shaderpacks"),
    ]
}

// ── Export targets ─────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExportTarget {
    CurseForge,
    Modrinth,
    Prism,
}

impl ExportTarget {
    pub const ALL: [Self; 3] = [Self::CurseForge, Self::Modrinth, Self::Prism];

    pub fn parse(s: &str) -> Result<Self> {
        match s.to_lowercase().as_str() {
            "curseforge" | "cf" => Ok(Self::CurseForge),
            "modrinth" | "mr" => Ok(Self::Modrinth),
            "prism" | "mmc" | "multimc" => Ok(Self::Prism),
            other => bail!("unknown export target \"{other}\" (curseforge, modrinth, prism)"),
        }
    }

    pub fn from_index(i: usize) -> Self {
        Self::ALL[i.min(Self::ALL.len() - 1)]
    }

    pub const fn label(self) -> &'static str {
        match self {
            Self::CurseForge => "CurseForge",
            Self::Modrinth => "Modrinth",
            Self::Prism => "Prism Launcher",
        }
    }

    pub const fn ext(self) -> &'static str {
        match self {
            Self::CurseForge | Self::Prism => "zip",
            Self::Modrinth => "mrpack",
        }
    }

    /// Key used in the lockfile's `targets` list.
    pub const fn key(self) -> &'static str {
        match self {
            Self::CurseForge => "curseforge",
            Self::Modrinth => "modrinth",
            Self::Prism => "prism",
        }
    }
}

/// Which side the export targets. Server exports drop client-only entries
/// (and client-only categories) entirely.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExportSide {
    Client,
    Server,
}

/// The side an entry runs on: the player's forced manifest declaration, or
/// the platform default per category (mods both, resourcepacks/shaders
/// client-only, datapacks server-side).
fn side_of(pack: &Manifest, cat: &str, id: &str) -> Side {
    pack.cat(cat)
        .get(id)
        .and_then(super::project::ModSpec::side)
        .unwrap_or(match cat {
            "resourcepack" | "shader" => Side::Client,
            "datapack" => Side::Server,
            _ => Side::Both,
        })
}

/// Should this entry appear in the given export? Client exports ship
/// everything (server-side mods still run in singleplayer); server exports
/// drop client-only entries.
fn include(entry: Side, export: ExportSide) -> bool {
    export == ExportSide::Client || entry != Side::Client
}

/// Modrinth `modrinth.index.json` env for a mod, from its declared side.
const fn mod_env(side: Side) -> (&'static str, &'static str) {
    match side {
        Side::Both => ("required", "required"),
        Side::Client => ("required", "unsupported"),
        Side::Server => ("unsupported", "required"),
    }
}

pub struct ExportReport {
    pub path: PathBuf,
    pub written: usize,
    pub skipped: usize,
    pub warnings: Vec<String>,
}

/// Resolve the latest loader version for a minecraft version, from the
/// loader's own meta endpoint (fabric/quilt JSON, forge promotions JSON,
/// neoforge maven-metadata XML).
pub async fn loader_version(mc: &str, loader: &str) -> Result<String> {
    match loader.to_lowercase().as_str() {
        "fabric" => {
            first_loader_version(&format!(
                "https://meta.fabricmc.net/v2/versions/loader/{mc}"
            ))
            .await
        }
        "quilt" => {
            first_loader_version(&format!("https://meta.quiltmc.org/v3/versions/loader/{mc}")).await
        }
        "forge" => {
            let body = reqwest::get(
                "https://files.minecraftforge.net/net/minecraftforge/forge/promotions_slim.json",
            )
            .await?
            .error_for_status()?
            .text()
            .await?;
            let v: Value = serde_json::from_str(&body)?;
            let promos = &v["promos"];
            promos[format!("{mc}-latest").as_str()]
                .as_str()
                .or_else(|| promos[format!("{mc}-recommended").as_str()].as_str())
                .map(str::to_owned)
                .ok_or_else(|| color_eyre::eyre::eyre!("no forge build for {mc}"))
        }
        "neoforge" | "neo" => {
            let body = reqwest::get(
                "https://maven.neoforged.net/releases/net/neoforged/neoforge/maven-metadata.xml",
            )
            .await?
            .error_for_status()?
            .text()
            .await?;
            let prefix = neoforge_prefix(mc);
            extract_xml_versions(&body)
                .into_iter()
                .rfind(|v| v == &prefix || v.starts_with(&format!("{prefix}.")))
                .map(str::to_owned)
                .ok_or_else(|| color_eyre::eyre::eyre!("no neoforge build for {mc}"))
        }
        other => bail!("export: unsupported loader \"{other}\" (fabric, forge, neoforge, quilt)"),
    }
}

async fn first_loader_version(url: &str) -> Result<String> {
    let body = reqwest::get(url).await?.error_for_status()?.text().await?;
    let v: Value = serde_json::from_str(&body)?;
    v[0]["loader"]["version"]
        .as_str()
        .map(str::to_owned)
        .ok_or_else(|| color_eyre::eyre::eyre!("no loader version at {url}"))
}

/// Neoforge versions are `21.1.x` for MC `1.21.1`, `20.4.x` for `1.20.4`,
/// `21.0.x` for `1.21`. Strip the leading `1.`; a missing patch level maps
/// to `.0`.
pub fn neoforge_prefix(mc: &str) -> String {
    let core = mc.strip_prefix("1.").unwrap_or(mc);
    if core.split('.').count() == 1 {
        format!("{core}.0")
    } else {
        core.to_owned()
    }
}

/// Pull `<version>…</version>` tags out of a maven-metadata.xml (machine
/// generated, stable shape — no XML dep needed).
pub fn extract_xml_versions(xml: &str) -> Vec<&str> {
    xml.split("<version>")
        .skip(1)
        .filter_map(|part| part.split("</version>").next())
        .collect()
}

// ── Loader identity per target format ──────────────────────────────────

/// (mmc-pack.json component uid, cachedName)
pub fn loader_component(loader: &str) -> (&'static str, &'static str) {
    match loader.to_lowercase().as_str() {
        "fabric" => ("net.fabricmc.fabric-loader", "Fabric Loader"),
        "forge" => ("net.minecraftforge", "Forge"),
        "neoforge" | "neo" => ("net.neoforged", "NeoForge"),
        "quilt" => ("org.quiltmc.quilt-loader", "Quilt Loader"),
        _ => ("net.minecraft", "Minecraft"),
    }
}

/// mrpack `dependencies` key for the loader.
pub fn mrpack_dep_key(loader: &str) -> &'static str {
    match loader.to_lowercase().as_str() {
        "fabric" => "fabric-loader",
        "forge" => "forge",
        "neoforge" | "neo" => "neoforge",
        "quilt" => "quilt-loader",
        _ => "minecraft",
    }
}

/// `CurseForge` `manifest.json` loader id: `<loader>-<version>`.
pub fn cf_loader_id(loader: &str, ver: &str) -> String {
    format!("{}-{}", loader.to_lowercase(), ver)
}

// ── Manifest builders (pure, tested) ───────────────────────────────────

fn optional_str(s: &str) -> Option<&str> {
    (!s.is_empty()).then_some(s)
}

/// `CurseForge` `manifest.json`.
pub fn cf_manifest_json(
    pack: &Manifest,
    loader: &str,
    loader_ver: &str,
    files: &[(i64, i64)],
) -> Value {
    let p = &pack.project;
    let mut manifest = json!({
        "minecraft": {
            "version": p.minecraft,
            "modLoaders": [{ "id": cf_loader_id(loader, loader_ver), "primary": true }],
        },
        "manifestType": "minecraftModpack",
        "manifestVersion": 1,
        "name": p.name,
        "version": p.version,
        "files": files
            .iter()
            .map(|(project_id, file_id)| {
                json!({ "projectID": project_id, "fileID": file_id, "required": true })
            })
            .collect::<Vec<_>>(),
        "overrides": "overrides",
    });
    if let Some(author) = optional_str(&p.authors) {
        manifest["author"] = json!(author);
    }
    manifest
}

/// One mrpack file entry.
pub struct MrFile {
    pub path: String,
    pub sha1: String,
    pub sha512: String,
    pub url: String,
    pub size: u64,
    /// `required` / `optional` / `unsupported` per side.
    pub client: &'static str,
    pub server: &'static str,
}

/// Modrinth `modrinth.index.json`.
pub fn mr_index_json(pack: &Manifest, loader: &str, loader_ver: &str, files: &[MrFile]) -> Value {
    let p = &pack.project;
    let mut index = json!({
        "formatVersion": 1,
        "game": "minecraft",
        "versionId": p.version,
        "name": p.name,
        "files": files
            .iter()
            .map(|f| {
                json!({
                    "path": f.path,
                    "hashes": { "sha1": f.sha1, "sha512": f.sha512 },
                    "env": {
                        "client": f.client,
                        "server": f.server,
                    },
                    "downloads": [f.url],
                    "fileSize": f.size,
                })
            })
            .collect::<Vec<_>>(),
        "dependencies": {
            "minecraft": p.minecraft,
            mrpack_dep_key(loader): loader_ver,
        },
    });
    if let Some(summary) = optional_str(&p.description) {
        index["summary"] = json!(summary);
    }
    index
}

/// Prism `mmc-pack.json` (`MultiMC` formatVersion 1).
pub fn mmc_pack_json(mc: &str, loader: &str, loader_ver: &str) -> Value {
    let (uid, cached_name) = loader_component(loader);
    json!({
        "formatVersion": 1,
        "components": [
            { "cachedName": "Minecraft", "important": true, "uid": "net.minecraft", "version": mc },
            { "cachedName": cached_name, "uid": uid, "version": loader_ver },
        ],
    })
}

/// Prism `instance.cfg`.
pub fn instance_cfg(name: &str) -> String {
    // Newlines would break the INI format.
    let name = name.replace(['\n', '\r'], " ");
    format!("InstanceType=OneSix\niconKey=default\nname={name}\n")
}

/// Which platform's data to use for a self-contained (prism) export.
enum Pick<'a> {
    Modrinth {
        url: &'a str,
        filename: &'a str,
        slug: &'a str,
        sha1: &'a str,
        mod_id: &'a str,
        version_id: &'a str,
    },
    CurseForge {
        url: String,
        filename: &'a str,
        sha1: &'a str,
        file_id: i64,
        project_id: i64,
    },
}

fn pick_platform(e: &LockEntry) -> Option<Pick<'_>> {
    if let Some(mr) = &e.modrinth
        && let Some(url) = &mr.url
        && let Some(h) = &mr.hashes
        && let Some(sha1) = &h.sha1
    {
        return Some(Pick::Modrinth {
            url,
            filename: &mr.filename,
            slug: &mr.slug,
            sha1,
            mod_id: &mr.project_id,
            version_id: &mr.version_id,
        });
    }
    if let Some(cf) = &e.curseforge
        && let Some(h) = &cf.hashes
        && let Some(sha1) = &h.sha1
    {
        return Some(Pick::CurseForge {
            url: cf_url(cf),
            filename: &cf.filename,
            sha1,
            file_id: cf.file_id,
            project_id: cf.project_id,
        });
    }
    None
}

/// `CurseForge` download URL — the CDN path is deterministic from the file id
/// and name when the API returned null (same trick as the lockfile).
fn cf_url(cf: &LockCurseForge) -> String {
    cf.url.clone().unwrap_or_else(|| {
        format!(
            "https://edge.forgecdn.net/files/{}/{}/{}",
            cf.file_id / 1000,
            cf.file_id % 1000,
            cf.filename
        )
    })
}

/// Prism mod metadata (packwiz V1 `.pw.toml`) for a mod. `update` is one of
/// `{ "modrinth": { "mod-id", "version" } }` or
/// `{ "curseforge": { "file-id", "project-id" } }`.
pub fn mod_toml(
    name: &str,
    filename: &str,
    url: &str,
    sha1: &str,
    update: &Value,
    side: &str,
) -> String {
    let mut o = String::new();
    writeln!(o, "name = {}", tstr(name)).unwrap();
    writeln!(o, "filename = {}", tstr(filename)).unwrap();
    writeln!(o, "side = {}", tstr(side)).unwrap();
    o.push_str("\n[download]\n");
    writeln!(o, "url = {}", tstr(url)).unwrap();
    o.push_str("hash-format = \"sha1\"\n");
    writeln!(o, "hash = {}", tstr(sha1)).unwrap();
    let Some(providers) = update.as_object() else {
        return o;
    };
    o.push_str("\n[update]\n");
    for (provider, fields) in providers {
        writeln!(o, "[update.{provider}]").unwrap();
        let Some(fields) = fields.as_object() else {
            continue;
        };
        for (k, v) in fields {
            match v {
                Value::String(s) => writeln!(o, "{k} = {}", tstr(s)).unwrap(),
                Value::Number(n) => writeln!(o, "{k} = {n}").unwrap(),
                _ => {}
            }
        }
    }
    o
}

fn update_block(pick: &Pick<'_>) -> Value {
    match pick {
        Pick::Modrinth {
            mod_id, version_id, ..
        } => json!({ "modrinth": { "mod-id": mod_id, "version": version_id } }),
        Pick::CurseForge {
            file_id,
            project_id,
            ..
        } => json!({ "curseforge": { "file-id": file_id, "project-id": project_id } }),
    }
}

/// (client, server) mrpack env for a category. Files that make no sense on
/// the other side (shaders/resourcepacks on a dedicated server) are marked
/// `unsupported` so launchers never install them there; datapacks stay
/// `optional` client-side because they apply to singleplayer worlds.
fn env_for(cat: &str) -> (&'static str, &'static str) {
    match cat {
        "resourcepack" | "shader" => ("required", "unsupported"),
        "datapack" => ("optional", "required"),
        _ => ("required", "required"),
    }
}

// ── Zip assembly ───────────────────────────────────────────────────────

fn zip_bytes(entries: &[(String, Vec<u8>)]) -> Result<Vec<u8>> {
    let mut out = std::io::Cursor::new(Vec::new());
    {
        let mut w = ZipWriter::new(&mut out);
        let opts =
            SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
        for (name, bytes) in entries {
            w.start_file(name, opts)?;
            w.write_all(bytes)?;
        }
        w.finish()?;
    }
    Ok(out.into_inner())
}

struct ZipOut {
    bytes: Vec<u8>,
    written: usize,
    skipped: usize,
    warnings: Vec<String>,
}

/// Guard: an export that produced nothing is an error, not a silent empty zip.
/// The first warnings are included so the TUI/CLI error explains why.
fn require_written(out: &ZipOut, target: &str) -> Result<()> {
    if out.written == 0 {
        let mut msg = format!(
            "export: {target} — nothing to export (all {} entries skipped)",
            out.skipped
        );
        for w in out.warnings.iter().take(3) {
            msg.push_str("\n  ");
            msg.push_str(w);
        }
        bail!("{msg}");
    }
    Ok(())
}

async fn download(http: &reqwest::Client, url: &str, what: &str) -> Result<Vec<u8>> {
    let resp = http
        .get(url)
        .send()
        .await
        .map_err(|e| color_eyre::eyre::eyre!("{what}: {e}"))?;
    if !resp.status().is_success() {
        bail!("{what}: HTTP {}", resp.status());
    }
    Ok(resp.bytes().await?.to_vec())
}

/// Pakku-style `exportAsOverride`: a project that can't be represented on
/// the target platform (no pin for it there) is shipped as a plain file
/// under `overrides/<folder>/` so it still installs. Returns true on success.
async fn stage_override(
    entries: &mut Vec<(String, Vec<u8>)>,
    warnings: &mut Vec<String>,
    http: &reqwest::Client,
    what: &str,
    folder: &str,
    filename: &str,
    url: &str,
) -> bool {
    match download(http, url, what).await {
        Ok(bytes) => {
            entries.push((format!("overrides/{folder}/{filename}"), bytes));
            true
        }
        Err(err) => {
            warnings.push(format!(
                "{what} ({filename}): override download failed, skipped: {err}"
            ));
            false
        }
    }
}

// ── Per-target exporters ───────────────────────────────────────────────

/// `CurseForge`: mods become `files[]` (projectID+fileID); anything else
/// (resourcepacks / datapacks / shaders, or mods without a CF pin) is
/// downloaded into `overrides/<folder>/` — CF packs can't reference
/// non-CF projects, so the jar ships as a plain file instead.
async fn export_curseforge(
    pack: &Manifest,
    lock: &LockPack,
    loader: &str,
    loader_ver: &str,
    side: ExportSide,
    http: &reqwest::Client,
) -> Result<ZipOut> {
    let mut entries: Vec<(String, Vec<u8>)> = Vec::new();
    let mut files: Vec<(i64, i64)> = Vec::new();
    let mut warnings = Vec::new();
    let (mut written, mut skipped) = (0, 0);
    for (cat, list, folder) in categories(lock) {
        for e in list {
            if !include(side_of(pack, cat, &e.id), side) {
                skipped += 1;
                continue;
            }
            if let Some(cf) = &e.curseforge {
                if cat == "mod" {
                    files.push((cf.project_id, cf.file_id));
                    written += 1;
                } else {
                    let url = cf_url(cf);
                    if stage_override(
                        &mut entries,
                        &mut warnings,
                        http,
                        &e.name,
                        folder,
                        &cf.filename,
                        &url,
                    )
                    .await
                    {
                        written += 1;
                    } else {
                        skipped += 1;
                    }
                }
            } else if let Some(mr) = &e.modrinth {
                // Modrinth-scoped mod: no CF pin, so bundle the file as an
                // override (Pakku's exportAsOverride).
                if let Some(url) = &mr.url {
                    if stage_override(
                        &mut entries,
                        &mut warnings,
                        http,
                        &e.name,
                        folder,
                        &mr.filename,
                        url,
                    )
                    .await
                    {
                        written += 1;
                    } else {
                        skipped += 1;
                    }
                } else {
                    warnings.push(format!(
                        "{cat}/{}: no curseforge pin and no download url",
                        e.id
                    ));
                    skipped += 1;
                }
            } else {
                warnings.push(format!("{cat}/{}: no pin on any platform", e.id));
                skipped += 1;
            }
        }
    }
    // A manifest that pins curseforge must end up with `files[]` entries; an
    // empty list while the lockfile has no CF pins at all means the lock
    // resolution failed (bad/expired key, 403s) and this export would
    // silently degrade to a crippled override-only pack that can't be
    // published on CurseForge.
    if files.is_empty()
        && !lock.mod_.iter().any(|e| e.curseforge.is_some())
        && pack
            .mods
            .values()
            .any(super::project::ModSpec::has_curseforge)
    {
        bail!(
            "curseforge export: none of the manifest's curseforge-pinned mods resolved \
             — regenerate Easypacker.lock with a working CurseForge API key (see the warnings above)"
        );
    }
    let manifest = cf_manifest_json(pack, loader, loader_ver, &files);
    entries.insert(
        0,
        (
            "manifest.json".into(),
            serde_json::to_vec_pretty(&manifest)?,
        ),
    );
    let out = ZipOut {
        bytes: zip_bytes(&entries)?,
        written,
        skipped,
        warnings,
    };
    require_written(&out, "curseforge")?;
    Ok(out)
}

/// Modrinth: mods with a modrinth pin become `files[]` entries (the launcher
/// downloads them by URL+hash); everything else (e.g. CF-only mods) is
/// downloaded into `overrides/<folder>/` so it still installs.
async fn export_modrinth(
    pack: &Manifest,
    lock: &LockPack,
    loader: &str,
    loader_ver: &str,
    side: ExportSide,
    http: &reqwest::Client,
) -> Result<ZipOut> {
    let mut files: Vec<MrFile> = Vec::new();
    let mut entries: Vec<(String, Vec<u8>)> = Vec::new();
    let mut warnings = Vec::new();
    let (mut written, mut skipped) = (0, 0);
    for (cat, list, folder) in categories(lock) {
        for e in list {
            if !include(side_of(pack, cat, &e.id), side) {
                skipped += 1;
                continue;
            }
            if let Some(mr) = &e.modrinth {
                let (Some(sha1), Some(sha512)) = (
                    mr.hashes.as_ref().and_then(|h| h.sha1.clone()),
                    mr.hashes.as_ref().and_then(|h| h.sha512.clone()),
                ) else {
                    warnings.push(format!("{}/{}: missing sha1/sha512", cat, e.id));
                    skipped += 1;
                    continue;
                };
                let Some(url) = &mr.url else {
                    warnings.push(format!("{}/{}: no download url", cat, e.id));
                    skipped += 1;
                    continue;
                };
                let (client, server) = if cat == "mod" {
                    mod_env(side_of(pack, cat, &e.id))
                } else {
                    env_for(cat)
                };
                files.push(MrFile {
                    path: format!("{folder}/{}", mr.filename),
                    sha1,
                    sha512,
                    url: url.clone(),
                    size: mr.size,
                    client,
                    server,
                });
                written += 1;
            } else if let Some(cf) = &e.curseforge {
                // CF-only project: mrpack can't reference it, bundle instead.
                let url = cf_url(cf);
                if stage_override(
                    &mut entries,
                    &mut warnings,
                    http,
                    &e.name,
                    folder,
                    &cf.filename,
                    &url,
                )
                .await
                {
                    written += 1;
                } else {
                    skipped += 1;
                }
            } else {
                warnings.push(format!("{cat}/{}: no pin on any platform", e.id));
                skipped += 1;
            }
        }
    }
    let index = mr_index_json(pack, loader, loader_ver, &files);
    entries.insert(
        0,
        (
            "modrinth.index.json".into(),
            serde_json::to_vec_pretty(&index)?,
        ),
    );
    let out = ZipOut {
        bytes: zip_bytes(&entries)?,
        written,
        skipped,
        warnings,
    };
    require_written(&out, "modrinth")?;
    Ok(out)
}

/// Prism Launcher: self-contained — every file is downloaded into
/// `minecraft/<folder>/`, with packwiz `.pw.toml` metadata for mods.
async fn export_prism(
    pack: &Manifest,
    lock: &LockPack,
    loader: &str,
    loader_ver: &str,
    side: ExportSide,
    http: &reqwest::Client,
) -> Result<ZipOut> {
    let mut entries: Vec<(String, Vec<u8>)> = vec![
        (
            "mmc-pack.json".into(),
            serde_json::to_vec_pretty(&mmc_pack_json(&lock.minecraft, loader, loader_ver))?,
        ),
        (
            "instance.cfg".into(),
            instance_cfg(&pack.project.name).into_bytes(),
        ),
    ];
    let mut warnings = Vec::new();
    let (mut written, mut skipped) = (0, 0);
    for (cat, list, folder) in categories(lock) {
        for e in list {
            if !include(side_of(pack, cat, &e.id), side) {
                skipped += 1;
                continue;
            }
            let Some(chosen) = pick_platform(e) else {
                warnings.push(format!("{cat}/{}: no url+sha1 pin on any platform", e.id));
                skipped += 1;
                continue;
            };
            let (url, filename) = match &chosen {
                Pick::Modrinth { url, filename, .. } => ((*url).to_owned(), *filename),
                Pick::CurseForge { url, filename, .. } => (url.clone(), *filename),
            };
            match download(http, &url, &e.name).await {
                Ok(bytes) => {
                    entries.push((format!("minecraft/{folder}/{filename}"), bytes));
                    if cat == "mod" {
                        let slug = match &chosen {
                            Pick::Modrinth { slug, .. } => (*slug).to_owned(),
                            Pick::CurseForge { .. } => e.id.clone(),
                        };
                        let toml = mod_toml(
                            &e.name,
                            filename,
                            &url,
                            match &chosen {
                                Pick::Modrinth { sha1, .. } | Pick::CurseForge { sha1, .. } => sha1,
                            },
                            &update_block(&chosen),
                            side_of(pack, cat, &e.id).as_str(),
                        );
                        entries.push((
                            format!("minecraft/{folder}/{slug}.pw.toml"),
                            toml.into_bytes(),
                        ));
                    }
                    written += 1;
                }
                Err(err) => {
                    warnings.push(format!(
                        "{} ({}): download failed, skipped: {err}",
                        e.name, filename
                    ));
                    skipped += 1;
                }
            }
        }
    }
    let out = ZipOut {
        bytes: zip_bytes(&entries)?,
        written,
        skipped,
        warnings,
    };
    require_written(&out, "prism")?;
    Ok(out)
}

// ── Entry points ───────────────────────────────────────────────────────

fn write_pack(dir: &Path, pack: &Manifest, target: ExportTarget, bytes: &[u8]) -> Result<PathBuf> {
    let proj = &pack.project;
    let file = dir.join(format!(
        "{}-{}-{}.{}",
        slugify(&proj.name),
        slugify(&proj.version),
        target.label().to_lowercase().replace(' ', "-"),
        target.ext()
    ));
    std::fs::write(&file, bytes)?;
    Ok(file)
}

/// Export the pack in `dir` (Easypacker.toml + Easypacker.lock) to `target`.
/// `side` picks the client export (everything ships) or the server export
/// (client-only mods and client-only categories are dropped).
pub async fn export_pack(
    dir: &Path,
    target: ExportTarget,
    side: ExportSide,
) -> Result<ExportReport> {
    let pack = Manifest::load(dir).map_err(|e| {
        color_eyre::eyre::eyre!("export: {e} (no Easypacker.toml in {})", dir.display())
    })?;
    let lock = LockPack::load(dir)?;
    // Heads-up when the pack doesn't target the requested platform: entries
    // without a pin for it are bundled as overrides instead of listed.
    let mut warnings = Vec::new();
    if !lock.targets.iter().any(|t| t == target.key()) {
        warnings.push(format!(
            "pack targets [{}], not {} — entries without a {} pin ship as overrides",
            lock.targets.join(", "),
            target.label(),
            target.key()
        ));
    }
    let loader = lock.loader.to_lowercase();
    let loader_ver = loader_version(&lock.minecraft, &loader).await?;
    let http = reqwest::Client::new();
    let out = match target {
        ExportTarget::CurseForge => {
            export_curseforge(&pack, &lock, &loader, &loader_ver, side, &http).await?
        }
        ExportTarget::Modrinth => {
            export_modrinth(&pack, &lock, &loader, &loader_ver, side, &http).await?
        }
        ExportTarget::Prism => {
            export_prism(&pack, &lock, &loader, &loader_ver, side, &http).await?
        }
    };
    warnings.extend(out.warnings);
    let path = write_pack(dir, &pack, target, &out.bytes)?;
    Ok(ExportReport {
        path,
        written: out.written,
        skipped: out.skipped,
        warnings,
    })
}

pub async fn run_cli(args: &Cli) -> Result<()> {
    let target = ExportTarget::parse(args.export.as_deref().unwrap_or("curseforge"))?;
    let side = if args.server {
        ExportSide::Server
    } else {
        ExportSide::Client
    };
    let dir = args.output.as_ref().map_or_else(
        || std::env::current_dir().unwrap_or_default(),
        PathBuf::from,
    );
    let report = export_pack(&dir, target, side).await?;
    for w in &report.warnings {
        eprintln!("easypacker: export: {w}");
    }
    println!(
        "Exported {} to {} — {} file(s), {} skipped",
        target.label(),
        report.path.display(),
        report.written,
        report.skipped
    );
    Ok(())
}

#[cfg(test)]
#[path = "../tests/export_tests.rs"]
mod tests;
