use crate::{apps, settings::Settings};
use anyhow::{bail, Context, Result};
use serde::Serialize;
use std::{
    env, fs,
    path::{Path, PathBuf},
    process::Command,
};
use tauri::AppHandle;

#[derive(Clone, Debug, Serialize)]
pub struct BinEntry {
    pub name: String,
    pub path: String,
    pub version: Option<String>,
    pub size: u64,
}

#[derive(Clone, Debug, Serialize)]
pub struct DetectedMise {
    pub path: String,
    pub version: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
pub struct MiseInfo {
    pub available: bool,
    pub source: String,
    pub path: Option<String>,
    pub version: Option<String>,
    pub local_path: String,
    pub bin_path: String,
    pub bin_in_path: bool,
    pub bin: Vec<BinEntry>,
    pub detected: Vec<DetectedMise>,
    pub force_local: bool,
}

pub fn bin_dir(root: &Path) -> PathBuf {
    root.join("bin")
}

pub fn local_mise(root: &Path) -> PathBuf {
    let new = bin_dir(root).join("mise.exe");
    if new.exists() { new } else { root.join("runtime").join("bin").join("mise.exe") }
}

pub fn resolve(settings: &Settings, root: &Path) -> Option<PathBuf> {
    let local = local_mise(root);
    if settings.force_local_mise {
        return local.exists().then_some(local);
    }
    if let Some(path) = settings.mise_path.as_ref().map(PathBuf::from).filter(|p| p.is_file()) {
        return Some(path);
    }
    if local.exists() { return Some(local); }
    if let Some(path) = env::var_os("MISE_BIN").map(PathBuf::from).filter(|p| p.is_file()) {
        return Some(path);
    }
    where_mise()
}

/// Every `mise` on PATH, so the mise step can offer them as one click.
fn where_all() -> Vec<PathBuf> {
    let output = match Command::new("where").arg("mise").output() {
        Ok(output) if output.status.success() => output,
        _ => return Vec::new(),
    };
    let mut found: Vec<PathBuf> = Vec::new();
    for line in String::from_utf8_lossy(&output.stdout).lines() {
        let path = PathBuf::from(line.trim());
        if line.trim().is_empty() || !path.is_file() {
            continue;
        }
        if found.iter().any(|seen| seen.to_string_lossy().eq_ignore_ascii_case(&path.to_string_lossy())) {
            continue;
        }
        found.push(path);
    }
    found
}

/// `where.exe` turns the PATH entry into a real path, which the UI lists and reports.
fn where_mise() -> Option<PathBuf> {
    where_all().into_iter().next()
}

fn environment(command: &mut Command, root: &Path) {
    let runtime = root.join("runtime");
    command.env("MISE_DATA_DIR", runtime.join("mise-data"))
        .env("MISE_CONFIG_DIR", runtime.join("mise-config"))
        .env("MISE_CACHE_DIR", runtime.join("mise-cache"))
        .env("MISE_STATE_DIR", runtime.join("mise-state"))
        .env("MISE_TMP_DIR", runtime.join("tmp"));
}

fn run_version(path: &Path, root: &Path) -> Option<String> {
    let mut command = Command::new(path);
    command.arg("--version");
    environment(&mut command, root);
    let output = command.output().ok()?;
    output.status.success().then(|| String::from_utf8_lossy(&output.stdout)
        .lines().find(|line| !line.trim().is_empty()).map(|line| line.trim().to_string())).flatten()
}

fn user_path() -> Vec<String> {
    let output = Command::new("powershell")
        .args([
            "-NoProfile",
            "-Command",
            "$p=[Environment]::GetEnvironmentVariable('Path','User'); if ($p) { $p -split ';' | ForEach-Object { $_.Trim().Trim('\"') } }",
        ])
        .output();
    match output {
        Ok(output) if output.status.success() => String::from_utf8_lossy(&output.stdout)
            .lines()
            .map(|line| line.trim().to_string())
            .filter(|line| !line.is_empty())
            .collect(),
        _ => Vec::new(),
    }
}

fn registry_has_path(path: &Path) -> bool {
    let target = path.to_string_lossy().to_lowercase();
    user_path()
        .iter()
        .any(|entry| entry.replace('/', "\\").to_lowercase() == target.replace('/', "\\"))
}

pub fn probe(settings: &Settings) -> MiseInfo {
    let root = settings.root_path();
    let selected = resolve(settings, &root);
    let mut source = "missing";
    let mut path = None;
    let mut version = None;
    if let Some(resolved) = selected {
        let local = local_mise(&root);
        source = if resolved == local { "local" }
            else if settings.mise_path.as_deref().map(Path::new) == Some(resolved.as_path())
                || env::var_os("MISE_BIN").map(PathBuf::from).as_ref() == Some(&resolved) { "system" }
            else { "path" };
        version = run_version(&resolved, &root);
        if version.is_some() {
            path = Some(resolved.to_string_lossy().to_string());
        } else {
            source = "missing";
        }
    }
    let bin_path = bin_dir(&root);
    let mut bin = Vec::new();
    if let Ok(entries) = fs::read_dir(&bin_path) {
        for entry in entries.flatten().filter(|e| e.path().is_file()) {
            let meta = match entry.metadata() { Ok(meta) => meta, Err(_) => continue };
            let name = entry.file_name().to_string_lossy().to_string();
            bin.push(BinEntry { name, path: entry.path().to_string_lossy().to_string(), version: None, size: meta.len() });
        }
    }
    let listing = format!("Get-ChildItem '{}' -File | ForEach-Object {{ \"$($_.Name)|$([string]$_.VersionInfo.ProductVersion)|$($_.Length)\" }}", bin_path.to_string_lossy().replace('\'', "''"));
    if let Ok(output) = Command::new("powershell").args(["-NoProfile", "-Command", &listing]).output() {
        for line in String::from_utf8_lossy(&output.stdout).lines() {
            let mut parts = line.splitn(3, '|');
            if let (Some(name), Some(ver), _) = (parts.next(), parts.next(), parts.next()) {
                if let Some(entry) = bin.iter_mut().find(|entry| entry.name.eq_ignore_ascii_case(name)) {
                    entry.version = (!ver.trim().is_empty()).then(|| ver.trim().to_string());
                }
            }
        }
    }
    if let Some(mise) = bin.iter_mut().find(|entry| entry.name.eq_ignore_ascii_case("mise.exe")) {
        mise.version = version.clone();
    }
    let own = local_mise(&root);
    let own_key = own.to_string_lossy().to_lowercase();
    let detected: Vec<DetectedMise> = where_all()
        .into_iter()
        .filter(|candidate| candidate.to_string_lossy().to_lowercase() != own_key)
        .filter_map(|candidate| {
            run_version(&candidate, &root).map(|found| DetectedMise {
                path: candidate.to_string_lossy().to_string(),
                version: Some(found),
            })
        })
        .collect();
    MiseInfo {
        available: version.is_some(), source: source.to_string(), path, version,
        local_path: bin_dir(&root).join("mise.exe").to_string_lossy().to_string(),
        bin_path: bin_path.to_string_lossy().to_string(),
        bin_in_path: registry_has_path(&bin_path), bin, detected, force_local: settings.force_local_mise,
    }
}

pub fn install_local(app: &AppHandle, id: &str, root: &Path) -> Result<()> {
    let target = bin_dir(root).join("mise.exe");
    fs::create_dir_all(bin_dir(root))?;
    let target_arg = target.to_string_lossy().replace('\'', "''");
    let script = format!("$ErrorActionPreference='Stop'; $headers=@{{'User-Agent'='autark'}}; $release=Invoke-RestMethod -Uri 'https://api.github.com/repos/jdx/mise/releases/latest' -Headers $headers; $asset=$release.assets | Where-Object {{ $_.name -like '*windows-x64.exe' }} | Select-Object -First 1; if (-not $asset) {{ throw 'no windows-x64 asset in the latest mise release' }}; $total=[long]$asset.size; $request=[System.Net.HttpWebRequest]::Create($asset.browser_download_url); $request.UserAgent='autark'; $response=$request.GetResponse(); $input=$response.GetResponseStream(); $output=[System.IO.File]::Create('{target_arg}'); $buffer=New-Object byte[] 1048576; $done=[long]0; $last=-1; while (($read=$input.Read($buffer,0,$buffer.Length)) -gt 0) {{ $output.Write($buffer,0,$read); $done+=$read; if ($total -gt 0) {{ $pct=[math]::Floor($done*100/$total); if ($pct -ne $last) {{ $last=$pct; Write-Output ('mise download ' + $pct + '%') }} }} }}; $output.Close(); $input.Close(); $response.Close(); Write-Output 'mise download 100%'");
    let mut command = Command::new("powershell");
    command.args(["-NoProfile", "-ExecutionPolicy", "Bypass", "-Command", &script]);
    apps::run_tool_streaming(app, id, "downloading mise".to_string(), command)?;
    if run_version(&target, root).is_none() { bail!("internal: the downloaded mise binary does not run"); }
    Ok(())
}

pub fn self_update(app: &AppHandle, id: &str, root: &Path) -> Result<()> {
    let path = local_mise(root);
    if !path.is_file() {
        bail!("invalid_argument: Autark's own mise is not installed");
    }
    let mut command = Command::new(path);
    command.arg("self-update");
    environment(&mut command, root);
    apps::run_tool_streaming(app, id, "updating mise".to_string(), command)
}

pub fn set_path_registration(root: &Path, enabled: bool) -> Result<bool> {
    let path = bin_dir(root).to_string_lossy().to_string();
    let quoted = path.replace('\'', "''");
    let action = if enabled {
        format!("if (-not ($parts | Where-Object {{ $_.Trim().Trim('\"') -ieq '{quoted}' }})) {{ $parts += '{quoted}' }}")
    } else {
        format!("$parts = @($parts | Where-Object {{ $_.Trim().Trim('\"') -ine '{quoted}' }})")
    };
    let script = format!("$old=[Environment]::GetEnvironmentVariable('Path','User'); $parts=@($old -split ';' | Where-Object {{ $_ -ne '' }}); {action}; [Environment]::SetEnvironmentVariable('Path',($parts -join ';'),'User')");
    let output = Command::new("powershell").args(["-NoProfile", "-ExecutionPolicy", "Bypass", "-Command", &script]).output()?;
    if !output.status.success() { bail!("{}", String::from_utf8_lossy(&output.stderr)); }
    Ok(registry_has_path(&bin_dir(root)))
}
