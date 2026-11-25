use crate::{manifest::{self, Manifest, MarketplaceIndex}, settings::MARKETPLACE_URL};
use anyhow::{bail, Context, Result};
use crate::tools;
use std::{
    fs,
    path::{Path, PathBuf},
};

pub fn dir(root: &Path) -> PathBuf {
    root.join("marketplace")
}

/// Records which source the working tree came from, so a restart can return to the default branch.
const APPLIED: &str = ".autark-source";

/// What the session asked the clone to track: the default branch, a branch, or a pull request.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Source {
    Default,
    Branch(String),
    Pr(u64),
}

impl Source {
    pub fn parse(input: &str) -> Result<Self> {
        let value = input.trim();
        if value.is_empty() {
            return Ok(Source::Default);
        }
        if value.len() > 120 {
            bail!("invalid_argument: a marketplace branch or pull request name is too long");
        }
        let number = value.strip_prefix('#').unwrap_or(value);
        if !number.is_empty() && number.chars().all(|c| c.is_ascii_digit()) {
            let parsed: u64 = number
                .parse()
                .context("invalid_argument: that pull request number is too large")?;
            if parsed == 0 {
                bail!("invalid_argument: a pull request number starts at 1");
            }
            return Ok(Source::Pr(parsed));
        }
        let invalid = value.starts_with('-')
            || value.starts_with('/')
            || value.ends_with('/')
            || value.ends_with('.')
            || value.contains("..")
            || value.contains("@{")
            || value.contains("//")
            || value.chars().any(|c| {
                !(c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.' | '/'))
            });
        if invalid {
            bail!(
                "invalid_argument: '{value}' is not a valid branch name or pull request number"
            );
        }
        Ok(Source::Branch(value.to_string()))
    }

    /// The label the UI and installed app records show. Empty means the default branch.
    pub fn label(&self) -> String {
        match self {
            Source::Default => String::new(),
            Source::Branch(branch) => branch.clone(),
            Source::Pr(number) => format!("#{number}"),
        }
    }

    fn refspec(&self) -> String {
        match self {
            Source::Default => "HEAD".to_string(),
            Source::Branch(branch) => format!("refs/heads/{branch}"),
            Source::Pr(number) => format!("refs/pull/{number}/head"),
        }
    }
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

/// The source the working tree currently holds; empty means the default branch.
pub fn applied_source(root: &Path) -> String {
    fs::read_to_string(dir(root).join(APPLIED))
        .map(|text| text.trim().to_string())
        .unwrap_or_default()
}

/// Clones the marketplace on first use, then fetches the requested source and hard-resets it.
pub fn sync(root: &Path, requested: &str) -> Result<String> {
    let source = Source::parse(requested)?;
    let dir = dir(root);
    if !dir.join(".git").exists() {
        if dir.exists() {
            fs::remove_dir_all(&dir).context("cannot clear a partial marketplace clone")?;
        }
        if let Some(parent) = dir.parent() {
            fs::create_dir_all(parent)?;
        }
        // No checkout first: every source, including a pull request, is fetched the same way.
        git(&["clone", "--depth", "1", "--no-checkout", MARKETPLACE_URL, &path_arg(&dir)])?;
    }
    git(&[
        "-C",
        &path_arg(&dir),
        "fetch",
        "--depth",
        "1",
        "origin",
        &source.refspec(),
    ])?;
    git(&["-C", &path_arg(&dir), "reset", "--hard", "FETCH_HEAD"])?;
    git(&["-C", &path_arg(&dir), "clean", "-fd"])?;
    fs::write(dir.join(APPLIED), source.label()).context("cannot record the marketplace source")?;
    Ok(match &source {
        Source::Default => "marketplace refreshed".to_string(),
        Source::Branch(branch) => format!("marketplace preview: branch {branch}"),
        Source::Pr(number) => format!("marketplace preview: #{number}"),
    })
}

fn path_arg(path: &Path) -> String {
    path.to_string_lossy().to_string()
}

fn git(args: &[&str]) -> Result<()> {
    let output = tools::new_command("git")
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
