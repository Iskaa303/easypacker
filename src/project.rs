use color_eyre::eyre::Result;
use serde::Deserialize;
use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::ops::{Deref, DerefMut};
use std::path::Path;

pub const MANIFEST_FILE: &str = "Easypacker.toml";

/// Version string meaning "newest version matching the pack's minecraft /
/// loader" — what an entry without a `version` resolves to.
pub const LATEST: &str = "latest";

/// Is this the `latest` marker (case-insensitive)?
pub const fn is_latest(v: &str) -> bool {
    v.eq_ignore_ascii_case(LATEST)
}

/// Easypacker.toml — the hand-editable manifest (cargo-style). Parsing is
/// serde (with short forms: `slugs = "1.2.3"`, `modrinth = true`,
/// `curseforge = 394468`); writing is a custom renderer that picks the
/// shortest equivalent form for every entry.
#[derive(Debug, Clone, Deserialize)]
pub struct Manifest {
    pub project: ModpackProject,
    #[serde(default)]
    pub mods: CategoryMap,
    #[serde(default)]
    pub resourcepacks: CategoryMap,
    #[serde(default)]
    pub datapacks: CategoryMap,
    #[serde(default)]
    pub shaders: CategoryMap,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ModpackProject {
    pub name: String,
    #[serde(default = "default_version")]
    pub version: String,
    #[serde(default)]
    pub authors: String,
    #[serde(default)]
    pub credits: String,
    #[serde(default)]
    pub description: String,
    pub minecraft: String,
    pub loader: String,
    #[serde(default)]
    pub platforms: Vec<String>,
    #[serde(default)]
    pub links: ProjectLinks,
}

fn default_version() -> String {
    "0.0.1".into()
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct ProjectLinks {
    #[serde(default)]
    pub website: Option<String>,
    #[serde(default)]
    pub discord: Option<String>,
    #[serde(default)]
    pub github: Option<String>,
}

/// Map of project id -> spec.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(transparent)]
pub struct CategoryMap(BTreeMap<String, ModSpec>);

impl Deref for CategoryMap {
    type Target = BTreeMap<String, ModSpec>;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for CategoryMap {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

/// Which side a project runs on. `Both` is the default for mods;
/// resourcepacks/shaders default to `Client`, datapacks to `Server`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Side {
    Client,
    Server,
    Both,
}

impl Side {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Client => "client",
            Self::Server => "server",
            Self::Both => "both",
        }
    }
}

/// One manifest entry. Either `sodium = "Version display name"` (the key is
/// the platform slug, same version everywhere) or a detailed inline table.
#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum ModSpec {
    Simple(String),
    Detailed(DetailedSpec),
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct DetailedSpec {
    /// Shared version display name — inherited by every platform below that
    /// doesn't override it with its own `version`. `"latest"` (or omitting
    /// it) means the newest matching version.
    #[serde(default)]
    pub version: Option<String>,
    #[serde(default)]
    pub modrinth: Option<ModrinthSpec>,
    #[serde(default)]
    pub curseforge: Option<CurseForgeSpec>,
    /// Force the side this project runs on (client / server). Unset =
    /// platform default (`both` for mods).
    #[serde(default)]
    pub side: Option<Side>,
}

/// Modrinth identity, written in whichever form is shortest:
/// `modrinth = true` (slug is the key), `modrinth = "some-slug"`, or
/// `modrinth = { slug = "some-slug", version = "1.2.3" }` when the version
/// differs from the shared one.
#[derive(Debug, Clone, Default)]
pub struct ModrinthSpec {
    pub slug: Option<String>,
    pub version: Option<String>,
}

/// `CurseForge` identity: `curseforge = true` (slug is the key),
/// `curseforge = 394468` (project id), `curseforge = "some-slug"`, or a full
/// `{ projectId = ..., slug = ..., version = ... }` table.
#[derive(Debug, Clone, Default)]
pub struct CurseForgeSpec {
    pub project_id: Option<i64>,
    /// `CurseForge` slug when it differs from the manifest key.
    pub slug: Option<String>,
    pub version: Option<String>,
}

impl<'de> Deserialize<'de> for ModrinthSpec {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum Repr {
            Flag(bool),
            Slug(String),
            Table {
                slug: Option<String>,
                version: Option<String>,
            },
        }
        Ok(match Repr::deserialize(d)? {
            Repr::Flag(true) => Self::default(),
            // `false` would be a second way to say "no modrinth"; leaving the
            // key out already says that, so point at the one true spelling.
            Repr::Flag(false) => {
                return Err(serde::de::Error::custom(
                    "modrinth = false is not a thing — remove the key to drop the platform",
                ));
            }
            Repr::Slug(slug) => Self {
                slug: Some(slug),
                version: None,
            },
            Repr::Table { slug, version } => Self { slug, version },
        })
    }
}

