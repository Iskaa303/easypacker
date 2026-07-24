use color_eyre::eyre::{Result, bail};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

const CONFIG_FILE: &str = ".easypacker.json";

#[derive(Serialize, Deserialize, Default)]
pub struct Config {
    #[serde(rename = "curseforge_api_key")]
    pub curseforge_api_key: Option<String>,
}

impl Config {
    pub fn path() -> PathBuf {
        dirs::home_dir().unwrap_or_default().join(CONFIG_FILE)
    }

    pub fn load() -> Result<Self> {
        let p = Self::path();
        if p.exists() {
            let raw = std::fs::read_to_string(&p)?;
            Ok(serde_json::from_str(&raw)?)
        } else {
            Ok(Self::default())
        }
    }

    #[allow(dead_code)]
    pub fn save(&self) -> Result<()> {
        let p = Self::path();
        let raw = serde_json::to_string_pretty(self)?;
        std::fs::write(&p, raw)?;
        Ok(())
    }

    pub fn get_api_key(&self, cli_override: Option<&str>) -> Result<String> {
        if let Some(k) = cli_override {
            return Ok(k.to_owned());
        }
        if let Ok(k) = std::env::var("CURSEFORGE_API_KEY") {
            return Ok(k);
        }
        if let Some(k) = &self.curseforge_api_key {
            return Ok(k.clone());
        }
        bail!(
            "CurseForge API key not found. Set it via:\n  \
             --api-key <key>\n  \
             CURSEFORGE_API_KEY env var\n  \
             or ~/.easypacker.json: {{\"curseforge_api_key\": \"...\"}}"
        );
    }
}
