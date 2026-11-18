use crate::manifest::Manifest;
use anyhow::{bail, Context, Result};
use std::{fs, path::{Path, PathBuf}};

pub fn dir(root: &Path) -> PathBuf {
    root.join("local-apps")
}

pub fn add(root: &Path, source: &Path) -> Result<String> {
    let text = fs::read_to_string(source).context("cannot read local app manifest")?;
    let mut manifest: Manifest = serde_json::from_str(&text).context("invalid local app manifest")?;
    let base = manifest.id.clone();
    let apps = dir(root);
    let mut key = base.clone();
    let mut suffix = 2;
    while apps.join(&key).exists() {
        key = format!("{base}-{suffix}");
        suffix += 1;
    }
    manifest.id = key.clone();
    let target = apps.join(&key);
    fs::create_dir_all(&target)?;
    fs::write(target.join("app.json"), serde_json::to_string_pretty(&manifest)?)?;
    copy_icon(&manifest, source, &target)?;
    if let Err(error) = Manifest::load(&target) {
        let _ = fs::remove_dir_all(&target);
        return Err(error);
    }
    Ok(key)
}

pub fn replace(root: &Path, key: &str, source: &Path) -> Result<()> {
    let target = dir(root).join(key);
    if !target.is_dir() {
        bail!("not a local app");
    }
    let text = fs::read_to_string(source).context("cannot read local app manifest")?;
    let mut manifest: Manifest = serde_json::from_str(&text).context("invalid local app manifest")?;
    manifest.id = key.to_string();
    let manifest_path = target.join("app.json");
    let previous = fs::read_to_string(&manifest_path).ok();
    fs::write(&manifest_path, serde_json::to_string_pretty(&manifest)?)?;
    copy_icon(&manifest, source, &target)?;
    if let Err(error) = Manifest::load(&target) {
        if let Some(previous) = previous {
            let _ = fs::write(&manifest_path, previous);
        }
        return Err(error);
    }
    Ok(())
}

fn copy_icon(manifest: &Manifest, source: &Path, target: &Path) -> Result<()> {
    if let Some(icon) = manifest.icon.as_deref().filter(|icon| !icon.contains(['/', '\\'])) {
        if let Some(parent) = source.parent() {
            let icon_source = parent.join(icon);
            if icon_source.is_file() {
                fs::copy(icon_source, target.join(icon)).context("cannot copy local app icon")?;
            }
        }
    }
    Ok(())
}