impl<'de> Deserialize<'de> for CurseForgeSpec {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum Repr {
            Flag(bool),
            Id(i64),
            Slug(String),
            Table {
                #[serde(rename = "projectId")]
                project_id: Option<i64>,
                slug: Option<String>,
                version: Option<String>,
            },
        }
        Ok(match Repr::deserialize(d)? {
            Repr::Flag(true) => Self::default(),
            Repr::Flag(false) => {
                return Err(serde::de::Error::custom(
                    "curseforge = false is not a thing — remove the key to drop the platform",
                ));
            }
            Repr::Id(project_id) => Self {
                project_id: Some(project_id),
                slug: None,
                version: None,
            },
            Repr::Slug(slug) => Self {
                project_id: None,
                slug: Some(slug),
                version: None,
            },
            Repr::Table {
                project_id,
                slug,
                version,
            } => Self {
                project_id,
                slug,
                version,
            },
        })
    }
}

impl ModSpec {
    /// Version requested for a platform: platform-specific override wins,
    /// then the shared one.
    pub fn version_for(&self, modrinth: bool) -> Option<String> {
        match self {
            Self::Simple(v) => Some(v.clone()),
            Self::Detailed(d) => {
                let specific = if modrinth {
                    d.modrinth.as_ref().and_then(|m| m.version.clone())
                } else {
                    d.curseforge.as_ref().and_then(|c| c.version.clone())
                };
                specific.or_else(|| d.version.clone())
            }
        }
    }

    pub fn shared_version(&self) -> Option<String> {
        match self {
            Self::Simple(v) => Some(v.clone()),
            Self::Detailed(d) => d.version.clone(),
        }
    }

    pub fn modrinth_slug(&self, key: &str) -> String {
        match self {
            Self::Simple(_) => key.to_owned(),
            Self::Detailed(d) => d
                .modrinth
                .as_ref()
                .and_then(|m| m.slug.clone())
                .unwrap_or_else(|| key.to_owned()),
        }
    }

    pub fn curseforge_project_id(&self) -> Option<i64> {
        match self {
            Self::Simple(_) => None,
            Self::Detailed(d) => d.curseforge.as_ref().and_then(|c| c.project_id),
        }
    }

    /// `CurseForge` slug to look the project up by: the explicit slug when
    /// given, otherwise the manifest key. `None` when the spec doesn't target
    /// `CurseForge` at all.
    pub fn curseforge_slug(&self, key: &str) -> Option<String> {
        let cf = match self {
            Self::Simple(_) => return Some(key.to_owned()),
            Self::Detailed(d) => d.curseforge.as_ref()?,
        };
        Some(cf.slug.clone().unwrap_or_else(|| key.to_owned()))
    }

    /// Which platforms an entry is on: the string form and a table that names
    /// no platform at all target every platform the pack targets; a table
    /// that names platforms targets exactly those (so `{ curseforge = 123 }`
    /// is `CurseForge`-only).
    pub const fn has_modrinth(&self) -> bool {
        match self {
            Self::Simple(_) => true,
            Self::Detailed(d) => d.modrinth.is_some() || d.curseforge.is_none(),
        }
    }

