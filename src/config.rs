use color_eyre::eyre::{Result, bail};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

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

    /// Look up the `CurseForge` API key: `--api-key` > `CURSEFORGE_API_KEY`
    /// env var > `<project>/.env` > `~/.easypacker.json`. Env values that
    /// don't look like a key (`CurseForge` keys are bcrypt-style, starting
    /// with `$2`) are rejected — a shell that expands `$2a$…` corrupts the
    /// value, and silently sending garbage to the API is worse than
    /// falling through to the project's `.env`.
    pub fn get_api_key(
        &self,
        cli_override: Option<&str>,
        project_dir: Option<&Path>,
    ) -> Result<String> {
        if let Some(k) = cli_override {
            return Ok(k.to_owned());
        }
        if let Ok(k) = std::env::var("CURSEFORGE_API_KEY") {
            if plausible_key(&k) {
                return Ok(k);
            }
            eprintln!(
                "easypacker: ignoring malformed CURSEFORGE_API_KEY from the environment \
                 (CurseForge keys start with \"$2\"; quote the value in your shell config)"
            );
        }
        let env_file = project_dir.map_or_else(
            || std::env::current_dir().unwrap_or_default().join(".env"),
            |dir| dir.join(".env"),
        );
        if let Some(k) = env_key(&env_file) {
            return Ok(k);
        }
        if let Some(k) = &self.curseforge_api_key
            && !k.is_empty()
        {
            return Ok(k.clone());
        }
        bail!(
            "CurseForge API key not found. Set it via:\n  \
             --api-key <key>\n  \
             CURSEFORGE_API_KEY env var\n  \
             a .env file with CURSEFORGE_API_KEY=... in the project directory\n  \
             or ~/.easypacker.json: {{\"curseforge_api_key\": \"...\"}}"
        );
    }
}

/// `CurseForge` API keys are bcrypt-shaped (`$2a$10$…`). A value not starting
/// with `$2` is almost always a shell-mangled copy of a real key.
fn plausible_key(k: &str) -> bool {
    k.starts_with("$2")
}

/// `KEY=VALUE` (quotes stripped) for `CURSEFORGE_API_KEY` in a `.env` file.
fn env_key(path: &Path) -> Option<String> {
    let raw = std::fs::read_to_string(path).ok()?;
    raw.lines().find_map(|line| {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            return None;
        }
        let (key, value) = line.split_once('=')?;
        if key.trim() != "CURSEFORGE_API_KEY" {
            return None;
        }
        let value = value.trim();
        let value = value
            .strip_prefix('"')
            .and_then(|v| v.strip_suffix('"'))
            .or_else(|| value.strip_prefix('\'').and_then(|v| v.strip_suffix('\'')))
            .unwrap_or(value);
        (!value.is_empty()).then(|| value.to_owned())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plausible_key_rejects_shell_mangled_values() {
        // `$2a$10$eEr…` expanded by the shell loses the `$2a$10$eEr` prefix.
        assert!(plausible_key(
            "$2a$10$eEr//aXFibeVWpIBd.GXh.z2ntI8zIFeDVVa1iR9ae5gQuZUBTcWO"
        ));
        assert!(!plausible_key(
            "//aXFibeVWpIBd.GXh.z2ntI8zIFeDVVa1iR9ae5gQuZUBTcWO"
        ));
        assert!(!plausible_key(""));
    }

    #[test]
    fn env_key_parses_dotenv_variants() {
        let dir = std::env::temp_dir().join(format!("easypacker-env-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join(".env");
        std::fs::write(
            &path,
            "# comment\nOTHER=x\nCURSEFORGE_API_KEY=$2a$10$abc\nEMPTY=\n",
        )
        .unwrap();
        assert_eq!(env_key(&path).as_deref(), Some("$2a$10$abc"));
        std::fs::write(&path, "CURSEFORGE_API_KEY=\"$2b$10$quoted\"\n").unwrap();
        assert_eq!(env_key(&path).as_deref(), Some("$2b$10$quoted"));
        std::fs::write(&path, "CURSEFORGE_API_KEY='$2y$10$single'\n").unwrap();
        assert_eq!(env_key(&path).as_deref(), Some("$2y$10$single"));
        std::fs::write(&path, "CURSEFORGE_API_KEY=\n").unwrap();
        assert_eq!(env_key(&path), None);
        assert_eq!(env_key(Path::new("/nonexistent/.env")), None);
        std::fs::remove_dir_all(&dir).ok();
    }
}
