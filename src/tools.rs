use crate::{apps, settings::Settings};
use anyhow::{bail, Context, Result};
use serde::Serialize;
use std::{
    env, ffi::OsStr, fs,
    os::windows::process::CommandExt,
    path::{Path, PathBuf},
    process::Command,
};
use tauri::AppHandle;

/// Console apps Autark starts must stay invisible; only the terminal button asks for a console.
pub(crate) fn new_command(program: impl AsRef<OsStr>) -> Command {
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    let mut command = Command::new(program);
    command.creation_flags(CREATE_NO_WINDOW);
    command
}

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

#[derive(Clone, Debug, Serialize)]
pub struct BinInfo {
    /// A bin.exe Autark can run, either its own copy or one found elsewhere.
    pub available: bool,
    /// Autark downloaded bin into its own bin folder.
    pub installed: bool,
    /// The running bin.exe lives outside Autark's bin folder.
    pub external: bool,
    pub path: Option<String>,
    pub local_path: String,
    /// Where bin installs tools, independent of which bin.exe runs.
    pub install_dir: String,
    /// Other bin.exe files on the PATH, offered in the wizard.
    pub detected: Vec<DetectedMise>,
}

#[derive(Clone, Debug, Serialize)]
pub struct Pg0Info {
    pub available: bool,
    pub version: Option<String>,
    pub path: Option<String>,
    pub installation: String,
    pub source: Option<String>,
    #[serde(default)]
    pub used_by: Vec<String>,
}

pub fn pg0_exe(root: &Path) -> PathBuf {
    root.join("bin").join("pg0.exe")
}

/// Finds a pg0.exe that an app installed into its own Python environment.
fn find_app_pg0(root: &Path) -> Option<PathBuf> {
    let apps = root.join("apps");
    for entry in fs::read_dir(apps).ok()?.flatten() {
        let base = entry.path().join("venv");
        for lib in ["Lib", "lib"] {
            let candidate = base
                .join(lib)
                .join("site-packages")
                .join("pg0")
                .join("bin")
                .join("pg0.exe");
            if candidate.exists() {
                return Some(candidate);
            }
        }
    }
    None
}

/// A pg0 in Autark's bin is the shared copy; otherwise an app's copy is only a source.
pub fn probe_pg0(root: &Path) -> Pg0Info {
    let own = pg0_exe(root);
    let shim = root
        .join("runtime")
        .join("mise-data")
        .join("shims")
        .join("pg0.exe");
    let installed = if own.exists() {
        Some(own)
    } else if shim.exists() {
        Some(shim)
    } else {
        None
    };
    let source = if installed.is_none() {
        find_app_pg0(root)
    } else {
        None
    };
    Pg0Info {
        available: installed.is_some(),
        version: installed.as_deref().and_then(|path| run_version(path, root)),
        path: installed.map(|path| path.to_string_lossy().to_string()),
        installation: pg0_installation(root).to_string_lossy().to_string(),
        source: source.map(|path| path.to_string_lossy().to_string()),
        used_by: Vec::new(),
    }
}

/// Installs pg0 with Autark's mise (github:vectorize-io/pg0). When mise cannot
/// install it, an app's own pg0.exe is copied into Autark's bin instead.
pub fn install_pg0(
    app: &AppHandle,
    id: &str,
    settings: &Settings,
    root: &Path,
    force: bool,
) -> Result<()> {
    let resolved = resolve(settings, root);
    if resolved.is_some() {
        for sub in ["mise-data", "mise-config", "mise-cache", "mise-state", "tmp"] {
            fs::create_dir_all(root.join("runtime").join(sub))?;
        }
        fs::write(global_config(root), git_config(root))
            .context("cannot write mise global config")?;
        fs::write(git_postinstall(root), GIT_POSTINSTALL_PS)
            .context("cannot write the git postinstall step")?;
        let mut command = git_command(settings, root);
        command.args(["install"]);
        if force {
            command.arg("--force");
        }
        command.arg("pg0");
        if let Err(error) = apps::run_tool_streaming(app, id, "installing pg0".to_string(), command) {
            if error.to_string().starts_with("cancelled") {
                return Err(error);
            }
            apps::log(app, id, format!("mise pg0 install failed; trying app-local fallback: {error:#}"));
        }
        if pg0_shim(root).exists() {
            match fs::remove_file(pg0_exe(root)) {
                Ok(()) => apps::log(app, id, "removed stale app-local pg0.exe".to_string()),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => return Err(error).context("cannot remove stale app-local pg0.exe"),
            }
            return Ok(());
        }
    }
    let target = pg0_exe(root);
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent)?;
    }
    let source = find_app_pg0(root)
        .context("no pg0.exe found in an app; install an app that uses pg0 first")?;
    fs::copy(&source, &target)
        .with_context(|| format!("cannot copy {}", source.display()))?;
    apps::log(app, id, format!("pg0.exe installed from {}", source.display()));
    Ok(())
}