    pub const fn has_curseforge(&self) -> bool {
        match self {
            Self::Simple(_) => true,
            Self::Detailed(d) => d.curseforge.is_some() || d.modrinth.is_none(),
        }
    }

    /// The side the player forced for this entry, if any.
    pub const fn side(&self) -> Option<Side> {
        match self {
            Self::Simple(_) => None,
            Self::Detailed(d) => d.side,
        }
    }
}

pub const CATEGORIES: [(&str, i32); 4] = [
    ("mod", 6),
    ("resourcepack", 12),
    ("datapack", 6945),
    ("shader", 6552),
];

impl Manifest {
    pub fn new(project: ModpackProject) -> Self {
        Self {
            project,
            mods: CategoryMap::default(),
            resourcepacks: CategoryMap::default(),
            datapacks: CategoryMap::default(),
            shaders: CategoryMap::default(),
        }
    }

    pub fn detect(dir: &Path) -> Option<Self> {
        Self::load(dir).ok()
    }

    pub fn load(dir: &Path) -> Result<Self> {
        let raw = std::fs::read_to_string(dir.join(MANIFEST_FILE))?;
        Ok(toml::from_str(&raw)?)
    }

    pub fn save(&self, dir: &Path) -> Result<()> {
        std::fs::write(dir.join(MANIFEST_FILE), self.render())?;
        Ok(())
    }

    pub fn init_project(dir: &Path, project: ModpackProject) -> Result<Self> {
        std::fs::create_dir_all(dir)?;
        let manifest = Self::new(project);
        manifest.save(dir)?;
        Ok(manifest)
    }

    pub fn cat(&self, project_type: &str) -> &CategoryMap {
        match project_type {
            "resourcepack" => &self.resourcepacks,
            "datapack" => &self.datapacks,
            "shader" => &self.shaders,
            _ => &self.mods,
        }
    }

    pub fn cat_mut(&mut self, project_type: &str) -> &mut CategoryMap {
        match project_type {
            "resourcepack" => &mut self.resourcepacks,
            "datapack" => &mut self.datapacks,
            "shader" => &mut self.shaders,
            _ => &mut self.mods,
        }
    }

    pub fn contains(
        &self,
        project_type: &str,
        modrinth_slug: Option<&str>,
        curseforge_id: Option<i64>,
    ) -> bool {
        self.cat(project_type).iter().any(|(key, spec)| {
            if let Some(s) = modrinth_slug
                && spec.modrinth_slug(key) == s
            {
                return true;
            }
            if let Some(id) = curseforge_id
                && spec.curseforge_project_id() == Some(id)
            {
                return true;
            }
            false
        })
    }

    /// Return the version string for a mod identified by its platform slugs/ids.
    pub fn version_of(
        &self,
        project_type: &str,
        modrinth_slug: Option<&str>,
        curseforge_id: Option<i64>,
    ) -> Option<String> {
        self.cat(project_type).iter().find_map(|(key, spec)| {
            if let Some(s) = modrinth_slug
                && spec.modrinth_slug(key) == s
            {
                return spec.version_for(true);
            }
            if let Some(id) = curseforge_id
                && spec.curseforge_project_id() == Some(id)
            {
                return spec.version_for(false);
            }
            None
        })
    }

    /// Return the spec for a mod identified by its platform slugs/ids.
    pub fn spec_for(
        &self,
        project_type: &str,
        modrinth_slug: Option<&str>,
        curseforge_id: Option<i64>,
    ) -> Option<&ModSpec> {
        self.cat(project_type).iter().find_map(|(key, spec)| {
            if let Some(s) = modrinth_slug
                && spec.modrinth_slug(key) == s
            {
                return Some(spec);
            }
            if let Some(id) = curseforge_id
                && spec.curseforge_project_id() == Some(id)
            {
                return Some(spec);
            }
            None
        })
    }

