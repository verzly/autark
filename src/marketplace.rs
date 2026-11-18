use crate::{manifest::{self, Manifest, MarketplaceIndex}, settings::MARKETPLACE_URL};
use anyhow::{bail, Context, Result};
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};

pub fn dir(root: &Path) -> PathBuf {
    root.join("marketplace")
}

pub struct ScanResult {
    pub manifests: Vec<ScannedApp>,
    pub warnings: Vec<String>,
}

pub struct ScannedApp {
    pub dir: PathBuf,
    pub manifest: Manifest,
    pub local: bool,
}

pub fn scan(root: &Path) -> Result<ScanResult> {
    let dir = dir(root);
    let index_path = dir.join("marketplace.json");
    let text = fs::read_to_string(&index_path)
        .with_context(|| format!("cannot read {}", index_path.display()))?;
    let index: MarketplaceIndex =
        serde_json::from_str(&text).context("invalid marketplace.json")?;
    if index.schema != manifest::SCHEMA {
        bail!(
            "invalid_argument: unsupported marketplace schema {}",
            index.schema
        );
    }
    let mut manifests = Vec::new();
    let mut warnings = Vec::new();
    for id in &index.apps {
        let app_dir = dir.join("apps").join(id);
        match Manifest::load(&app_dir) {
            Ok(manifest) => manifests.push(ScannedApp { dir: app_dir, manifest, local: false }),
            Err(error) => warnings.push(format!("{id}: {error:#}")),
        }
    }
    let local_dir = crate::local::dir(root);
    if let Ok(entries) = fs::read_dir(&local_dir) {
        for entry in entries.flatten().filter(|entry| entry.path().is_dir()) {
            let app_dir = entry.path();
            let id = entry.file_name().to_string_lossy().to_string();
            match Manifest::load(&app_dir) {
                Ok(manifest) => manifests.push(ScannedApp { dir: app_dir, manifest, local: true }),
                Err(error) => warnings.push(format!("{id}: {error:#}")),
            }
        }
    }
    Ok(ScanResult {
        manifests,
        warnings,
    })
}

/// Clones the marketplace on first use, then fetches and hard-resets it.
pub fn sync(root: &Path) -> Result<String> {
    let dir = dir(root);
    if dir.join(".git").exists() {
        git(&["-C", &path_arg(&dir), "fetch", "--depth", "1", "origin", "main"])?;
        git(&["-C", &path_arg(&dir), "reset", "--hard", "FETCH_HEAD"])?;
        git(&["-C", &path_arg(&dir), "clean", "-fd"])?;
        Ok("marketplace refreshed".to_string())
    } else {
        if dir.exists() {
            fs::remove_dir_all(&dir).context("cannot clear a partial marketplace clone")?;
        }
        if let Some(parent) = dir.parent() {
            fs::create_dir_all(parent)?;
        }
        git(&[
            "clone",
            "--depth",
            "1",
            "--branch",
            "main",
            MARKETPLACE_URL,
            &path_arg(&dir),
        ])?;
        Ok("marketplace cloned".to_string())
    }
}

fn path_arg(path: &Path) -> String {
    path.to_string_lossy().to_string()
}

fn git(args: &[&str]) -> Result<()> {
    let output = Command::new("git")
        .args(args)
        .output()
        .context("git is not available on PATH")?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stdout = String::from_utf8_lossy(&output.stdout);
        let detail = [stderr.trim(), stdout.trim()]
            .into_iter()
            .filter(|part| !part.is_empty())
            .collect::<Vec<_>>()
            .join(" ");
        bail!(
            "git {} failed: {}",
            args.iter().take(2).cloned().collect::<Vec<_>>().join(" "),
            tail(&detail, 400)
        );
    }
    Ok(())
}

pub fn tail(text: &str, limit: usize) -> String {
    if text.len() <= limit {
        return text.to_string();
    }
    let start = text.len() - limit;
    let start = text
        .char_indices()
        .map(|(index, _)| index)
        .find(|index| *index >= start)
        .unwrap_or(text.len());
    format!("...{}", &text[start..])
}