/// Removes pg0 from Autark's mise and drops the shim. The tool entry stays in
/// the generated config, so a later install puts it back.
pub fn uninstall_pg0(app: &AppHandle, id: &str, settings: &Settings, root: &Path) -> Result<()> {
    if resolve(settings, root).is_some() {
        let mut command = git_command(settings, root);
        command.args(["uninstall", "pg0"]);
        let output = command.output().context("cannot run mise uninstall pg0")?;
        if !output.status.success() {
            let tail = |bytes: &[u8]| {
                String::from_utf8_lossy(bytes).lines().rev().take(4).collect::<Vec<_>>()
                    .into_iter().rev().collect::<Vec<_>>().join("\n")
            };
            bail!("mise uninstall pg0 failed ({}): {}", output.status, tail(if output.stderr.is_empty() { &output.stdout } else { &output.stderr }));
        }
    }
    let mut errors = Vec::new();
    for path in [pg0_exe(root), pg0_shim(root)] {
        if let Err(error) = fs::remove_file(&path) {
            if error.kind() != std::io::ErrorKind::NotFound {
                errors.push(format!("{}: {error}", path.display()));
            }
        }
    }
    if !errors.is_empty() {
        bail!("failed to remove pg0 files: {}", errors.join("; "));
    }
    apps::log(app, id, "pg0 removed from Autark".to_string());
    Ok(())
}
fn pg0_shim(root: &Path) -> PathBuf {
    root.join("runtime")
        .join("mise-data")
        .join("shims")
        .join("pg0.exe")
}
/// The shared pg0 installation: one PostgreSQL payload for every app.
pub fn pg0_installation(root: &Path) -> PathBuf {
    root.join("runtime").join("pg0").join("installation")
}

pub fn bin_dir(root: &Path) -> PathBuf {
    root.join("bin")
}

/// bin refuses to start when `BIN_CONFIG` points at a missing file, so every bin
/// call gets the same config file. bin also asks for a download directory on its
/// first run, so a fresh config names Autark's bin folder and never prompts.
pub fn bin_config(root: &Path) -> PathBuf {
    root.join("runtime").join("bin-config").join("config.json")
}

