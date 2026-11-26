use crate::{
    prefs::{self, Prefs},
    manifest::{self, AppSetting, Manifest, Requirements},
    marketplace,
    local,
    settings::{self, Settings},
    tools::{self, MiseInfo},
};
use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::{
    collections::HashMap,
    fs,
    io::{BufRead, BufReader, Read},
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::{Arc, Mutex},
    thread,
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tauri::{AppHandle, Emitter};

static CANCEL: Mutex<Vec<String>> = Mutex::new(Vec::new());

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Phase {
    NotInstalled,
    Installing,
    Installed,
    Starting,
    Running,
    Stopping,
    Updating,
    Removing,
    Failed,
}

impl Phase {
    pub fn is_busy(self) -> bool {
        matches!(
            self,
            Phase::Installing
                | Phase::Starting
                | Phase::Stopping
                | Phase::Updating
                | Phase::Removing
        )
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct ErrorInfo {
    pub code: String,
    pub message: String,
}

impl ErrorInfo {
    pub fn new(code: &str, message: impl Into<String>) -> Self {
        Self {
            code: code.to_string(),
            message: message.into(),
        }
    }

    pub fn from_error(error: &anyhow::Error) -> Self {
        let text = format!("{error:#}");
        const CODES: [&str; 7] = [
            "operation_in_progress",
            "approval_required",
            "confirm_required",
            "invalid_argument",
            "marketplace_missing",
            "app_not_found",
            "cancelled",
        ];
        for code in CODES {
            let prefix = format!("{code}: ");
            if let Some(message) = text.strip_prefix(&prefix) {
                return Self::new(code, message.to_string());
            }
        }
        if text.contains("cancelled by user") {
            return Self::new("cancelled", "operation cancelled");
        }
        Self::new("internal", text)
    }
}

#[derive(Clone, Debug, Serialize, Default)]
pub struct StepInfo {
    pub index: u32,
    pub total: u32,
    pub name: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct AppState {
    pub id: String,
    pub local: bool,
    pub name: String,
    pub description: String,
    pub version_installed: Option<String>,
    pub version_app: Option<String>,
    pub version_latest: Option<String>,
    pub upstream: Option<String>,
    pub update_available: bool,
    pub installed: bool,
    pub phase: Phase,
    pub settings: Map<String, Value>,
    pub prefs: Prefs,
    pub settings_schema: Vec<AppSetting>,
    /// Settings the machine environment owns, with the value that wins.
    pub settings_locked: Vec<SettingsLock>,
    /// Buttons the manifest declares for this app.
    pub actions: Vec<AppAction>,
    /// Whether the manifest declares a start command.
    pub can_start: bool,
    pub url: Option<String>,
    pub homepage: Option<String>,
    pub license: Option<String>,
    pub icon_path: Option<String>,
    pub icon_data: Option<String>,
    pub requirements: Requirements,
    pub faq: Vec<manifest::FaqItem>,
    pub last_error: Option<ErrorInfo>,
    pub step: Option<StepInfo>,
    pub started_at: Option<u64>,
    pub elapsed_secs: Option<u64>,
    /// The preview branch or pull request this installation came from; None means stable.
    pub preview: Option<String>,
    /// True when the app came from a preview and the marketplace is back on its default branch.
    pub restore_available: bool,
}

#[derive(Clone, Debug, Serialize)]
pub struct SettingsLock {
    pub key: String,
    pub var: String,
    pub value: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct AppAction {
    pub id: String,
    pub label: String,
    pub confirm: bool,
}

/// Settings the machine environment owns: any listed variable already set wins.
fn settings_locks(manifest: &Manifest) -> Vec<SettingsLock> {
    let mut locks = Vec::new();
    for setting in &manifest.settings {
        for name in &setting.locked_by {
            if let Ok(value) = std::env::var(name) {
                locks.push(SettingsLock {
                    key: setting.key.clone(),
                    var: name.clone(),
                    value,
                });
                break;
            }
        }
    }
    locks
}

#[derive(Clone, Debug)]
pub struct AppRecord {
    pub manifest: Manifest,
    pub local: bool,
    pub manifest_dir: PathBuf,
    pub settings: Map<String, Value>,
    pub prefs: Prefs,
    pub installed_version: Option<String>,
    pub app_version: Option<String>,
    pub version_latest: Option<String>,
    pub installed: bool,
    pub phase: Phase,
    pub url: Option<String>,
    pub last_error: Option<ErrorInfo>,
    pub step: Option<StepInfo>,
    pub started_at: Option<u64>,
    pub icon_data: Option<String>,
    pub preview: Option<String>,
}

impl AppRecord {
    fn from_manifest(root: &Path, manifest_dir: &Path, catalogue: Manifest, in_catalogue: bool, local: bool) -> Self {
        let app_dir = manifest::app_dir(root, &catalogue.id);
        let installed_record = manifest::read_installed(&app_dir);
        let preview = installed_record.as_ref().and_then(|record| record.source.clone());
        let version_latest = (in_catalogue && !local).then(|| catalogue.version.clone());
        // A preview install keeps the recipe it was installed from, because the
        // marketplace clone is reset to the default branch on the next start.
        let manifest = preview
            .as_ref()
            .and_then(|_| Manifest::load(&app_dir).ok())
            .unwrap_or(catalogue);
        let values = manifest::load_settings(&app_dir, &manifest);
        let prefs = prefs::load(&app_dir);
        let check = manifest
            .setup
            .installed_check
            .as_ref()
            .and_then(|template| manifest::expand(template, root, &app_dir, &values).ok())
            .map(|path| PathBuf::from(path).exists());
        let installed = installed_record.is_some() && check.unwrap_or(true);
        let icon_data = read_icon_data(&manifest, manifest_dir);
        Self {
            installed_version: installed_record
                .as_ref()
                .map(|record| record.version.clone())
                .filter(|_| installed),
            app_version: installed_record
                .as_ref()
                .and_then(|record| record.app_version.clone())
                .filter(|_| installed),
            version_latest,
            installed,
            phase: if installed {
                Phase::Installed
            } else {
                Phase::NotInstalled
            },
            url: None,
            last_error: None,
            step: None,
            started_at: None,
            settings: values,
            prefs,
            manifest_dir: manifest_dir.to_path_buf(),
            icon_data,
            preview: installed_record
                .as_ref()
                .and_then(|record| record.source.clone())
                .filter(|_| installed),
            manifest,
            local,
        }
    }

    fn update_available(&self) -> bool {
        !self.local && self.installed
            && matches!(
                (&self.installed_version, &self.version_latest),
                (Some(installed), Some(latest)) if newer(latest, installed)
            )
    }

    fn wire(&self, marketplace_source: &str) -> AppState {
        let elapsed_secs = self
            .started_at
            .map(|started| now().saturating_sub(started));
        AppState {
            id: self.manifest.id.clone(),
            local: self.local,
            name: self.manifest.name.clone(),
            description: self.manifest.description.clone(),
            version_installed: self.installed_version.clone(),
            version_app: self.app_version.clone(),
            version_latest: self.version_latest.clone(),
            upstream: self.manifest.upstream.clone(),
            update_available: self.update_available(),
            installed: self.installed,
            phase: self.phase,
            settings: self.settings.clone(),
            prefs: self.prefs.clone(),
            settings_schema: self.manifest.settings.clone(),
            settings_locked: settings_locks(&self.manifest),
            actions: self.manifest.actions.iter().map(|action| AppAction {
                id: action.id.clone(),
                label: action.label.clone(),
                confirm: action.confirm,
            }).collect(),
            can_start: !self.manifest.launch.run.is_empty(),
            url: self.url.clone(),
            homepage: self.manifest.homepage.clone(),
            license: self.manifest.license.clone(),
            icon_path: self.manifest.icon_path(&self.manifest_dir),
            icon_data: self.icon_data.clone(),
            requirements: self.manifest.requirements.clone(),
            faq: self.manifest.faq.clone(),
            last_error: self.last_error.clone(),
            step: self.step.clone(),
            started_at: self.started_at,
            elapsed_secs,
            preview: self.preview.clone(),
            restore_available: self.preview.is_some() && marketplace_source.is_empty(),
        }
    }
}

/// Only a strictly newer recipe may overwrite an installed app.
fn newer(latest: &str, installed: &str) -> bool {
    fn parts(value: &str) -> Vec<String> {
        value
            .trim()
            .trim_start_matches(|c| c == 'v' || c == 'V')
            .split(|c: char| !c.is_ascii_alphanumeric())
            .filter(|part| !part.is_empty())
            .map(str::to_ascii_lowercase)
            .collect()
    }
    let (left, right) = (parts(latest), parts(installed));
    for (a, b) in left.iter().zip(right.iter()) {
        let order = match (a.parse::<u64>(), b.parse::<u64>()) {
            (Ok(a), Ok(b)) => a.cmp(&b),
            _ => a.cmp(b),
        };
        if !order.is_eq() {
            return order.is_gt();
        }
    }
    left.len() > right.len()
}

fn read_icon_data(manifest: &Manifest, manifest_dir: &Path) -> Option<String> {
    use base64::Engine;
    let path = PathBuf::from(manifest.icon_path(manifest_dir)?);
    let bytes = std::fs::read(&path).ok()?;
    let mime = match path
        .extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| ext.to_ascii_lowercase())
        .as_deref()
    {
        Some("svg") => "image/svg+xml",
        Some("ico") => "image/x-icon",
        Some("jpg") | Some("jpeg") => "image/jpeg",
        Some("webp") => "image/webp",
        _ => "image/png",
    };
    Some(format!(
        "data:{};base64,{}",
        mime,
        base64::engine::general_purpose::STANDARD.encode(bytes)
    ))
}

#[derive(Clone, Debug, Serialize, Default)]
pub struct MarketplaceInfo {    pub path: String,
    pub url: String,
    pub exists: bool,
    pub refreshing: bool,
    pub last_refresh: Option<u64>,
    pub error: Option<ErrorInfo>,
    /// The session's preview branch or pull request; empty means the default branch.
    pub source: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct Operation {
    pub app_id: String,
    pub kind: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct State {
    pub settings: Settings,
    pub settings_path: String,
    pub settings_exists: bool,
    pub marketplace: MarketplaceInfo,
    pub apps: Vec<AppState>,
    pub operations: Vec<Operation>,
    pub busy: bool,
    pub mise: MiseInfo,
    pub git: tools::GitInfo,
    pub terminal: tools::TerminalInfo,
}

#[derive(Clone, Debug, Serialize)]
pub struct LogLine {
    pub app_id: String,
    pub line: String,
}

pub struct Core {
    pub settings: Settings,
    pub records: Vec<AppRecord>,
    pub marketplace: MarketplaceInfo,
    pub mise: MiseInfo,
    pub git: tools::GitInfo,
    pub terminal: tools::TerminalInfo,
}

pub struct Shared {
    pub core: Mutex<Core>,
    pub operations: Mutex<HashMap<String, Operation>>,
    pub children: Mutex<HashMap<String, Child>>,
}

impl Shared {
    pub fn new() -> Self {
        let settings = Settings::load_or_default();
        let root = settings.root_path();
        let mut core = Core {
            settings: settings.clone(),
            mise: tools::probe(&settings),
            git: tools::probe_git(&settings),
            terminal: tools::probe_terminal(&root),
            records: Vec::new(),
            marketplace: MarketplaceInfo {
                path: marketplace::dir(&root).to_string_lossy().to_string(),
                url: settings::MARKETPLACE_URL.to_string(),
                exists: false,
                refreshing: false,
                last_refresh: settings.marketplace_last_refresh,
                error: None,
                source: settings.marketplace_source.clone(),
            },
        };
        // The selected source is remembered, so the clone follows it on every start.
        let applied = marketplace::applied_source(&root);
        if !settings.marketplace_source.is_empty() || !applied.is_empty() {
            if let Err(error) = marketplace::sync(&root, &settings.marketplace_source) {
                core.marketplace.error = Some(ErrorInfo::from_error(&error));
            }
        }
        load_catalogue(&mut core);
        Self {
            core: Mutex::new(core),
            operations: Mutex::new(HashMap::new()),
            children: Mutex::new(HashMap::new()),
        }
    }

    pub fn root(&self) -> PathBuf {
        self.core
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .settings
            .root_path()
    }

    pub fn plugin_settings(&self) -> Settings {
        self.core
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .settings
            .clone()
    }
}

/// Quotes a logged argument so a multi-line payload cannot swallow the flags after it.
fn echo_arg(arg: &str) -> String {
    if !arg.contains([' ', '\t', '\n', '"']) {
        return arg.to_string();
    }
    let quoted = format!("\"{}\"", arg.replace('"', "\\\""));
    if arg.contains('\n') {
        format!("{quoted}\n")
    } else {
        quoted
    }
}

/// Frees the app's port when the listener is an orphaned process from the app folder.
/// A process outside the app folder is left alone, so an unrelated service is never killed.
fn reclaim_own_port(url: &str, app_dir: &Path, app_id: &str) -> Result<Option<u16>> {
    let parsed = url
        .split("://")
        .nth(1)
        .and_then(|rest| rest.split('/').next())
        .and_then(|host| host.rsplit(':').next())
        .and_then(|port| port.parse::<u16>().ok());
    let Some(port) = parsed else {
        return Ok(None);
    };
    let script = format!(
        "$c=Get-NetTCPConnection -LocalPort {port} -State Listen -ErrorAction SilentlyContinue | Select-Object -First 1; if ($c) {{ $p=Get-Process -Id $c.OwningProcess -ErrorAction SilentlyContinue; $w=Get-CimInstance Win32_Process -Filter \"ProcessId=$($c.OwningProcess)\" -ErrorAction SilentlyContinue; Write-Output \"$($c.OwningProcess)|$($p.Path)|$($w.CommandLine)\" }}"
    );
    let Ok(output) = std::process::Command::new("powershell")
        .args(["-NoProfile", "-Command", &script])
        .output()
    else {
        return Ok(None);
    };
    let text = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let mut parts = text.split('|');
    let Some(pid) = parts.next() else {
        return Ok(None);
    };
    if pid.trim().is_empty() {
        return Ok(None);
    }
    let path = parts.next().unwrap_or("").to_lowercase();
    let command_line = parts.next().unwrap_or("").to_lowercase();
    let dir = app_dir.to_string_lossy().to_lowercase();
    let app = app_id.to_lowercase();
    // Only app-specific evidence proves ownership; an unknown owner is reported, never killed.
    let owned = !dir.is_empty()
        && (path.starts_with(&dir)
            || command_line.contains(&dir)
            || (!app.is_empty() && command_line.contains(&app)));
    if !owned {
        let name = if path.is_empty() { "unknown process".to_string() } else { path.clone() };
        bail!(
            "port {port} is held by PID {pid} ({name}). Stop it or change the port in the app settings."
        );
    }
    let _ = std::process::Command::new("taskkill")
        .args(["/PID", pid, "/F"])
        .output();
    Ok(Some(port))
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_secs())
        .unwrap_or(0)
}

/// Rebuilds the catalogue from the marketplace clone plus installed app snapshots.
pub fn load_catalogue(core: &mut Core) -> Vec<String> {
    let root = core.settings.root_path();
    let market_dir = marketplace::dir(&root);
    core.marketplace.path = market_dir.to_string_lossy().to_string();
    core.marketplace.url = settings::MARKETPLACE_URL.to_string();
    core.marketplace.exists = market_dir.join("marketplace.json").exists();

    let mut warnings = Vec::new();
    let mut discovered: Vec<(PathBuf, Manifest, bool, bool)> = Vec::new();
    if core.marketplace.exists {
        match marketplace::scan(&root) {
            Ok(result) => {
                warnings = result.warnings;
                core.marketplace.error = None;
                for scanned in result.manifests {
                    discovered.push((scanned.dir, scanned.manifest, true, scanned.local));
                }
            }
            Err(error) => core.marketplace.error = Some(ErrorInfo::from_error(&error)),
        }
    }

    let apps_root = root.join("apps");
    if let Ok(entries) = fs::read_dir(&apps_root) {
        for entry in entries.flatten() {
            if !entry.path().is_dir() {
                continue;
            }
            let id = entry.file_name().to_string_lossy().to_string();
            if discovered.iter().any(|(_, manifest, _, _)| manifest.id == id) {
                continue;
            }
            match Manifest::load(&entry.path()) {
                Ok(manifest) => {
                    let local = local::dir(&root).join(&id).is_dir();
                    discovered.push((entry.path(), manifest, false, local));
                }
                Err(_) => continue,
            }
        }
    }

    // Local apps must stay visible even when the marketplace clone is missing.
    if let Ok(entries) = fs::read_dir(local::dir(&root)) {
        for entry in entries.flatten() {
            if !entry.path().is_dir() {
                continue;
            }
            let id = entry.file_name().to_string_lossy().to_string();
            if discovered.iter().any(|(_, manifest, _, _)| manifest.id == id) {
                continue;
            }
            match Manifest::load(&entry.path()) {
                Ok(manifest) => discovered.push((entry.path(), manifest, false, true)),
                Err(error) => warnings.push(format!("{id}: {error:#}")),
            }
        }
    }

    // One marketplace list: the list keeps the order the user already sees, and an app
    // known from a previously selected source stays visible when the current one omits it.
    let order: Vec<String> = core
        .records
        .iter()
        .map(|record| record.manifest.id.clone())
        .collect();
    let mut previous: HashMap<String, AppRecord> = core
        .records
        .drain(..)
        .map(|record| (record.manifest.id.clone(), record))
        .collect();
    discovered.sort_by_key(|(_, manifest, _, _)| {
        order
            .iter()
            .position(|id| id == &manifest.id)
            .unwrap_or(usize::MAX)
    });

    let mut records = Vec::with_capacity(discovered.len() + previous.len());
    for (dir, manifest, in_catalogue, local) in discovered {
        let mut record = AppRecord::from_manifest(&root, &dir, manifest, in_catalogue, local);
        if let Some(old) = previous.remove(&record.manifest.id) {
            if old.phase.is_busy() || old.phase == Phase::Running {
                record.phase = old.phase;
                record.url = old.url.clone();
                record.last_error = old.last_error.clone();
                record.step = old.step.clone();
                record.started_at = old.started_at;
                record.installed = old.installed;
                record.installed_version = old.installed_version.clone();
                record.app_version = old.app_version.clone();
            }
        }
        records.push(record);
    }
    for id in &order {
        if let Some(record) = previous.remove(id) {
            records.push(record);
        }
    }
    core.records = records;
    warnings
}

pub fn wire(shared: &Shared) -> State {
    let operations: Vec<Operation> = shared
        .operations
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .values()
        .cloned()
        .collect();
    let core = shared.core.lock().unwrap_or_else(|error| error.into_inner());
    let busy = !operations.is_empty() || core.marketplace.refreshing;
    State {
        settings: core.settings.clone(),
        settings_path: settings::settings_path().to_string_lossy().to_string(),
        settings_exists: settings::settings_path().exists(),
        marketplace: core.marketplace.clone(),
        apps: core.records.iter().map(|record| record.wire(&core.marketplace.source)).collect(),
        operations,
        busy,
        mise: core.mise.clone(),
        git: core.git.clone(),
        terminal: core.terminal.clone(),
    }
}

pub fn emit_state(app: &AppHandle, shared: &Shared) {
    let _ = app.emit("state", wire(shared));
}

pub fn log(app: &AppHandle, app_id: &str, line: impl Into<String>) {
    let _ = app.emit(
        "log",
        LogLine {
            app_id: app_id.to_string(),
            line: line.into(),
        },
    );
}

fn mutate(app: &AppHandle, shared: &Shared, id: &str, update: impl FnOnce(&mut AppRecord)) {
    {
        let mut core = shared.core.lock().unwrap_or_else(|error| error.into_inner());
        if let Some(record) = core
            .records
            .iter_mut()
            .find(|record| record.manifest.id == id)
        {
            update(record);
        }
    }
    emit_state(app, shared);
}

fn find_record(shared: &Shared, id: &str) -> Result<AppRecord> {
    let core = shared.core.lock().unwrap_or_else(|error| error.into_inner());
    core.records
        .iter()
        .find(|record| record.manifest.id == id)
        .cloned()
        .with_context(|| format!("app_not_found: {id}"))
}

pub fn reserve(shared: &Shared, app_id: &str, kind: &str) -> Result<()> {
    let mut operations = shared
        .operations
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    let setup = matches!(kind, "install" | "update");
    if operations.contains_key(app_id) {
        bail!("operation_in_progress: this app is already busy");
    }
    if kind == "update_all" || setup {
        if operations.values().any(|op| {
            op.kind == "update_all" || matches!(op.kind.as_str(), "install" | "update")
        }) {
            bail!("operation_in_progress: another setup is running");
        }
    }
    operations.insert(
        app_id.to_string(),
        Operation {
            app_id: app_id.to_string(),
            kind: kind.to_string(),
        },
    );
    clear_cancel(app_id);
    Ok(())
}

pub fn release(shared: &Shared, app_id: &str) {
    if let Ok(mut operations) = shared.operations.lock() {
        operations.remove(app_id);
    }
    clear_cancel(app_id);
}

pub fn cancel(app_id: &str) {
    let mut cancelled = CANCEL.lock().unwrap_or_else(|error| error.into_inner());
    if !cancelled.iter().any(|id| id == app_id) {
        cancelled.push(app_id.to_string());
    }
}

fn clear_cancel(app_id: &str) {
    let mut cancelled = CANCEL.lock().unwrap_or_else(|error| error.into_inner());
    cancelled.retain(|id| id != app_id);
}

fn is_cancelled(app_id: &str) -> bool {
    CANCEL
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .iter()
        .any(|id| id == app_id)
}

fn check_cancel(app_id: &str) -> Result<()> {
    if is_cancelled(app_id) {
        bail!("cancelled: operation cancelled");
    }
    Ok(())
}

fn set_phase(app: &AppHandle, shared: &Shared, id: &str, phase: Phase) {
    mutate(app, shared, id, |record| {
        record.phase = phase;
        if phase != Phase::Running && !phase.is_busy() {
            record.step = None;
        }
        if phase == Phase::Failed {
            record.url = None;
            record.started_at = None;
        }
    });
}

fn step_begin(app: &AppHandle, shared: &Shared, id: &str, index: u32, total: u32, name: &str) {
    log(app, id, format!("[{index}/{total}] {name}"));
    mutate(app, shared, id, |record| {
        record.step = Some(StepInfo {
            index,
            total,
            name: name.to_string(),
        })
    });
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum SetupKind {
    Install,
    Update,
}

impl SetupKind {
    fn phase(self) -> Phase {
        match self {
            SetupKind::Install => Phase::Installing,
            SetupKind::Update => Phase::Updating,
        }
    }
}

pub fn setup(app: &AppHandle, shared: &Shared, id: &str, kind: SetupKind) -> Result<()> {
    let snapshot = find_record(shared, id)?;
    if snapshot.phase == Phase::Running || snapshot.phase == Phase::Starting {
        bail!("stop the app before running setup");
    }
    let mut manifest = snapshot.manifest.clone();
    let mut values = snapshot.settings.clone();
    let core_settings = shared
        .core
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .settings
        .clone();
    let marketplace_source = {
        let core = shared.core.lock().unwrap_or_else(|error| error.into_inner());
        core.marketplace.source.clone()
    };
    let root = core_settings.root_path();
    let force_local_mise = core_settings.force_local_mise;
    let app_dir = manifest::app_dir(&root, id);
    // A preview recipe is kept after a switch to stable, but setup must run the
    // recipe of the active source, otherwise new settings never appear.
    if marketplace_source.is_empty() && snapshot.preview.is_some() {
        if let Ok(stable) = Manifest::load(&snapshot.manifest_dir) {
            values = manifest::load_settings(&app_dir, &stable);
            manifest = stable;
        }
    }

    set_phase(app, shared, id, kind.phase());
    mutate(app, shared, id, |record| {
        record.last_error = None;
        record.url = None;
        record.started_at = None;
    });

    let mut total = 2u32 + manifest.setup.steps.len() as u32;
    if !manifest.setup.tools.is_empty() {
        total += 2;
    }
    if manifest.setup.source.is_some() {
        total += 1;
    }
    let mut index = 0u32;

    index += 1;
    step_begin(app, shared, id, index, total, "Preparing folders");
    let runtime = root.join("runtime");
    for sub in [
        "tmp",
        "mise-data",
        "mise-config",
        "mise-cache",
        "mise-state",
        "bin",
    ] {
        fs::create_dir_all(runtime.join(sub))?;
    }
    fs::create_dir_all(tools::bin_dir(&root))?;
    for sub in [
        "app",
        "user-data/roaming",
        "user-data/local",
        "runtime/tmp",
        "runtime/uv-cache",
    ] {
        fs::create_dir_all(app_dir.join(sub))?;
    }
    manifest::save_settings(&app_dir, &values)?;
    manifest::snapshot_manifest(&app_dir, &manifest)?;
    // Copy the recipe icon into the app folder so the card keeps it when the
    // marketplace clone switches to another source.
    if let Some(icon) = manifest.icon_path(&snapshot.manifest_dir) {
        let icon = PathBuf::from(icon);
        if let Some(name) = icon.file_name() {
            let target = app_dir.join(name);
            if icon.is_file() && icon != target {
                fs::copy(&icon, &target)?;
            }
        }
    }
    check_cancel(id)?;

    if !manifest.setup.tools.is_empty() {
        index += 1;
        step_begin(app, shared, id, index, total, "Writing mise.toml");
        write_mise_toml(&app_dir, &manifest)?;
        check_cancel(id)?;

        index += 1;
        step_begin(app, shared, id, index, total, "Installing tools");
        ensure_mise(app, id, &root, force_local_mise)?;
        let mut command = mise_command(&root, force_local_mise);
        command.arg("--cd").arg(&app_dir).arg("install");
        apply_environment(&mut command, &root, &app_dir, &manifest, &values)?;
        command.current_dir(&app_dir);
        run_streaming(app, id, "mise install".to_string(), command)?;
    }

    if let Some(source) = manifest.setup.source.clone() {
        index += 1;
        step_begin(app, shared, id, index, total, "Fetching source");
        sync_source(app, id, &root, &app_dir, &manifest, &values, &source)?;
    }

    for step in manifest.setup.steps.clone() {
        if let Some(key) = &step.when_setting {
            if values.get(key) != Some(&Value::Bool(true)) {
                log(app, id, format!("skipping setup step `{}`: {key} is off", step.name));
                continue;
            }
        }
        index += 1;
        step_begin(app, shared, id, index, total, &step.name);
        match run_step(
            app,
            id,
            &root,
            &app_dir,
            &manifest,
            &values,
            &step,
            force_local_mise,
        ) {
            Ok(()) => {}
            Err(error) if step.optional => {
                log(app, id, format!("optional step failed: {error:#}"));
            }
            Err(error) => return Err(error),
        }
    }

    index += 1;
    step_begin(app, shared, id, index, total, "Finishing");
    let app_version = read_app_version(app, id, &root, &app_dir, &manifest, &values, force_local_mise);
    if let Some(check) = &manifest.setup.installed_check {
        let path = PathBuf::from(manifest::expand(check, &root, &app_dir, &values)?);
        if !path.exists() {
            bail!(
                "the app did not land in its folder: {} is missing. A system environment variable may have redirected the installer.",
                path.display()
            );
        }
    }
    manifest::write_installed(&app_dir, &manifest.version, app_version.as_deref(), &marketplace_source)?;
    let version = manifest.version.clone();
    let preview = (!marketplace_source.is_empty()).then(|| marketplace_source.clone());
    mutate(app, shared, id, |record| {
        record.installed = true;
        record.installed_version = Some(version);
        record.app_version = app_version;
        record.preview = preview;
        record.phase = Phase::Installed;
        record.step = None;
        record.last_error = None;
        record.url = None;
        record.started_at = None;
    });
    log(app, id, "app is ready");
    Ok(())
}

pub fn install(app: &AppHandle, shared: &Shared, id: &str, kind: SetupKind) -> Result<()> {
    let result = setup(app, shared, id, kind);
    if let Err(error) = &result {
        let info = ErrorInfo::from_error(error);
        mutate(app, shared, id, |record| {
            record.phase = Phase::Failed;
            record.last_error = Some(info);
            record.step = None;
        });
    }
    result
}

fn write_mise_toml(app_dir: &Path, manifest: &Manifest) -> Result<()> {
    let mut text =
        String::from("# Managed by Autark. Change tools in the marketplace manifest instead.\n[tools]\n");
    for (tool, version) in &manifest.setup.tools {
        text.push_str(&format!("{tool} = \"{version}\"\n"));
    }
    fs::write(app_dir.join("mise.toml"), text).context("cannot write mise.toml")
}

fn run_step(
    app: &AppHandle,
    id: &str,
    root: &Path,
    app_dir: &Path,
    manifest: &Manifest,
    values: &Map<String, Value>,
    step: &manifest::Step,
    force_local_mise: bool,
) -> Result<()> {
    let program = manifest::expand(&step.run[0], root, app_dir, values)?;
    let args = step.run[1..]
        .iter()
        .map(|arg| manifest::expand(arg, root, app_dir, values))
        .collect::<Result<Vec<_>>>()?;
    let mut command = if manifest.setup.tools.is_empty() {
        tools::new_command(&program)
    } else {
        let mut command = mise_command(root, force_local_mise);
        command
            .arg("--cd")
            .arg(app_dir)
            .arg("exec")
            .arg("--")
            .arg(&program);
        command
    };
    command.args(&args);
    apply_environment(&mut command, root, app_dir, manifest, values)?;
    command.current_dir(app_dir);
    let mut label = program;
    if !args.is_empty() {
        label.push(' ');
        label.push_str(&args.join(" "));
    }
    run_streaming(app, id, label, command)
}

fn sync_source(
    app: &AppHandle,
    id: &str,
    root: &Path,
    app_dir: &Path,
    manifest: &Manifest,
    values: &Map<String, Value>,
    source: &manifest::Source,
) -> Result<()> {
    let dir = app_dir.join("source");
    if dir.join(".git").exists() {
        for args in [
            vec!["fetch", "--depth", "1", "origin", source.branch.as_str()],
            vec!["reset", "--hard", "FETCH_HEAD"],
            vec!["clean", "-fd"],
        ] {
            let mut command = tools::new_command("git");
            command.arg("-C").arg(&dir).args(&args);
            apply_environment(&mut command, root, app_dir, manifest, values)?;
            run_streaming(app, id, format!("git {}", args.join(" ")), command)?;
        }
    } else {
        if dir.exists() {
            fs::remove_dir_all(&dir).context("cannot clear a partial source checkout")?;
        }
        let target = dir.to_string_lossy().to_string();
        let mut command = tools::new_command("git");
        command.args([
            "clone",
            "--depth",
            "1",
            "--branch",
            source.branch.as_str(),
            source.url.as_str(),
            target.as_str(),
        ]);
        apply_environment(&mut command, root, app_dir, manifest, values)?;
        run_streaming(app, id, "git clone".to_string(), command)?;
    }
    Ok(())
}

fn read_app_version(
    app: &AppHandle,
    id: &str,
    root: &Path,
    app_dir: &Path,
    manifest: &Manifest,
    values: &Map<String, Value>,
    force_local_mise: bool,
) -> Option<String> {
    let check = manifest.setup.version_check.as_ref()?;
    let program = manifest::expand(&check[0], root, app_dir, values).ok()?;
    let args = check[1..]
        .iter()
        .map(|arg| manifest::expand(arg, root, app_dir, values).ok())
        .collect::<Option<Vec<_>>>()?;
    let mut command = if manifest.setup.tools.is_empty() {
        tools::new_command(&program)
    } else {
        let mut command = mise_command(root, force_local_mise);
        command
            .arg("--cd")
            .arg(app_dir)
            .arg("exec")
            .arg("--")
            .arg(&program);
        command
    };
    command.args(&args);
    apply_environment(&mut command, root, app_dir, manifest, values).ok()?;
    command.current_dir(app_dir).stdin(Stdio::null());
    let output = command.output().ok()?;
    if !output.status.success() {
        log(app, id, "reading the app version failed");
        return None;
    }
    let text = String::from_utf8_lossy(&output.stdout);
    let version = text
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .next_back()?
        .to_string();
    log(app, id, format!("app version {version}"));
    Some(version)
}

fn mise_command(root: &Path, force_local: bool) -> Command {
    let settings = Settings { root: root.to_string_lossy().to_string(), force_local_mise: force_local, ..Settings::load_or_default() };
    tools::new_command(tools::resolve(&settings, root).unwrap_or_else(|| PathBuf::from("mise")))
}

fn mise_available(root: &Path, force_local: bool) -> bool {
    let settings = Settings { root: root.to_string_lossy().to_string(), force_local_mise: force_local, ..Settings::load_or_default() };
    tools::probe(&settings).available
}

fn ensure_mise(app: &AppHandle, id: &str, root: &Path, force_local: bool) -> Result<()> {
    if mise_available(root, force_local) {
        return Ok(());
    }
    if std::env::var_os("MISE_BIN").is_some() && !force_local {
        bail!("invalid_argument: MISE_BIN does not point to a working mise");
    }
    log(app, id, "mise was not found, downloading a local copy");
    tools::install_local(app, id, root)
}

pub fn run_tool_streaming(app: &AppHandle, id: &str, label: String, command: Command) -> Result<()> {
    run_streaming(app, id, label, command)
}

pub fn refresh_mise(shared: &Arc<Shared>) {
    let mut core = shared.core.lock().unwrap_or_else(|error| error.into_inner());
    core.mise = tools::probe(&core.settings);
    core.git = tools::probe_git(&core.settings);
    core.terminal = tools::probe_terminal(&core.settings.root_path());
}

fn apply_environment(
    command: &mut Command,
    root: &Path,
    app_dir: &Path,
    manifest: &Manifest,
    values: &Map<String, Value>,
) -> Result<()> {
    command.env_clear();
    const PASSTHROUGH: [&str; 15] = [
        "PATH",
        "SystemRoot",
        "WINDIR",
        "COMSPEC",
        "USERPROFILE",
        "HOMEDRIVE",
        "HOMEPATH",
        "PATHEXT",
        "SystemDrive",
        "ProgramData",
        "ProgramFiles",
        "ProgramFiles(x86)",
        "NUMBER_OF_PROCESSORS",
        "USERNAME",
        "PROCESSOR_ARCHITECTURE",
    ];
    for key in PASSTHROUGH {
        if let Some(value) = std::env::var_os(key) {
            command.env(key, value);
        }
    }
    let runtime = root.join("runtime");
    let user_data = app_dir.join("user-data");
    let temp = app_dir.join("runtime").join("tmp");
    command
        .env("HOME", app_dir.join("app"))
        .env("APPDATA", user_data.join("roaming"))
        .env("LOCALAPPDATA", user_data.join("local"))
        .env("TEMP", &temp)
        .env("TMP", &temp)
        .env("UV_CACHE_DIR", app_dir.join("runtime").join("uv-cache"))
        .env("MISE_DATA_DIR", runtime.join("mise-data"))
        .env("MISE_CONFIG_DIR", runtime.join("mise-config"))
        .env("MISE_CACHE_DIR", runtime.join("mise-cache"))
        .env("MISE_STATE_DIR", runtime.join("mise-state"))
        .env("MISE_TMP_DIR", runtime.join("tmp"));
    // A var listed in `env_keep_parent` keeps the parent value when the machine
    // defines that exact name, so a machine-wide HF_HOME is never shadowed while
    // the other listed names still come from the manifest.
    let kept: Vec<&String> = manifest
        .env_keep_parent
        .iter()
        .filter(|name| std::env::var_os(name).is_some())
        .collect();
    for name in &kept {
        if let Some(value) = std::env::var_os(name) {
            command.env(name, value);
        }
    }
    for (key, value) in &manifest.env {
        if kept.iter().any(|name| *name == key) {
            continue;
        }
        command.env(key, manifest::expand(value, root, app_dir, values)?);
    }
    Ok(())
}

fn run_streaming(
    app: &AppHandle,
    id: &str,
    label: String,
    mut command: Command,
) -> Result<()> {
    log(app, id, format!("$ {label}"));
    command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = command
        .spawn()
        .with_context(|| format!("cannot start `{label}`"))?;
    let stdout = child.stdout.take();
    let stderr = child.stderr.take();
    let mut readers = Vec::new();
    if let Some(stream) = stdout {
        let handle = app.clone();
        let app_id = id.to_string();
        readers.push(thread::spawn(move || {
            stream_lines(stream, false, &handle, &app_id)
        }));
    }
    if let Some(stream) = stderr {
        let handle = app.clone();
        let app_id = id.to_string();
        readers.push(thread::spawn(move || {
            stream_lines(stream, true, &handle, &app_id)
        }));
    }
    let status = loop {
        if is_cancelled(id) {
            let pid = child.id().to_string();
            let _ = tools::new_command("taskkill")
                .args(["/PID", &pid, "/T", "/F"])
                .output();
            let _ = child.kill();
            let _ = child.wait();
            for reader in readers {
                let _ = reader.join();
            }
            bail!("cancelled: operation cancelled");
        }
        if let Some(status) = child.try_wait()? {
            break status;
        }
        thread::sleep(Duration::from_millis(250));
    };
    for reader in readers {
        let _ = reader.join();
    }
    if !status.success() {
        bail!("child command failed: {status}");
    }
    Ok(())
}

fn stream_lines<R: Read>(stream: R, is_error: bool, app: &AppHandle, app_id: &str) {
    let mut reader = BufReader::new(stream);
    let mut buffer = Vec::new();
    loop {
        buffer.clear();
        match reader.read_until(b'\n', &mut buffer) {
            Ok(0) | Err(_) => break,
            Ok(_) => {}
        }
        let text = String::from_utf8_lossy(&buffer).to_string();
        for line in text.split(['\r', '\n']) {
            let line = strip_ansi(line);
            let line = line.trim_end();
            if line.trim().is_empty() {
                continue;
            }
            log(
                app,
                app_id,
                if is_error {
                    format!("! {line}")
                } else {
                    line.to_string()
                },
            );
        }
    }
}

fn strip_ansi(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars();
    while let Some(ch) = chars.next() {
        if ch == '\u{1b}' {
            if chars.next() == Some('[') {
                for next in chars.by_ref() {
                    if next.is_ascii_alphabetic() {
                        break;
                    }
                }
            }
            continue;
        }
        out.push(ch);
    }
    out
}

fn kill_child(shared: &Shared, id: &str) {
    let child = shared
        .children
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .remove(id);
    if let Some(mut child) = child {
        let pid = child.id().to_string();
        let _ = tools::new_command("taskkill")
            .args(["/PID", &pid, "/T", "/F"])
            .output();
        let _ = child.kill();
        let _ = child.wait();
    }
}

pub fn start(app: &AppHandle, shared: &Arc<Shared>, id: &str) -> Result<()> {
    reserve(shared, id, "start")?;
    let result = start_inner(app, shared, id);
    release(shared, id);
    result
}

pub fn run_action(app: &AppHandle, shared: &Arc<Shared>, id: &str, action_id: &str) -> Result<()> {
    let record = find_record(shared, id)?;
    let action = record.manifest.actions.iter().find(|action| action.id == action_id)
        .cloned().ok_or_else(|| anyhow::anyhow!("invalid_argument: unknown action"))?;
    reserve(shared, id, "action")?;
    let result = (|| {
        let settings = shared.plugin_settings();
        let root = settings.root_path();
        let app_dir = manifest::app_dir(&root, id);
        let values = record.settings.clone();
        let args = action.run.iter()
            .map(|arg| manifest::expand(arg, &root, &app_dir, &values))
            .collect::<Result<Vec<_>>>()?;
        let (program, args) = args.split_first()
            .ok_or_else(|| anyhow::anyhow!("invalid_argument: unknown action"))?;
        let mut command = if record.manifest.setup.tools.is_empty() {
            let mut command = tools::new_command(program);
            command.args(args);
            command
        } else {
            let mut command = mise_command(&root, settings.force_local_mise);
            command.arg("--cd").arg(&app_dir).arg("exec").arg("--").arg(program).args(args);
            command
        };
        apply_environment(&mut command, &root, &app_dir, &record.manifest, &values)?;
        command.current_dir(&app_dir);
        let mut label = program.clone();
        if !args.is_empty() {
            label.push(' ');
            label.push_str(&args.join(" "));
        }
        run_streaming(app, id, label, command)?;
        if let Some(url) = action.url {
            let url = manifest::expand(&url, &root, &app_dir, &values)?;
            if let Err(error) = tauri_plugin_opener::open_url(&url, None::<&str>) {
                log(app, id, format!("could not open action URL: {error}"));
            }
        }
        Ok(())
    })();
    release(shared, id);
    emit_state(app, shared);
    result
}

fn start_inner(app: &AppHandle, shared: &Arc<Shared>, id: &str) -> Result<()> {
    let record = find_record(shared, id)?;
    if !record.installed {
        bail!("invalid_argument: install the app first");
    }
    if record.phase == Phase::Running {
        return Ok(());
    }
    if record.manifest.launch.run.is_empty() {
        bail!("invalid_argument: this app has no start command");
    }
    let settings = shared.plugin_settings();
    let root = settings.root_path();
    let force_local_mise = settings.force_local_mise;
    let app_dir = manifest::app_dir(&root, id);
    let manifest = record.manifest.clone();
    let values = record.settings.clone();

    let program = manifest::expand(&manifest.launch.run[0], &root, &app_dir, &values)?;
    let args = manifest.launch.run[1..]
        .iter()
        .map(|arg| manifest::expand(arg, &root, &app_dir, &values))
        .collect::<Result<Vec<_>>>()?;
    let mut command = if manifest.setup.tools.is_empty() {
        tools::new_command(&program)
    } else {
        let mut command = mise_command(&root, force_local_mise);
        command
            .arg("--cd")
            .arg(&app_dir)
            .arg("exec")
            .arg("--")
            .arg(&program);
        command
    };
    command.args(&args);
    apply_environment(&mut command, &root, &app_dir, &manifest, &values)?;
    command
        .current_dir(&app_dir)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    let mut echo = program.clone();
    for arg in &args {
        if !echo.ends_with('\n') {
            echo.push(' ');
        }
        echo.push_str(&echo_arg(arg));
    }
    log(app, id, format!("$ {echo}"));

    if let Some(template) = manifest.launch.url.clone() {
        if let Ok(url) = manifest::expand(&template, &root, &app_dir, &values) {
            match reclaim_own_port(&url, &app_dir, id) {
                Ok(Some(port)) => log(app, id, format!("released port {port} from a stale app process")),
                Ok(None) => {}
                Err(error) => return Err(error),
            }
        }
    }

    set_phase(app, shared, id, Phase::Starting);
    let mut child = command
        .spawn()
        .with_context(|| format!("cannot start {}", manifest.name))?;
    if let Some(stream) = child.stdout.take() {
        let handle = app.clone();
        let app_id = id.to_string();
        thread::spawn(move || stream_lines(stream, false, &handle, &app_id));
    }
    if let Some(stream) = child.stderr.take() {
        let handle = app.clone();
        let app_id = id.to_string();
        thread::spawn(move || stream_lines(stream, true, &handle, &app_id));
    }
    shared
        .children
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .insert(id.to_string(), child);

    let url = manifest
        .launch
        .url
        .as_ref()
        .map(|template| manifest::expand(template, &root, &app_dir, &values))
        .transpose()?;
    mutate(app, shared, id, |record| {
        record.phase = Phase::Running;
        record.url = url;
        record.last_error = None;
        record.step = None;
        record.started_at = Some(now());
    });
    log(app, id, "app started");
    spawn_watcher(app.clone(), Arc::clone(shared), id.to_string());
    Ok(())
}

fn spawn_watcher(app: AppHandle, shared: Arc<Shared>, id: String) {
    thread::spawn(move || loop {
        thread::sleep(Duration::from_millis(500));
        let exited = {
            let mut children = shared
                .children
                .lock()
                .unwrap_or_else(|error| error.into_inner());
            match children.get_mut(&id) {
                Some(child) => match child.try_wait() {
                    Ok(Some(status)) => {
                        children.remove(&id);
                        Some(status)
                    }
                    Ok(None) => None,
                    Err(_) => {
                        children.remove(&id);
                        Some(std::process::ExitStatus::default())
                    }
                },
                None => return,
            }
        };
        if let Some(status) = exited {
            mutate(&app, &shared, &id, |record| {
                if record.phase == Phase::Running || record.phase == Phase::Starting {
                    // A self-exit is not a broken install: Start stays available.
                    record.phase = if record.installed {
                        Phase::Installed
                    } else {
                        Phase::NotInstalled
                    };
                    record.url = None;
                    record.started_at = None;
                    record.last_error = None;
                }
            });
            log(&app, &id, format!("app exited ({status})"));
            return;
        }
    });
}

pub fn stop(app: &AppHandle, shared: &Arc<Shared>, id: &str) -> Result<()> {
    reserve(shared, id, "stop")?;
    let result = stop_inner(app, shared, id);
    release(shared, id);
    result
}

/// Runs the manifest shutdown command before the process tree is killed.
fn run_stop_command(app: &AppHandle, shared: &Shared, id: &str) {
    let record = match find_record(shared, id) {
        Ok(record) => record,
        Err(_) => return,
    };
    let stop = record.manifest.launch.stop.clone();
    if stop.is_empty() {
        return;
    }
    let settings = shared.plugin_settings();
    let root = settings.root_path();
    let force_local_mise = settings.force_local_mise;
    let app_dir = manifest::app_dir(&root, id);
    let values = record.settings.clone();
    let expanded = stop
        .iter()
        .map(|arg| manifest::expand(arg, &root, &app_dir, &values))
        .collect::<Result<Vec<_>>>();
    let expanded = match expanded {
        Ok(expanded) => expanded,
        Err(error) => {
            log(app, id, format!("shutdown command skipped: {error:#}"));
            return;
        }
    };
    let program = &expanded[0];
    let args = &expanded[1..];
    let mut command = if record.manifest.setup.tools.is_empty() {
        tools::new_command(program)
    } else {
        let mut command = mise_command(&root, force_local_mise);
        command
            .arg("--cd")
            .arg(&app_dir)
            .arg("exec")
            .arg("--")
            .arg(program);
        command
    };
    command.args(args);
    if let Err(error) = apply_environment(&mut command, &root, &app_dir, &record.manifest, &values)
    {
        log(app, id, format!("shutdown command skipped: {error:#}"));
        return;
    }
    command.current_dir(&app_dir);
    let mut label = program.clone();
    if !args.is_empty() {
        label.push(' ');
        label.push_str(&args.join(" "));
    }
    if let Err(error) = run_streaming(app, id, label, command) {
        log(app, id, format!("shutdown command failed: {error:#}"));
    }
}

/// Kills a listener left behind by a rebuilt or hard-killed Autark, when it belongs to this app.
fn kill_leftovers(app: &AppHandle, shared: &Shared, id: &str) {
    let (root, manifest, values, app_dir) = {
        let core = shared.core.lock().unwrap_or_else(|error| error.into_inner());
        let record = match core.records.iter().find(|record| record.manifest.id == id) {
            Some(record) => record,
            None => return,
        };
        let root = core.settings.root_path();
        (
            root.clone(),
            record.manifest.clone(),
            record.settings.clone(),
            manifest::app_dir(&root, id),
        )
    };
    let Some(template) = manifest.launch.url.clone() else {
        return;
    };
    let Ok(url) = manifest::expand(&template, &root, &app_dir, &values) else {
        return;
    };
    if let Ok(Some(port)) = reclaim_own_port(&url, &app_dir, id) {
        log(app, id, format!("released port {port} left by an earlier run"));
    }
}

fn stop_inner(app: &AppHandle, shared: &Shared, id: &str) -> Result<()> {
    set_phase(app, shared, id, Phase::Stopping);
    run_stop_command(app, shared, id);
    kill_child(shared, id);
    // A rebuild or a hard exit can leave the old process listening; the port is the proof.
    kill_leftovers(app, shared, id);
    mutate(app, shared, id, |record| {
        record.phase = if record.installed {
            Phase::Installed
        } else {
            Phase::NotInstalled
        };
        record.url = None;
        record.started_at = None;
        record.step = None;
    });
    log(app, id, "app stopped");
    Ok(())
}

pub fn remove(app: &AppHandle, shared: &Arc<Shared>, id: &str, confirm: &str) -> Result<()> {
    if confirm != "REMOVE" {
        bail!("confirm_required: type REMOVE to confirm removal");
    }
    reserve(shared, id, "remove")?;
    let result = remove_inner(app, shared, id);
    release(shared, id);
    result
}

fn remove_inner(app: &AppHandle, shared: &Shared, id: &str) -> Result<()> {
    set_phase(app, shared, id, Phase::Removing);
    kill_child(shared, id);
    let root = shared.root();
    let app_dir = manifest::app_dir(&root, id);
    if app_dir.exists() {
        for entry in fs::read_dir(&app_dir)? {
            let entry = entry?;
            let name = entry.file_name().to_string_lossy().to_string();
            if name == "user-data" || name == "settings.json" {
                continue;
            }
            let path = entry.path();
            if path.is_dir() {
                fs::remove_dir_all(&path)?;
            } else {
                fs::remove_file(&path)?;
            }
        }
    }
    mutate(app, shared, id, |record| {
        record.installed = false;
        record.installed_version = None;
        record.phase = Phase::NotInstalled;
        record.url = None;
        record.last_error = None;
        record.step = None;
        record.started_at = None;
    });
    log(app, id, "app removed, settings and user data kept");
    Ok(())
}

pub fn save_app_settings(
    app: &AppHandle,
    shared: &Shared,
    id: &str,
    values: Map<String, Value>,
) -> Result<()> {
    let record = find_record(shared, id)?;
    if record.phase.is_busy() || record.phase == Phase::Running {
        bail!("operation_in_progress: stop the app before changing its settings");
    }
    let clean = manifest::validate_settings(&record.manifest, &values, &manifest::app_dir(&shared.root(), id))?;
    for lock in settings_locks(&record.manifest) {
        if clean.get(&lock.key) != record.settings.get(&lock.key) {
            bail!(
                "invalid_argument: setting `{}` is owned by the system environment ({}), Autark cannot change it",
                lock.key,
                lock.var
            );
        }
    }
    let root = shared.root();
    let app_dir = manifest::app_dir(&root, id);
    manifest::save_settings(&app_dir, &clean)?;
    mutate(app, shared, id, |record| record.settings = clean);
    Ok(())
}

pub fn save_app_prefs(app: &AppHandle, shared: &Shared, id: &str, prefs: Prefs) -> Result<()> {
    find_record(shared, id)?;
    let app_dir = manifest::app_dir(&shared.root(), id);
    prefs::save(&app_dir, &prefs)?;
    mutate(app, shared, id, |record| record.prefs = prefs);
    Ok(())
}

pub fn app_record(shared: &Shared, id: &str) -> Result<AppRecord> {
    find_record(shared, id)
}

pub fn open_tab(app: &AppHandle, shared: &Shared, id: &str) -> Result<()> {
    let mut core = shared.core.lock().unwrap_or_else(|error| error.into_inner());
    if !core.records.iter().any(|record| record.manifest.id == id) {
        bail!("app_not_found: {id}");
    }
    if !core.settings.open_tabs.iter().any(|tab| tab == id) {
        core.settings.open_tabs.push(id.to_string());
    }
    core.settings.active_tab = Some(id.to_string());
    core.settings.save()?;
    drop(core);
    emit_state(app, shared);
    Ok(())
}

pub fn close_tab(app: &AppHandle, shared: &Shared, id: &str) -> Result<()> {
    let mut core = shared.core.lock().unwrap_or_else(|error| error.into_inner());
    core.settings.open_tabs.retain(|tab| tab != id);
    if core.settings.active_tab.as_deref() == Some(id) {
        core.settings.active_tab = core.settings.open_tabs.last().cloned();
    }
    core.settings.save()?;
    drop(core);
    emit_state(app, shared);
    Ok(())
}

pub fn set_active_tab(app: &AppHandle, shared: &Shared, id: Option<String>) -> Result<()> {
    let mut core = shared.core.lock().unwrap_or_else(|error| error.into_inner());
    if let Some(id) = &id {
        if !core.settings.open_tabs.iter().any(|tab| tab == id) {
            core.settings.open_tabs.push(id.clone());
        }
    }
    core.settings.active_tab = id;
    core.settings.save()?;
    drop(core);
    emit_state(app, shared);
    Ok(())
}

pub fn refresh_marketplace(app: &AppHandle, shared: &Arc<Shared>) -> Result<()> {
    reserve(shared, "", "marketplace")?;
    let result = refresh_inner(app, shared);
    release(shared, "");
    emit_state(app, shared);
    result
}

/// Switching the session's marketplace source takes effect immediately, so the catalogue is reloaded.
pub fn set_marketplace_source(app: &AppHandle, shared: &Arc<Shared>, requested: &str) -> Result<()> {
    let parsed = marketplace::Source::parse(requested)?;
    reserve(shared, "", "marketplace")?;
    let previous = {
        let mut core = shared.core.lock().unwrap_or_else(|error| error.into_inner());
        let previous = core.marketplace.source.clone();
        core.marketplace.source = parsed.label();
        previous
    };
    let result = refresh_inner(app, shared);
    if result.is_err() {
        // The clone still holds the previous source, so the state must not claim the new one.
        let mut core = shared.core.lock().unwrap_or_else(|error| error.into_inner());
        core.marketplace.source = previous;
    }
    if result.is_ok() {
        // Remember the choice so the next start follows the same branch or pull request.
        let mut core = shared.core.lock().unwrap_or_else(|error| error.into_inner());
        core.settings.marketplace_source = core.marketplace.source.clone();
        let settings = core.settings.clone();
        drop(core);
        if let Err(error) = settings.save() {
            release(shared, "");
            emit_state(app, shared);
            return Err(error);
        }
    }
    release(shared, "");
    emit_state(app, shared);
    result
}

/// Restoring is only meaningful while the clone really is on the default branch.
pub fn restore_guard(shared: &Shared, id: &str) -> Result<()> {
    let (source, applied, installed) = {
        let core = shared.core.lock().unwrap_or_else(|error| error.into_inner());
        let installed = core
            .records
            .iter()
            .find(|record| record.manifest.id == id)
            .map(|record| record.installed)
            .unwrap_or(false);
        (
            core.marketplace.source.clone(),
            marketplace::applied_source(&core.settings.root_path()),
            installed,
        )
    };
    if !source.is_empty() || !applied.is_empty() {
        bail!(
            "preview_active: restore needs the default branch. Switch the marketplace back and reload it first."
        );
    }
    if !installed {
        bail!("invalid_argument: the app is not installed");
    }
    Ok(())
}

fn refresh_inner(app: &AppHandle, shared: &Shared) -> Result<()> {
    let root = shared.root();
    let source = {
        let mut core = shared.core.lock().unwrap_or_else(|error| error.into_inner());
        core.marketplace.refreshing = true;
        core.marketplace.error = None;
        core.marketplace.source.clone()
    };
    emit_state(app, shared);
    let result = marketplace::sync(&root, &source);
    let mut core = shared.core.lock().unwrap_or_else(|error| error.into_inner());
    core.marketplace.refreshing = false;
    match result {
        Ok(message) => {
            log(app, "marketplace", message);
            let warnings = load_catalogue(&mut core);
            for warning in warnings {
                log(app, "marketplace", format!("manifest warning: {warning}"));
            }
            core.marketplace.last_refresh = Some(now());
            core.settings.marketplace_last_refresh = Some(now());
            core.marketplace.error = None;
            let _ = core.settings.save();
        }
        Err(error) => {
            let info = ErrorInfo::from_error(&error);
            log(app, "marketplace", format!("refresh failed: {}", info.message));
            core.marketplace.error = Some(info);
        }
    }
    drop(core);
    emit_state(app, shared);
    Ok(())
}

pub fn check_updates(app: &AppHandle, shared: &Shared) -> Result<()> {
    let root = shared.root();
    if marketplace::dir(&root).join("marketplace.json").exists() {
        let mut core = shared.core.lock().unwrap_or_else(|error| error.into_inner());
        load_catalogue(&mut core);
    }
    emit_state(app, shared);
    Ok(())
}

pub fn add_local_app(app: &AppHandle, shared: &Arc<Shared>, path: &str) -> Result<()> {
    let (root, enabled) = {
        let core = shared.core.lock().unwrap_or_else(|error| error.into_inner());
        (core.settings.root_path(), core.settings.experimental_local_apps)
    };
    if !enabled {
        bail!("local apps are experimental and turned off in settings");
    }
    let key = local::add(&root, Path::new(path))?;
    let warnings = {
        let mut core = shared.core.lock().unwrap_or_else(|error| error.into_inner());
        load_catalogue(&mut core)
    };
    for warning in warnings {
        log(app, "marketplace", format!("manifest warning: {warning}"));
    }
    log(app, &key, "local app added");
    emit_state(app, shared);
    Ok(())
}

pub fn replace_local_app(app: &AppHandle, shared: &Arc<Shared>, id: &str, path: &str) -> Result<()> {
    let record = find_record(shared, id).ok();
    if !record.is_some_and(|record| record.local) {
        bail!("the app is not a local app");
    }
    local::replace(&shared.root(), id, Path::new(path))?;
    let warnings = {
        let mut core = shared.core.lock().unwrap_or_else(|error| error.into_inner());
        load_catalogue(&mut core)
    };
    for warning in warnings {
        log(app, "marketplace", format!("manifest warning: {warning}"));
    }
    log(app, id, "local app replaced");
    emit_state(app, shared);
    Ok(())
}

/// Copies the current marketplace recipe into the installed app folder without
/// running setup: a cheap way to pick up an app.json change from a preview source.
pub fn refresh_app_manifest(app: &AppHandle, shared: &Arc<Shared>, id: &str) -> Result<()> {
    let (source_dir, app_dir, manifest) = {
        let core = shared.core.lock().unwrap_or_else(|error| error.into_inner());
        let record = match core.records.iter().find(|record| record.manifest.id == id) {
            Some(record) => record,
            None => bail!("unknown app: {id}"),
        };
        if !record.installed {
            bail!("the app is not installed");
        }
        (
            record.manifest_dir.clone(),
            manifest::app_dir(&core.settings.root_path(), id),
            record.manifest.clone(),
        )
    };
    let file = source_dir.join("app.json");
    if source_dir == app_dir || !file.is_file() {
        bail!("the marketplace source has no recipe for this app");
    }
    fs::copy(&file, app_dir.join("app.json"))?;
    if let Some(icon) = manifest.icon_path(&source_dir) {
        let icon = PathBuf::from(icon);
        if let Some(name) = icon.file_name() {
            let target = app_dir.join(name);
            if icon.is_file() && icon != target {
                fs::copy(&icon, &target)?;
            }
        }
    }
    let mut core = shared.core.lock().unwrap_or_else(|error| error.into_inner());
    load_catalogue(&mut core);
    drop(core);
    emit_state(app, shared);
    log(app, id, "recipe refreshed from the marketplace source".to_string());
    Ok(())
}

pub fn update_all(app: &AppHandle, shared: &Arc<Shared>) -> Result<()> {
    reserve(shared, "", "update_all")?;
    let ids: Vec<String> = {
        let core = shared.core.lock().unwrap_or_else(|error| error.into_inner());
        core.records
            .iter()
            // Global update is version-gated: it never overwrites a preview build
            // with an equal or older recipe. A single app is force reloaded instead.
            .filter(|record| record.installed && record.update_available())
            .map(|record| record.manifest.id.clone())
            .collect()
    };
    if ids.is_empty() {
        log(app, "marketplace", "every installed app is up to date");
    }
    for id in ids {
        let running = shared
            .children
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .contains_key(&id);
        if running {
            kill_child(shared, &id);
        }
        log(app, &id, "starting update");
        if let Err(error) = install(app, shared, &id, SetupKind::Update) {
            log(app, &id, format!("update failed: {error:#}"));
        }
    }
    release(shared, "");
    emit_state(app, shared);
    Ok(())
}

pub fn reload_settings(shared: &Shared, settings: Settings) -> Result<()> {
    {
        let mut core = shared.core.lock().unwrap_or_else(|error| error.into_inner());
        core.settings = settings;
        load_catalogue(&mut core);
    }
    Ok(())
}

pub fn stop_all(shared: &Shared) {
    let ids: Vec<String> = shared
        .children
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .keys()
        .cloned()
        .collect();
    for id in ids {
        kill_child(shared, &id);
    }
}
