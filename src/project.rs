use color_eyre::eyre::Result;
use serde::Deserialize;
use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::ops::{Deref, DerefMut};
use std::path::Path;

pub const MANIFEST_FILE: &str = "Easypacker.toml";

/// Easypacker.toml — the hand-editable manifest (cargo-style).
/// Parsing is serde; writing is a custom renderer so entries stay
/// single-line inline tables (`sodium = { modrinth = {...}, ... }`).
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

/// One manifest entry. Either `sodium = "Version display name"` (the key is
/// the platform slug, same version everywhere) or a detailed inline table.
#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum ModSpec {
    Simple(String),
    Detailed(DetailedSpec),
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct DetailedSpec {
    /// Shared version display name — inherited by every platform below that
    /// doesn't override it with its own `version`.
    #[serde(default)]
    pub version: Option<String>,
    #[serde(default)]
    pub modrinth: Option<ModrinthSpec>,
    #[serde(default)]
    pub curseforge: Option<CurseForgeSpec>,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct ModrinthSpec {
    #[serde(default)]
    pub slug: Option<String>,
    #[serde(default)]
    pub version: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct CurseForgeSpec {
    #[serde(default, rename = "projectId")]
    pub project_id: Option<i64>,
    #[serde(default)]
    pub version: Option<String>,
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

    /// Render back to TOML. Detailed entries become single-line inline
    /// tables; simple string entries stay plain strings.
    pub fn render(&self) -> String {
        let mut o = String::from("[project]\n");
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
                    writeln!(o, "{} = {}", tkey(k), render_spec(d)).unwrap();
                }
            }
        }
        o
    }
}

fn render_spec(d: &DetailedSpec) -> String {
    let mut parts = Vec::new();
    if let Some(mr) = &d.modrinth {
        let mut inner = Vec::new();
        if let Some(s) = &mr.slug {
            inner.push(format!("slug = {}", tstr(s)));
        }
        if let Some(v) = &mr.version {
            inner.push(format!("version = {}", tstr(v)));
        }
        parts.push(format!("modrinth = {{ {} }}", inner.join(", ")));
    }
    if let Some(cf) = &d.curseforge {
        let mut inner = Vec::new();
        if let Some(id) = cf.project_id {
            inner.push(format!("projectId = {id}"));
        }
        if let Some(v) = &cf.version {
            inner.push(format!("version = {}", tstr(v)));
        }
        parts.push(format!("curseforge = {{ {} }}", inner.join(", ")));
    }
    if let Some(v) = &d.version {
        parts.push(format!("version = {}", tstr(v)));
    }
    format!("{{ {} }}", parts.join(", "))
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