pub fn ensure_bin_config(root: &Path) -> PathBuf {
    let path = bin_config(root);
    if !path.is_file() {
        if let Some(parent) = path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        let seed = serde_json::json!({
            "default_path": bin_dir(root).to_string_lossy(),
            "bins": {},
        });
        if let Err(error) = fs::write(&path, seed.to_string()) {
            eprintln!("could not create the bin config: {error}");
        }
    }
    path
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

/// Every copy of `name` on PATH, so the setup steps can offer them as one click.
fn where_tool(name: &str) -> Vec<PathBuf> {
    let output = match new_command("where").arg(name).output() {
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
    where_tool("mise").into_iter().next()
}

pub(crate) fn environment(command: &mut Command, root: &Path) {
    let runtime = root.join("runtime");
    command.env("MISE_DATA_DIR", runtime.join("mise-data"))
        .env("MISE_CONFIG_DIR", runtime.join("mise-config"))
        .env("MISE_CACHE_DIR", runtime.join("mise-cache"))
        .env("MISE_STATE_DIR", runtime.join("mise-state"))
        .env("MISE_TMP_DIR", runtime.join("tmp"))
        // bin keeps one install and one config for every shell, app and tool.
        .env("BIN_INSTALL_DIR", bin_dir(root))
        .env("BIN_CONFIG", ensure_bin_config(root));
}

fn run_version(path: &Path, root: &Path) -> Option<String> {
    let mut command = new_command(path);
    command.arg("--version");
    environment(&mut command, root);
    let output = command.output().ok()?;
    output.status.success().then(|| String::from_utf8_lossy(&output.stdout)
        .lines().find(|line| !line.trim().is_empty()).map(|line| line.trim().to_string())).flatten()
}

fn user_path() -> Vec<String> {
    let output = new_command("powershell")
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
    if let Ok(output) = new_command("powershell").args(["-NoProfile", "-Command", &listing]).output() {
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
    let detected: Vec<DetectedMise> = where_tool("mise")
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
    let mut command = new_command("powershell");
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
    let mut command = new_command(path);
    command.arg("self-update");
    environment(&mut command, root);
    apps::run_tool_streaming(app, id, "updating mise".to_string(), command)
}

pub fn bin_exe(root: &Path) -> PathBuf {
    bin_dir(root).join("bin.exe")
}

/// Autark's own bin.exe wins; otherwise the configured file, then the PATH.
pub fn resolve_bin(settings: &Settings, root: &Path) -> Option<PathBuf> {
    let own = bin_exe(root);
    if own.is_file() {
        return Some(own);
    }
    if let Some(path) = settings.bin_path.as_deref().map(PathBuf::from).filter(|path| path.is_file()) {
        return Some(path);
    }
    where_tool("bin").into_iter().next()
}

/// Every bin call gets Autark's config, so an external bin.exe still installs
/// into Autark's bin folder instead of its own download directory.
pub fn probe_bin(settings: &Settings, root: &Path) -> BinInfo {
    let own = bin_exe(root);
    let installed = own.is_file();
    let own_key = own.to_string_lossy().to_lowercase();
    let detected: Vec<DetectedMise> = where_tool("bin")
        .into_iter()
        .filter(|candidate| candidate.to_string_lossy().to_lowercase() != own_key)
        .map(|candidate| DetectedMise { path: candidate.to_string_lossy().to_string(), version: None })
        .collect();
    let resolved = resolve_bin(settings, root);
    BinInfo {
        available: resolved.is_some(),
        installed,
        external: resolved.as_ref().is_some_and(|path| *path != own),
        path: resolved.map(|path| path.to_string_lossy().to_string()),
        local_path: own.to_string_lossy().to_string(),
        install_dir: bin_dir(root).to_string_lossy().to_string(),
        detected,
    }
}

/// Installs the latest bin release into Autark's bin and lets bin register
/// itself, so `bin update` keeps bin and every installed tool current.
pub fn install_bin(app: &AppHandle, id: &str, root: &Path) -> Result<()> {
    fs::create_dir_all(bin_dir(root))?;
    let target = bin_exe(root);
    let temp = target.with_extension("new");
    let script = BIN_DOWNLOAD.replace("@TEMP@", &ps_quote(&temp)).replace("@TARGET@", &ps_quote(&target));
    let mut command = new_command("powershell");
    command.args(["-NoProfile", "-ExecutionPolicy", "Bypass", "-Command", &script]);
    apps::run_tool_streaming(app, id, "downloading bin".to_string(), command)?;
    if !target.is_file() {
        bail!("internal: the downloaded bin binary is missing");
    }
    let mut command = new_command(&target);
    command.args(["install", "github.com/marcosnils/bin", "--force"]);
    environment(&mut command, root);
    let result = apps::run_tool_streaming(app, id, "registering bin".to_string(), command);
    // Replacing the running bin leaves its previous file behind on Windows.
    let _ = fs::remove_file(bin_dir(root).join(".bin.exe.old"));
    result
}

/// Runs bin's own updater. Without a name every tool is updated, which also
/// covers bin itself once it is registered.
pub fn update_bin(app: &AppHandle, id: &str, settings: &Settings, root: &Path, name: Option<&str>) -> Result<()> {
    let path = resolve_bin(settings, root).context("invalid_argument: bin is not installed")?;
    let mut command = new_command(&path);
    command.arg("update");
    if let Some(name) = name {
        command.arg(name);
    }
    environment(&mut command, root);
    let label = name.map(|name| format!("updating {name} with bin")).unwrap_or_else(|| "updating tools with bin".to_string());
    apps::run_tool_streaming(app, id, label, command)
}

fn ps_quote(path: &Path) -> String {
    path.to_string_lossy().replace('\'', "''")
}

/// Latest bin release, checksum checked before the temp file replaces bin.exe.
const BIN_DOWNLOAD: &str = r#"$ErrorActionPreference='Stop'
$headers=@{'User-Agent'='autark'}
$release=Invoke-RestMethod -Uri 'https://api.github.com/repos/marcosnils/bin/releases/latest' -Headers $headers
$arch=if ($env:PROCESSOR_ARCHITECTURE -like 'ARM64') { 'arm64' } else { 'amd64' }
$asset=$release.assets | Where-Object { $_.name -like ('*windows_' + $arch + '.exe') } | Select-Object -First 1
if (-not $asset) { throw ('no windows_' + $arch + ' asset in the latest bin release') }
Invoke-WebRequest -Uri $asset.browser_download_url -OutFile '@TEMP@' -UseBasicParsing
$sums=$release.assets | Where-Object { $_.name -eq 'checksums.txt' } | Select-Object -First 1
if ($sums) {
  $text=(Invoke-WebRequest -Uri $sums.browser_download_url -UseBasicParsing).Content
  $line=($text -split "`n" | Where-Object { $_ -match [regex]::Escape($asset.name) } | Select-Object -First 1)
  if ($line) {
    $expected=($line -split '\s+')[0].ToLower()
    $actual=(Get-FileHash -Algorithm SHA256 '@TEMP@').Hash.ToLower()
    if ($expected -ne $actual) { throw 'bin checksum mismatch' }
  }
}
Move-Item -Force '@TEMP@' '@TARGET@'
Write-Output ('bin ' + $release.tag_name + ' installed')"#;

/// Autark's mise global config. git lives here, so every mise run of Autark sees it.
pub fn global_config(root: &Path) -> PathBuf {
    root.join("runtime").join("mise-config").join("config.toml")
}

fn git_postinstall(root: &Path) -> PathBuf {
    global_config(root).with_file_name("postinstall-git.ps1")
}

fn git_config(root: &Path) -> String {
    GIT_MISE_TOML.replace("@@POSTINSTALL@@", &git_postinstall(root).display().to_string())
}

/// git is a global tool of Autark's own mise, so its recipe is the global config file.
/// The extraction step runs through PowerShell, so mise never has to parse it in cmd.
const GIT_POSTINSTALL_PS: &str = r##"$ErrorActionPreference = 'Stop'

$root = $env:MISE_TOOL_INSTALL_PATH
$stage = Join-Path $root '.portablegit-stage'

$archive = Get-ChildItem -LiteralPath $root -Recurse -File -Filter 'PortableGit-*.7z.exe' |
    Select-Object -First 1

if (!$archive) { throw "PortableGit archive not found" }
if (Test-Path -LiteralPath $stage) { throw "Staging directory already exists" }

New-Item -ItemType Directory -Path $stage | Out-Null

$exe = Join-Path $stage $archive.Name
Move-Item -LiteralPath $archive.FullName -Destination $exe

$process = Start-Process -FilePath $exe `
    -ArgumentList '-y -gm2' `
    -WorkingDirectory $stage `
    -Wait -PassThru

if ($process.ExitCode -ne 0) {
    throw "Extraction failed: $($process.ExitCode)"
}

$source = Join-Path $stage 'PortableGit'

if (!(Test-Path "$source\cmd\git.exe")) {
    throw "git.exe not found after extraction"
}

if (!(Test-Path "$source\usr\bin\gpg.exe")) {
    throw "gpg.exe not found after extraction"
}

function Move-Merge {
    param(
        [string]$From,
        [string]$To
    )

    foreach ($item in Get-ChildItem -LiteralPath $From -Force) {
        $destination = Join-Path $To $item.Name

        if ($item.PSIsContainer -and !$item.LinkType -and
            (Test-Path -LiteralPath $destination -PathType Container)) {
            Move-Merge -From $item.FullName -To $destination
            Remove-Item -LiteralPath $item.FullName -Force
        }
        elseif (Test-Path -LiteralPath $destination) {
            throw "Destination already exists: $destination"
        }
        else {
            Move-Item -LiteralPath $item.FullName -Destination $destination
        }
    }
}

Move-Merge -From $source -To $root

$git = Join-Path $root 'cmd\git.exe'
$gpg = Join-Path $root 'usr\bin\gpg.exe'

if (!(Test-Path $git)) { throw "git.exe not found" }
if (!(Test-Path $gpg)) { throw "gpg.exe not found" }

& $git --version
if ($LASTEXITCODE -ne 0) { throw "Git verification failed" }

& $gpg --version
if ($LASTEXITCODE -ne 0) { throw "GPG verification failed" }

Remove-Item -LiteralPath $stage -Recurse -Force"##;

const GIT_MISE_TOML: &str = r##"# Managed by Autark. git is installed once, by Autark's own mise.
[tools."git"]
version = "latest"
asset_pattern = "PortableGit-*-64-bit.7z.exe"
format = "raw"
bin_path = "cmd"

postinstall = '''powershell -NoProfile -ExecutionPolicy Bypass -File "@@POSTINSTALL@@"'''

[tools."pg0"]
version = "latest"
asset_pattern = "pg0-windows-x86_64.exe"
format = "raw"

[env]
_.path = { path = [
    "{{ tools.git.path }}/usr/bin",
    "{{ tools.git.path }}/ucrt64/bin",
], tools = true, position = "append" }

[tool_alias]
git = "github:git-for-windows/git"
pg0 = "github:vectorize-io/pg0"
"##;

/// The shells the terminal button can open. bash and sh are only usable when an exe exists.
#[derive(Clone, Debug, Default, Serialize)]
pub struct TerminalInfo {
    pub bash: Option<String>,
    pub sh: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
pub struct GitExternal {
    pub path: String,
    pub version: Option<String>,
    pub from_mise: bool,
}

#[derive(Clone, Debug, Serialize)]
pub struct GitInfo {
    pub installed: bool,
    pub version: Option<String>,
    pub path: Option<String>,
    pub external: Option<GitExternal>,
}

fn git_installs(root: &Path) -> PathBuf {
    root.join("runtime").join("mise-data").join("installs").join("git")
}

/// The newest git that Autark's mise installed. The folder name is the version mise resolved.
fn own_git(root: &Path) -> Option<(PathBuf, String)> {
    let mut found: Vec<(PathBuf, String)> = fs::read_dir(git_installs(root))
        .ok()?
        .flatten()
        .filter(|entry| entry.path().is_dir())
        .filter_map(|entry| {
            let exe = entry.path().join("cmd").join("git.exe");
            exe.is_file().then(|| (exe, entry.file_name().to_string_lossy().to_string()))
        })
        .collect();
    found.sort_by(|a, b| section_cmp(&b.1, &a.1));
    found.into_iter().next()
}

/// Compares version folder names numerically so `2.10.0` sorts above `2.9.0`.
fn section_cmp(a: &str, b: &str) -> std::cmp::Ordering {
    let split = |text: &str| text.split(|c: char| !c.is_ascii_digit()).filter(|part| !part.is_empty()).map(|part| part.parse::<u64>().unwrap_or(0)).collect::<Vec<_>>();
    split(a).cmp(&split(b))
}

fn looks_mise_managed(path: &Path) -> bool {
    let key = path.to_string_lossy().replace('/', "\\").to_lowercase();
    key.contains("\\installs\\git\\") || key.contains("\\mise\\shims\\")
}

fn external_git(root: &Path) -> Option<GitExternal> {
    let ours = git_installs(root).to_string_lossy().replace('/', "\\").to_lowercase();
    for candidate in where_tool("git") {
        let key = candidate.to_string_lossy().replace('/', "\\").to_lowercase();
        if key.starts_with(&ours) {
            continue;
        }
        let version = new_command(&candidate)
            .arg("--version")
            .output()
            .ok()
            .filter(|output| output.status.success())
            .and_then(|output| String::from_utf8_lossy(&output.stdout).lines().next().map(|line| line.trim().trim_start_matches("git version ").to_string()));
        return Some(GitExternal {
            from_mise: looks_mise_managed(&candidate),
            path: candidate.to_string_lossy().to_string(),
            version,
        });
    }
    None
}

pub fn probe_terminal(root: &Path) -> TerminalInfo {
    // git ships its own bash and sh, so a built-in git is enough on its own.
    let git = own_git(root).and_then(|(exe, _)| exe.parent().and_then(|bin| bin.parent()).map(Path::to_path_buf));
    let from_git = |relative: &[&str]| {
        git.as_ref()
            .map(|dir| relative.iter().fold(dir.clone(), |path, part| path.join(part)))
            .filter(|path| path.is_file())
            .map(|path| path.display().to_string())
    };
    let on_path = |name: &str| {
        where_tool(name)
            .into_iter()
            .next()
            .map(|path| path.display().to_string())
    };
    TerminalInfo {
        bash: from_git(&["bin", "bash.exe"]).or_else(|| on_path("bash")),
        sh: from_git(&["bin", "sh.exe"])
            .or_else(|| from_git(&["usr", "bin", "sh.exe"]))
            .or_else(|| on_path("sh")),
    }
}

pub fn probe_git(settings: &Settings) -> GitInfo {
    let root = settings.root_path();
    let own = own_git(&root);
    GitInfo {
        installed: own.is_some(),
        version: own.as_ref().map(|(_, version)| version.clone()),
        path: own.map(|(exe, _)| exe.to_string_lossy().to_string()),
        external: external_git(&root),
    }
}

fn git_command(settings: &Settings, root: &Path) -> Command {
    let mut command = new_command(resolve(settings, root).unwrap_or_else(|| PathBuf::from("mise")));
    // The global config is written by Autark, so mise does not need to ask about trusting it.
    command.env("MISE_TRUSTED_CONFIG_PATHS", root.join("runtime").join("mise-config"));
    environment(&mut command, root);
    command.current_dir(root);
    command
}

pub fn install_git(app: &AppHandle, id: &str, settings: &Settings, root: &Path) -> Result<()> {
    if resolve(settings, root).is_none() {
        bail!("invalid_argument: mise is required before git can be installed");
    }
    for sub in ["mise-data", "mise-config", "mise-cache", "mise-state", "tmp"] {
        fs::create_dir_all(root.join("runtime").join(sub))?;
    }
    fs::write(global_config(root), git_config(root)).context("cannot write mise global config")?;
    fs::write(git_postinstall(root), GIT_POSTINSTALL_PS).context("cannot write git postinstall")?;
    let mut command = git_command(settings, root);
    command.args(["install", "git"]);
    apps::run_tool_streaming(app, id, "installing git".to_string(), command)?;
    if own_git(root).is_none() {
        bail!("internal: git did not land in Autark's mise installs");
    }
    Ok(())
}

pub fn update_git(app: &AppHandle, id: &str, settings: &Settings, root: &Path) -> Result<()> {
    if own_git(root).is_none() {
        bail!("invalid_argument: Autark's git is not installed");
    }
    let mut command = git_command(settings, root);
    command.args(["upgrade", "git"]);
    apps::run_tool_streaming(app, id, "updating git".to_string(), command)
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
    let output = new_command("powershell").args(["-NoProfile", "-ExecutionPolicy", "Bypass", "-Command", &script]).output()?;
    if !output.status.success() { bail!("{}", String::from_utf8_lossy(&output.stderr)); }
    Ok(registry_has_path(&bin_dir(root)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn git_outside_a_mise_install_is_third_party() {
        assert!(looks_mise_managed(Path::new(r"C:\Users\me\AppData\Local\mise\installs\git\2.50.1\cmd\git.exe")));
        assert!(!looks_mise_managed(Path::new(r"C:\Program Files\Git\cmd\git.exe")));
    }

    #[test]
    fn version_folders_sort_numerically() {
        assert_eq!(section_cmp("2.10.0", "2.9.0"), std::cmp::Ordering::Greater);
        assert_eq!(section_cmp("2.9.0", "2.10.0"), std::cmp::Ordering::Less);
    }
}
