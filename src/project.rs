use serde::{Deserialize, Serialize};
use std::path::Path;

pub const MODPACK_FILE: &str = "modpack.json";

#[derive(Debug, Clone, Serialize, Deserialize)]
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
    #[serde(default)]
    pub links: ProjectLinks,
    pub minecraft: String,
    pub loader: String,
    #[serde(default)]
    pub platforms: Vec<String>,
}

fn default_version() -> String {
    "0.0.1".into()
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ProjectLinks {
    #[serde(default)]
    pub website: Option<String>,
    #[serde(default)]
    pub discord: Option<String>,
    #[serde(default)]
    pub github: Option<String>,
}

impl ModpackProject {
    pub fn detect(dir: &Path) -> Option<Self> {
        let path = dir.join(MODPACK_FILE);
        if path.exists() {
            std::fs::read_to_string(&path)
                .ok()
                .and_then(|s| serde_json::from_str(&s).ok())
        } else {
            None
        }
    }

    pub fn save(&self, dir: &Path) -> Result<(), Box<dyn std::error::Error>> {
        let path = dir.join(MODPACK_FILE);
        let json = serde_json::to_string_pretty(self)?;
        std::fs::write(path, json)?;
        Ok(())
    }

    pub fn init_project(dir: &Path, project: &Self) -> Result<(), Box<dyn std::error::Error>> {
        std::fs::create_dir_all(dir)?;
        project.save(dir)?;
        Ok(())
    }
}

pub const PROJECTS_FILE: &str = "projects.json";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModrinthMeta {
    pub slug: String,
    pub version_id: String,
    pub url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CurseForgeMeta {
    pub mod_id: i32,
    pub file_id: i32,
    pub url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectEntry {
    pub name: String,
    pub project_type: String,
    pub loaders: Vec<String>,
    pub mc_versions: Vec<String>,
    pub version_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub modrinth: Option<ModrinthMeta>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub curseforge: Option<CurseForgeMeta>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ProjectsFile {
    #[serde(rename = "mod")]
    pub mod_: Vec<ProjectEntry>,
    pub resourcepack: Vec<ProjectEntry>,
    pub datapack: Vec<ProjectEntry>,
    pub shader: Vec<ProjectEntry>,
}

impl ProjectsFile {
    pub fn load(dir: &Path) -> Self {
        let path = dir.join(PROJECTS_FILE);
        if path.exists() {
            std::fs::read_to_string(&path)
                .ok()
                .and_then(|s| serde_json::from_str(&s).ok())
                .unwrap_or_default()
        } else {
            Self::default()
        }
    }

    pub fn save(&self, dir: &Path) -> Result<(), Box<dyn std::error::Error>> {
        let path = dir.join(PROJECTS_FILE);
        let json = serde_json::to_string_pretty(self)?;
        std::fs::write(path, json)?;
        Ok(())
    }

    fn cat(&self, project_type: &str) -> &[ProjectEntry] {
        match project_type {
            "resourcepack" => &self.resourcepack,
            "datapack" => &self.datapack,
            "shader" => &self.shader,
            _ => &self.mod_,
        }
    }

    fn cat_mut(&mut self, project_type: &str) -> &mut Vec<ProjectEntry> {
        match project_type {
            "resourcepack" => &mut self.resourcepack,
            "datapack" => &mut self.datapack,
            "shader" => &mut self.shader,
            _ => &mut self.mod_,
        }
    }

    pub fn contains(
        &self,
        project_type: &str,
        modrinth_slug: Option<&str>,
        curseforge_id: Option<i32>,
    ) -> bool {
        self.cat(project_type).iter().any(|e| {
            if let Some(s) = modrinth_slug {
                if e.modrinth.as_ref().map(|m| m.slug.as_str()) == Some(s) {
                    return true;
                }
            }
            if let Some(id) = curseforge_id {
                if e.curseforge.as_ref().map(|c| c.mod_id) == Some(id) {
                    return true;
                }
            }
            false
        })
    }

    pub fn add(&mut self, entry: ProjectEntry) {
        let cat = self.cat_mut(&entry.project_type);
        if let Some(existing) = cat.iter_mut().find(|e| {
            if let Some(ref m) = entry.modrinth {
                if e.modrinth.as_ref().map(|em| em.slug.as_str()) == Some(m.slug.as_str()) {
                    return true;
                }
            }
            if let Some(ref c) = entry.curseforge {
                if e.curseforge.as_ref().map(|ec| ec.mod_id) == Some(c.mod_id) {
                    return true;
                }
            }
            false
        }) {
            *existing = entry;
        } else {
            cat.push(entry);
        }
    }
}
