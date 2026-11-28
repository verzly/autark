use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::{fs, path::Path};

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Prefs {
    pub auto_start: bool,
    pub shortcut_autostart: bool,
    pub shortcut_background: bool,
    pub source_ref: Option<String>,
}

pub fn load(app_dir: &Path) -> Prefs {
    fs::read_to_string(app_dir.join("launcher.json"))
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default()
}

pub fn save(app_dir: &Path, prefs: &Prefs) -> Result<()> {
    fs::create_dir_all(app_dir)?;
    fs::write(app_dir.join("launcher.json"), serde_json::to_string_pretty(prefs)?)
        .context("cannot write app launcher.json")
}
