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
