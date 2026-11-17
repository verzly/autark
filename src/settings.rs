use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::{
    env, fs,
    path::{Path, PathBuf},
    sync::OnceLock,
};

pub const DEFAULT_FOLDER_NAME: &str = "Autark";
pub const MARKETPLACE_URL: &str = "https://github.com/verzly/autark-marketplace.git";

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub root: String,
    pub folder_name: String,
    pub force_local_mise: bool,
    pub open_tabs: Vec<String>,
    pub active_tab: Option<String>,
    pub marketplace_last_refresh: Option<u64>,
    pub tray_enabled: bool,
    pub start_with_windows: bool,
    pub close_to_tray: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            root: default_root().to_string_lossy().to_string(),
            folder_name: DEFAULT_FOLDER_NAME.to_string(),
            force_local_mise: false,
            open_tabs: Vec::new(),
            active_tab: None,
            marketplace_last_refresh: None,
            tray_enabled: false,
            start_with_windows: false,
            close_to_tray: false,
        }
    }
}

impl Settings {
    pub fn load_or_default() -> Self {
        let path = settings_path();
        fs::read_to_string(&path)
            .ok()
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or_default()
    }

    pub fn root_path(&self) -> PathBuf {
        PathBuf::from(self.root.trim())
    }

    pub fn save(&self) -> Result<()> {
        let root = self.root_path();
        fs::create_dir_all(&root)
            .with_context(|| format!("cannot create root folder {}", root.display()))?;
        let text = serde_json::to_string_pretty(self)?;
        fs::write(settings_path(), text).context("cannot write launcher-settings.json")?;
        write_pointer(&root)?;
        Ok(())
    }

    /// Renames the root folder when requested and keeps `root` in sync with `folder_name`.
    pub fn apply_folder_name(&mut self, move_folder: bool) -> Result<()> {
        let folder_name = {
            let trimmed = self.folder_name.trim();
            if trimmed.is_empty() {
                DEFAULT_FOLDER_NAME.to_string()
            } else {
                trimmed.to_string()
            }
        };
        self.folder_name = folder_name.clone();

        let current = self.root_path();
        let parent = current
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(default_root);
        let target = parent.join(&folder_name);
        if target == current {
            self.root = target.to_string_lossy().to_string();
            return Ok(());
        }
        if move_folder && current.exists() {
            if target.exists() {
                anyhow::bail!("invalid_argument: target folder already exists");
            }
            fs::create_dir_all(&parent)?;
            fs::rename(&current, &target).context("cannot rename the root folder")?;
        }
        self.root = target.to_string_lossy().to_string();
        Ok(())
    }
}

fn default_root() -> PathBuf {
    env::var_os("USERPROFILE")
        .map(PathBuf::from)
        .unwrap_or_default()
        .join(DEFAULT_FOLDER_NAME)
}

pub fn pointer_path() -> &'static Path {
    static PATH: OnceLock<PathBuf> = OnceLock::new();
    PATH.get_or_init(|| {
        if let Ok(exe) = env::current_exe() {
            if let Some(dir) = exe.parent() {
                if is_writable(dir) {
                    return dir.join("launcher-root.json");
                }
            }
        }
        let fallback = env::var_os("LOCALAPPDATA")
            .map(PathBuf::from)
            .unwrap_or_else(env::temp_dir)
            .join("Autark");
        let _ = fs::create_dir_all(&fallback);
        fallback.join("launcher-root.json")
    })
}

fn is_writable(dir: &Path) -> bool {
    let probe = dir.join(".autark-settings-probe");
    if fs::write(&probe, b"probe").is_err() {
        return false;
    }
    let _ = fs::remove_file(&probe);
    true
}

pub fn active_root() -> PathBuf {
    let Ok(text) = fs::read_to_string(pointer_path()) else {
        return Settings::default().root_path();
    };
    let Ok(value) = serde_json::from_str::<serde_json::Value>(&text) else {
        return Settings::default().root_path();
    };
    value
        .get("root")
        .and_then(|root| root.as_str())
        .filter(|root| !root.trim().is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| Settings::default().root_path())
}

pub fn settings_path() -> PathBuf {
    active_root().join("launcher-settings.json")
}

pub fn write_pointer(root: &Path) -> Result<()> {
    let path = pointer_path();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let text = serde_json::to_string_pretty(&serde_json::json!({
        "root": root.to_string_lossy(),
    }))?;
    fs::write(path, text).context("cannot write launcher-root.json")
}