    /// Return the manifest key for a mod identified by platform slugs/ids.
    pub fn key_for(
        &self,
        project_type: &str,
        modrinth_slug: Option<&str>,
        curseforge_id: Option<i64>,
    ) -> Option<String> {
        self.cat(project_type).iter().find_map(|(key, spec)| {
            if let Some(s) = modrinth_slug
                && spec.modrinth_slug(key) == s
            {
                return Some(key.clone());
            }
            if let Some(id) = curseforge_id
                && spec.curseforge_project_id() == Some(id)
            {
                return Some(key.clone());
            }
            None
        })
    }

    /// Render back to TOML. Detailed entries become single-line inline
    /// tables; simple string entries stay plain strings.
    pub fn render(&self) -> String {
        let mut o = String::new();
        o.push_str("[project]\n");
        let p = &self.project;
        writeln!(o, "name = {}", tstr(&p.name)).unwrap();
        writeln!(o, "version = {}", tstr(&p.version)).unwrap();
        writeln!(o, "authors = {}", tstr(&p.authors)).unwrap();
        writeln!(o, "credits = {}", tstr(&p.credits)).unwrap();
        writeln!(o, "description = {}", tstr(&p.description)).unwrap();
        writeln!(o, "minecraft = {}", tstr(&p.minecraft)).unwrap();
        writeln!(o, "loader = {}", tstr(&p.loader)).unwrap();
        let platforms = p
            .platforms
            .iter()
            .map(|s| tstr(s))
            .collect::<Vec<_>>()
            .join(", ");
        writeln!(o, "platforms = [{platforms}]").unwrap();

        o.push_str("\n[project.links]\n");
        if let Some(w) = &p.links.website {
            writeln!(o, "website = {}", tstr(w)).unwrap();
        }
        if let Some(d) = &p.links.discord {
            writeln!(o, "discord = {}", tstr(d)).unwrap();
        }
        if let Some(g) = &p.links.github {
            writeln!(o, "github = {}", tstr(g)).unwrap();
        }

        for (header, map) in [
            ("mods", &self.mods),
            ("resourcepacks", &self.resourcepacks),
            ("datapacks", &self.datapacks),
            ("shaders", &self.shaders),
        ] {
            writeln!(o, "\n[{header}]").unwrap();
            for (k, v) in map.iter() {
                if let ModSpec::Simple(s) = v {
                    writeln!(o, "{} = {}", tkey(k), tstr(s)).unwrap();
                }
            }
            for (k, v) in map.iter() {
                if let ModSpec::Detailed(d) = v {
                    writeln!(o, "{} = {}", tkey(k), render_spec(k, d)).unwrap();
                }
            }
        }
        o
    }
}

/// Render one detailed spec in the shortest form that means the same thing:
/// `sodium = "0.8.13"` when both platforms use the key as their slug,
/// otherwise a table with only the keys that carry information —
/// `{ version = "0.8.13", modrinth = true, curseforge = 394468 }`,
/// `{ curseforge = 482378 }` for a `CurseForge`-only mod, or
/// `{ modrinth = "other-slug", version = "1.2.3" }`.
fn render_spec(key: &str, d: &DetailedSpec) -> String {
    let mr_ver = d.modrinth.as_ref().and_then(|m| m.version.as_deref());
    let cf_ver = d.curseforge.as_ref().and_then(|c| c.version.as_deref());
    let hoisted = match (d.version.as_deref(), mr_ver, cf_ver) {
        (Some(v), _, _) => Some(v),
        (None, Some(a), Some(b)) if a == b => Some(a),
        _ => None,
    };

    // Both platforms, default identities, no forced side: the plain string
    // form. Unpinned writes `"latest"` so the intent is visible. Naming no
    // platform at all also means "both", so `{ version = "1.0" }` normalizes
    // to `"1.0"` instead of staying a longer spelling of the same thing.
    let implied = d.modrinth.is_none() && d.curseforge.is_none();
    let mr_default = d
        .modrinth
        .as_ref()
        .is_none_or(|m| m.version.is_none() && m.slug.as_deref().is_none_or(|s| s == key));
    let cf_default = d.curseforge.as_ref().is_none_or(|c| {
        c.version.is_none() && c.project_id.is_none() && c.slug.as_deref().is_none_or(|s| s == key)
    });
    if (implied || (d.modrinth.is_some() && d.curseforge.is_some()))
        && mr_default
        && cf_default
        && d.side.is_none()
    {
        return tstr(hoisted.unwrap_or(LATEST));
    }

    let mut parts = Vec::new();
    if let Some(v) = hoisted {
        parts.push(format!("version = {}", tstr(v)));
    }
    if let Some(s) = d.side {
        parts.push(format!("side = {}", tstr(s.as_str())));
    }
    if let Some(mr) = &d.modrinth {
        parts.push(render_modrinth(key, mr, hoisted));
    }
    if let Some(cf) = &d.curseforge {
        parts.push(render_curseforge(key, cf, hoisted));
    }
    format!("{{ {} }}", parts.join(", "))
}

/// A platform's own version is only written when it differs from the shared
/// one (otherwise it inherits it).
fn platform_version<'a>(version: Option<&'a str>, hoisted: Option<&str>) -> Option<&'a str> {
    version.filter(|v| hoisted != Some(*v))
}

fn render_modrinth(key: &str, mr: &ModrinthSpec, hoisted: Option<&str>) -> String {
    let version = platform_version(mr.version.as_deref(), hoisted);
    let slug = mr.slug.as_deref().filter(|s| *s != key);
    match (slug, version) {
        (None, None) => "modrinth = true".to_owned(),
        (Some(s), None) => format!("modrinth = {}", tstr(s)),
        (slug, Some(v)) => {
            let mut inner = Vec::new();
            if let Some(s) = slug {
                inner.push(format!("slug = {}", tstr(s)));
            }
            inner.push(format!("version = {}", tstr(v)));
            format!("modrinth = {{ {} }}", inner.join(", "))
        }
    }
}

fn render_curseforge(key: &str, cf: &CurseForgeSpec, hoisted: Option<&str>) -> String {
    let version = platform_version(cf.version.as_deref(), hoisted);
    let slug = cf.slug.as_deref().filter(|s| *s != key);
    if version.is_none() {
        return match (cf.project_id, slug) {
            (Some(id), None) => format!("curseforge = {id}"),
            (None, Some(s)) => format!("curseforge = {}", tstr(s)),
            (None, None) => "curseforge = true".to_owned(),
            (Some(id), Some(s)) => {
                format!("curseforge = {{ projectId = {id}, slug = {} }}", tstr(s))
            }
        };
    }
    let mut inner = Vec::new();
    if let Some(id) = cf.project_id {
        inner.push(format!("projectId = {id}"));
    }
    if let Some(s) = slug {
        inner.push(format!("slug = {}", tstr(s)));
    }
    inner.push(format!("version = {}", tstr(version.unwrap_or_default())));
    format!("curseforge = {{ {} }}", inner.join(", "))
}

/// Quote a string as a TOML basic string.
pub fn tstr(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            c if (c as u32) < 0x20 => {
                write!(out, "\\u{:04X}", c as u32).unwrap();
            }
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// Bare key if safe ([A-Za-z0-9_-]+), quoted otherwise.
fn tkey(k: &str) -> String {
    if !k.is_empty()
        && k.chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
    {
        k.to_owned()
    } else {
        tstr(k)
    }
}

pub fn slugify(s: &str) -> String {
    s.chars()
        .map(|c| {
            if c.is_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect()
}

#[cfg(test)]
#[path = "../tests/project_tests.rs"]
mod tests;
