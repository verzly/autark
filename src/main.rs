#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod apps;
mod manifest;
mod marketplace;
mod local;
mod prefs;
mod settings;
mod terminal;
mod tools;

use apps::{Phase, SetupKind, Shared, State};
use prefs::Prefs;
use serde::Serialize;
use settings::Settings;
use std::sync::Arc;
use tauri::{AppHandle, Emitter, Manager, State as TauriState};
use tauri::{
    menu::{Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
};
use std::sync::OnceLock;

static ARGS: OnceLock<LaunchArgs> = OnceLock::new();
static TRAY_ID: &str = "autark-tray";

#[derive(Default, Clone)]
struct LaunchArgs { app: Option<String>, start: bool, background: bool, shortcut: bool }

fn parse_args(args: impl Iterator<Item = String>) -> LaunchArgs {
    let mut parsed = LaunchArgs::default();
    let mut args = args.skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--app" => parsed.app = args.next(),
            "--start" => parsed.start = true,
            "--background" => parsed.background = true,
            "--shortcut" => parsed.shortcut = true,
            _ => {}
        }
    }
    parsed
}

/// A shortcut only points at the app; the start/background options come from the live prefs.
fn resolve_launch(mut args: LaunchArgs, root: &std::path::Path) -> LaunchArgs {
    if args.shortcut {
        if let Some(id) = args.app.clone() {
            let prefs = prefs::load(&manifest::app_dir(root, &id));
            args.start = prefs.shortcut_autostart;
            args.background = prefs.shortcut_background;
        }
    }
    args
}

#[derive(Debug, Serialize)]
struct CmdError {
    code: String,
    message: String,
}

impl CmdError {
    fn new(code: &str, message: impl Into<String>) -> Self {
        Self {
            code: code.to_string(),
            message: message.into(),
        }
    }
}

impl From<anyhow::Error> for CmdError {
    fn from(error: anyhow::Error) -> Self {
        let info = apps::ErrorInfo::from_error(&error);
        Self {
            code: info.code,
            message: info.message,
        }
    }
}

type CmdResult<T> = Result<T, CmdError>;

fn operation_running(shared: &Shared) -> bool {
    shared
        .operations
        .lock()
        .map(|operations| !operations.is_empty())
        .unwrap_or(false)
}

fn is_installed(shared: &Shared, app_id: &str) -> bool {
    apps::wire(shared)
        .apps
        .iter()
        .find(|app| app.id == app_id)
        .map(|app| app.installed)
        .unwrap_or(false)
}

#[tauri::command(rename_all = "snake_case")]
fn get_state(shared: TauriState<'_, Arc<Shared>>) -> State {
    apps::wire(&shared)
}

#[tauri::command(rename_all = "snake_case")]
async fn set_mise_path(app: AppHandle, shared: TauriState<'_, Arc<Shared>>, path: Option<String>) -> CmdResult<State> {
    let inner = shared.inner().clone();
    let check_path = path.clone();
    tauri::async_runtime::spawn_blocking(move || {
        if let Some(path) = check_path {
            let path = std::path::PathBuf::from(path);
            if !path.is_file() || tools::new_command(&path).arg("--version").output().map(|o| !o.status.success()).unwrap_or(true) {
                return Err(CmdError::new("invalid_argument", "that file does not run as mise"));
            }
        }
        Ok::<(), CmdError>(())
    }).await.map_err(|e| CmdError::new("internal", e.to_string()))??;
    {
        let mut settings = inner.plugin_settings();
        settings.mise_path = path;
        settings.save()?;
        apps::reload_settings(&inner, settings)?;
    }
    apps::refresh_mise(&inner);
    let _ = app;
    Ok(apps::wire(&inner))
}

#[tauri::command(rename_all = "snake_case")]
async fn install_mise(app: AppHandle, shared: TauriState<'_, Arc<Shared>>) -> CmdResult<State> {
    let inner = shared.inner().clone();
    let root = inner.root();
    tauri::async_runtime::spawn_blocking(move || tools::install_local(&app, "mise.exe", &root)).await.map_err(|e| CmdError::new("internal", e.to_string()))??;
    apps::refresh_mise(&inner);
    Ok(apps::wire(&inner))
}

#[tauri::command(rename_all = "snake_case")]
async fn update_mise(app: AppHandle, shared: TauriState<'_, Arc<Shared>>) -> CmdResult<State> {
    let inner = shared.inner().clone();
    let root = inner.root();
    tauri::async_runtime::spawn_blocking(move || tools::self_update(&app, "mise.exe", &root)).await.map_err(|e| CmdError::new("internal", e.to_string()))??;
    apps::refresh_mise(&inner);
    Ok(apps::wire(&inner))
}

#[tauri::command(rename_all = "snake_case")]
async fn install_git(app: AppHandle, shared: TauriState<'_, Arc<Shared>>) -> CmdResult<State> {
    let inner = shared.inner().clone();
    let settings = inner.plugin_settings();
    let root = inner.root();
    tauri::async_runtime::spawn_blocking(move || tools::install_git(&app, "git.exe", &settings, &root)).await.map_err(|e| CmdError::new("internal", e.to_string()))??;
    apps::refresh_mise(&inner);
    Ok(apps::wire(&inner))
}

#[tauri::command(rename_all = "snake_case")]
async fn update_git(app: AppHandle, shared: TauriState<'_, Arc<Shared>>) -> CmdResult<State> {
    let inner = shared.inner().clone();
    let settings = inner.plugin_settings();
    let root = inner.root();
    tauri::async_runtime::spawn_blocking(move || tools::update_git(&app, "git.exe", &settings, &root)).await.map_err(|e| CmdError::new("internal", e.to_string()))??;
    apps::refresh_mise(&inner);
    Ok(apps::wire(&inner))
}

#[tauri::command(rename_all = "snake_case")]
async fn set_bin_path(shared: TauriState<'_, Arc<Shared>>, enabled: bool) -> CmdResult<State> {
    let inner = shared.inner().clone();
    let root = inner.root();
    let actual = tauri::async_runtime::spawn_blocking(move || tools::set_path_registration(&root, enabled)).await.map_err(|e| CmdError::new("internal", e.to_string()))??;
    let mut settings = inner.plugin_settings();
    settings.bin_in_path = actual;
    settings.save()?;
    apps::reload_settings(&inner, settings)?;
    apps::refresh_mise(&inner);
    Ok(apps::wire(&inner))
}

#[tauri::command(rename_all = "snake_case")]
async fn remove_bin_entry(shared: TauriState<'_, Arc<Shared>>, name: String) -> CmdResult<State> {
    let inner = shared.inner().clone();
    if operation_running(&inner) { return Err(CmdError::new("operation_in_progress", "another operation is running")); }
    if name.starts_with('.') || name.contains('/') || name.contains('\\') { return Err(CmdError::new("invalid_argument", "invalid binary name")); }
    let mut settings = inner.plugin_settings();
    let path = std::path::PathBuf::from(&name);
    let root = settings.root_path();
    let target = tools::bin_dir(&root).join(path);
    let removed = target.clone();
    tauri::async_runtime::spawn_blocking(move || std::fs::remove_file(target)).await.map_err(|e| CmdError::new("internal", e.to_string()))?.map_err(anyhow::Error::from)?;
    // A removed copy must not stay selected, otherwise mise looks broken.
    if settings.mise_path.as_deref().map(std::path::Path::new) == Some(removed.as_path()) {
        settings.mise_path = None;
        settings.save()?;
        apps::reload_settings(&inner, settings)?;
    }
    apps::refresh_mise(&inner);
    Ok(apps::wire(&inner))
}

#[tauri::command(rename_all = "snake_case")]
async fn save_settings(
    app: AppHandle,
    shared: TauriState<'_, Arc<Shared>>,
    mut settings: Settings,
    move_folder: Option<bool>,
) -> CmdResult<State> {
    if settings.root.trim().is_empty() {
        return Err(CmdError::new(
            "invalid_argument",
            "the root folder cannot be empty",
        ));
    }
    if operation_running(&shared) {
        return Err(CmdError::new(
            "operation_in_progress",
            "finish the running operation first",
        ));
    }
    let inner = shared.inner().clone();
    let current = apps::wire(&inner).settings;
    settings.open_tabs = current.open_tabs.clone();
    settings.active_tab = current.active_tab.clone();
    settings.marketplace_last_refresh = current.marketplace_last_refresh;
    settings.marketplace_source = current.marketplace_source.clone();
    settings.wizard_complete |= current.wizard_complete;
    // The folder dialog returns the parent, so a new root means <picked>\<folder name>.
    let picked = settings.root_path();
    if picked != current.root_path() {
        let (root, folder_name) =
            Settings::adopt_picked_folder(&settings.root, &settings.folder_name);
        settings.root = root;
        settings.folder_name = folder_name;
    }
    let mise_changed = settings.mise_path != current.mise_path
        || settings.force_local_mise != current.force_local_mise
        || settings.bin_in_path != current.bin_in_path;
    let task_shared = inner.clone();
    tauri::async_runtime::spawn_blocking(move || -> CmdResult<()> {
        settings.apply_folder_name(move_folder.unwrap_or(false))?;
        settings.save()?;
        apps::reload_settings(&task_shared, settings)?;
        if mise_changed {
            apps::refresh_mise(&task_shared);
        }
        Ok(())
    })
    .await
    .map_err(|e| CmdError::new("internal", e.to_string()))??;
    let state = apps::wire(&inner);
    reconcile_tray(&app, &state.settings);
    apply_startup(&app, &state.settings);
    apps::emit_state(&app, &inner);
    Ok(state)
}

#[tauri::command(rename_all = "snake_case")]
async fn reset_autark(
    _app: AppHandle,
    shared: TauriState<'_, Arc<Shared>>,
    confirm: String,
) -> CmdResult<State> {
    if confirm != "RESET" {
        return Err(CmdError::new("confirm_required", "type RESET to confirm"));
    }
    let inner = shared.inner().clone();
    if operation_running(&inner) {
        return Err(CmdError::new("operation_in_progress", "another operation is running"));
    }
    let task_shared = inner.clone();
    tauri::async_runtime::spawn_blocking(move || {
        apps::stop_all(&task_shared);
        let settings = task_shared.plugin_settings();
        let root = settings.root_path();
        let had_path = settings.bin_in_path;
        // A reset forgets the folder as well: the next run starts from the wizard intro.
        if root.exists() {
            let _ = std::fs::remove_dir_all(&root);
        }
        let fresh = Settings::default();
        fresh.save()?;
        if had_path {
            let _ = tools::set_path_registration(&root, false);
        }
        apps::reload_settings(&task_shared, fresh)?;
        apps::refresh_mise(&task_shared);
        Ok::<(), CmdError>(())
    })
    .await
    .map_err(|e| CmdError::new("internal", e.to_string()))??;
    Ok(apps::wire(&inner))
}

#[tauri::command(rename_all = "snake_case")]
fn pick_directory(target: String) -> Option<String> {
    let mut dialog = rfd::FileDialog::new().set_title(match target.as_str() {
        "root_parent" => "Choose the Autark folder",
        _ => "Choose a folder",
    });
    if let Some(home) = std::env::var_os("USERPROFILE") {
        dialog = dialog.set_directory(home);
    }
    dialog
        .pick_folder()
        .map(|path| path.to_string_lossy().to_string())
}

#[tauri::command(rename_all = "snake_case")]
async fn pick_file(title: String, kind: Option<String>) -> Result<Option<String>, CmdError> {
    let dialog = rfd::FileDialog::new().set_title(title);
    let dialog = match kind.as_deref() {
        Some("any") => dialog,
        Some("exe") => dialog.add_filter("Executable", &["exe"]),
        _ => dialog.add_filter("App manifest", &["json"]),
    };
    Ok(dialog.pick_file()
        .map(|path| path.to_string_lossy().to_string()))
}

#[tauri::command(rename_all = "snake_case")]
async fn add_local_app(app: AppHandle, shared: TauriState<'_, Arc<Shared>>, path: String) -> CmdResult<State> {
    let inner = shared.inner().clone();
    let task = inner.clone();
    let handle = app.clone();
    tauri::async_runtime::spawn_blocking(move || apps::add_local_app(&handle, &task, &path))
        .await.map_err(|error| CmdError::new("internal", error.to_string()))??;
    Ok(apps::wire(&inner))
}

#[tauri::command(rename_all = "snake_case")]
async fn replace_local_app(app: AppHandle, shared: TauriState<'_, Arc<Shared>>, app_id: String, path: String) -> CmdResult<State> {
    let inner = shared.inner().clone();
    let task = inner.clone();
    let handle = app.clone();
    tauri::async_runtime::spawn_blocking(move || apps::replace_local_app(&handle, &task, &app_id, &path))
        .await.map_err(|error| CmdError::new("internal", error.to_string()))??;
    Ok(apps::wire(&inner))
}

#[tauri::command(rename_all = "snake_case")]
fn set_active_tab(
    app: AppHandle,
    shared: TauriState<'_, Arc<Shared>>,
    app_id: Option<String>,
) -> CmdResult<State> {
    apps::set_active_tab(&app, &shared, app_id)?;
    Ok(apps::wire(&shared))
}

#[tauri::command(rename_all = "snake_case")]
fn open_app_tab(
    app: AppHandle,
    shared: TauriState<'_, Arc<Shared>>,
    app_id: String,
) -> CmdResult<State> {
    apps::open_tab(&app, &shared, &app_id)?;
    Ok(apps::wire(&shared))
}

#[tauri::command(rename_all = "snake_case")]
fn close_tab(
    app: AppHandle,
    shared: TauriState<'_, Arc<Shared>>,
    app_id: String,
) -> CmdResult<State> {
    apps::close_tab(&app, &shared, &app_id)?;
    Ok(apps::wire(&shared))
}

#[tauri::command(rename_all = "snake_case")]
async fn refresh_marketplace(
    app: AppHandle,
    shared: TauriState<'_, Arc<Shared>>,
) -> CmdResult<State> {
    let inner = shared.inner().clone();
    let task = inner.clone();
    let handle = app.clone();
    let joined =
        tauri::async_runtime::spawn_blocking(move || apps::refresh_marketplace(&handle, &task))
            .await
            .map_err(|error| CmdError::new("internal", error.to_string()))?;
    joined?;
    Ok(apps::wire(&inner))
}

#[tauri::command(rename_all = "snake_case")]
async fn check_updates(app: AppHandle, shared: TauriState<'_, Arc<Shared>>) -> CmdResult<State> {
    let inner = shared.inner().clone();
    apps::check_updates(&app, &inner)?;
    Ok(apps::wire(&inner))
}

/// Refreshes one app's recipe from a branch or pull request, or from the default
/// branch when `source` is empty. Other apps keep the default branch.
#[tauri::command(rename_all = "snake_case")]
async fn refresh_app_source(
    app: AppHandle,
    shared: TauriState<'_, Arc<Shared>>,
    app_id: String,
    source: Option<String>,
) -> CmdResult<State> {
    let inner = shared.inner().clone();
    let handle = app.clone();
    let task = inner.clone();
    tauri::async_runtime::spawn_blocking(move || {
        apps::refresh_app_source(&handle, &task, &app_id, source.as_deref())
    })
    .await
    .map_err(|error| CmdError::new("internal", error.to_string()))??;
    Ok(apps::wire(&inner))
}

#[tauri::command(rename_all = "snake_case")]
async fn install_app(
    app: AppHandle,
    shared: TauriState<'_, Arc<Shared>>,
    app_id: String,
    approve_effects: bool,
) -> CmdResult<State> {
    if !approve_effects {
        return Err(CmdError::new(
            "approval_required",
            "confirm the setup before it changes anything",
        ));
    }
    let inner = shared.inner().clone();
    let kind = if is_installed(&inner, &app_id) {
        SetupKind::Update
    } else {
        SetupKind::Install
    };
    apps::reserve(&inner, &app_id, if kind == SetupKind::Update { "update" } else { "install" })?;
    let task = inner.clone();
    let handle = app.clone();
    let id = app_id.clone();
    let joined = tauri::async_runtime::spawn_blocking(move || {
        apps::install(&handle, &task, &id, kind)
    })
    .await
    .map_err(|error| CmdError::new("internal", error.to_string()));
    apps::release(&inner, &app_id);
    joined??;
    Ok(apps::wire(&inner))
}

#[tauri::command(rename_all = "snake_case")]
async fn update_app(
    app: AppHandle,
    shared: TauriState<'_, Arc<Shared>>,
    app_id: String,
    approve_effects: bool,
) -> CmdResult<State> {
    if !approve_effects {
        return Err(CmdError::new(
            "approval_required",
            "confirm the update before it changes anything",
        ));
    }
    let inner = shared.inner().clone();
    if !is_installed(&inner, &app_id) {
        return Err(CmdError::new("invalid_argument", "the app is not installed"));
    }
    apps::reserve(&inner, &app_id, "update")?;
    let task = inner.clone();
    let handle = app.clone();
    let id = app_id.clone();
    let joined = tauri::async_runtime::spawn_blocking(move || {
        apps::install(&handle, &task, &id, SetupKind::Update)
    })
    .await
    .map_err(|error| CmdError::new("internal", error.to_string()));
    apps::release(&inner, &app_id);
    joined??;
    Ok(apps::wire(&inner))
}

#[tauri::command(rename_all = "snake_case")]
async fn reinstall_app(
    app: AppHandle,
    shared: TauriState<'_, Arc<Shared>>,
    app_id: String,
    approve_effects: bool,
) -> CmdResult<State> {
    if !approve_effects {
        return Err(CmdError::new(
            "approval_required",
            "confirm the reinstall before it changes anything",
        ));
    }
    let inner = shared.inner().clone();
    if !is_installed(&inner, &app_id) {
        return Err(CmdError::new("invalid_argument", "the app is not installed"));
    }
    apps::reserve(&inner, &app_id, "reinstall")?;
    let task = inner.clone();
    let handle = app.clone();
    let id = app_id.clone();
    let joined = tauri::async_runtime::spawn_blocking(move || {
        apps::install(&handle, &task, &id, SetupKind::Update)
    })
    .await
    .map_err(|error| CmdError::new("internal", error.to_string()));
    apps::release(&inner, &app_id);
    joined??;
    Ok(apps::wire(&inner))
}

#[tauri::command(rename_all = "snake_case")]
async fn restore_app(
    app: AppHandle,
    shared: TauriState<'_, Arc<Shared>>,
    app_id: String,
    approve_effects: bool,
) -> CmdResult<State> {
    if !approve_effects {
        return Err(CmdError::new(
            "approval_required",
            "confirm the stable reinstall before it changes anything",
        ));
    }
    let inner = shared.inner().clone();
    apps::restore_guard(&inner, &app_id)?;
    // Stop with the preview recipe, so its shutdown command and port are handled,
    // before the reinstall replaces it with the default branch.
    apps::stop(&app, &inner, &app_id)?;
    apps::refresh_app_source(&app, &inner, &app_id, Some(""))?;
    apps::reserve(&inner, &app_id, "update")?;
    let task = inner.clone();
    let handle = app.clone();
    let id = app_id.clone();
    let joined = tauri::async_runtime::spawn_blocking(move || {
        apps::install(&handle, &task, &id, SetupKind::Update)
    })
    .await
    .map_err(|error| CmdError::new("internal", error.to_string()));
    apps::release(&inner, &app_id);
    joined??;
    Ok(apps::wire(&inner))
}

#[tauri::command(rename_all = "snake_case")]
async fn update_all(
    app: AppHandle,
    shared: TauriState<'_, Arc<Shared>>,
    approve_effects: bool,
) -> CmdResult<State> {
    if !approve_effects {
        return Err(CmdError::new(
            "approval_required",
            "confirm updating every app",
        ));
    }
    let inner = shared.inner().clone();
    let task = inner.clone();
    let handle = app.clone();
    let joined = tauri::async_runtime::spawn_blocking(move || apps::update_all(&handle, &task))
        .await
        .map_err(|error| CmdError::new("internal", error.to_string()))?;
    joined?;
    Ok(apps::wire(&inner))
}

#[tauri::command(rename_all = "snake_case")]
async fn start_app(
    app: AppHandle,
    shared: TauriState<'_, Arc<Shared>>,
    app_id: String,
) -> CmdResult<State> {
    let inner = shared.inner().clone();
    let task = inner.clone();
    let handle = app.clone();
    tauri::async_runtime::spawn_blocking(move || apps::start(&handle, &task, &app_id))
        .await
        .map_err(|error| CmdError::new("internal", error.to_string()))??;
    Ok(apps::wire(&inner))
}

#[tauri::command(rename_all = "snake_case")]
async fn run_action(
    app: AppHandle,
    shared: TauriState<'_, Arc<Shared>>,
    app_id: String,
    action_id: String,
) -> CmdResult<State> {
    let inner = shared.inner().clone();
    let task = inner.clone();
    tauri::async_runtime::spawn_blocking(move || apps::run_action(&app, &task, &app_id, &action_id))
        .await
        .map_err(|error| CmdError::new("internal", error.to_string()))??;
    Ok(apps::wire(&inner))
}

#[tauri::command(rename_all = "snake_case")]
async fn stop_app(
    app: AppHandle,
    shared: TauriState<'_, Arc<Shared>>,
    app_id: String,
) -> CmdResult<State> {
    let inner = shared.inner().clone();
    let task = inner.clone();
    let handle = app.clone();
    tauri::async_runtime::spawn_blocking(move || apps::stop(&handle, &task, &app_id))
        .await
        .map_err(|error| CmdError::new("internal", error.to_string()))??;
    Ok(apps::wire(&inner))
}

#[tauri::command(rename_all = "snake_case")]
fn open_app(shared: TauriState<'_, Arc<Shared>>, app_id: String) -> CmdResult<State> {
    let state = apps::wire(&shared);
    let app = state
        .apps
        .iter()
        .find(|app| app.id == app_id)
        .ok_or_else(|| CmdError::new("app_not_found", format!("unknown app {app_id}")))?;
    let url = app
        .url
        .clone()
        .ok_or_else(|| CmdError::new("invalid_argument", "the app is not running"))?;
    open_url(url)?;
    Ok(apps::wire(&shared))
}

#[tauri::command(rename_all = "snake_case")]
async fn remove_app(
    app: AppHandle,
    shared: TauriState<'_, Arc<Shared>>,
    app_id: String,
    confirm: String,
) -> CmdResult<State> {
    let inner = shared.inner().clone();
    let handle = app.clone();
    let id = app_id.clone();
    tauri::async_runtime::spawn_blocking(move || apps::remove(&handle, &inner, &id, &confirm))
        .await
        .map_err(|error| CmdError::new("internal", error.to_string()))??;
    Ok(apps::wire(&shared))
}

#[tauri::command(rename_all = "snake_case")]
fn save_app_settings(
    app: AppHandle,
    shared: TauriState<'_, Arc<Shared>>,
    app_id: String,
    settings: serde_json::Map<String, serde_json::Value>,
) -> CmdResult<State> {
    apps::save_app_settings(&app, &shared, &app_id, settings)?;
    Ok(apps::wire(&shared))
}

#[tauri::command(rename_all = "snake_case")]
fn probe_llm(shared: TauriState<'_, Arc<Shared>>, app_id: String, check: String, settings: Option<serde_json::Map<String, serde_json::Value>>) -> CmdResult<String> {
    let record = apps::app_record(&shared, &app_id)?;
    let root = shared.root();
    let app_dir = manifest::app_dir(&root, &app_id);
    let mut values = manifest::load_settings(&app_dir, &record.manifest);
    if let Some(settings) = settings {
        for (key, value) in settings { values.insert(key, value); }
    }
    let get = |key: &str| values.get(key).and_then(serde_json::Value::as_str).unwrap_or("");
    let base = get("llm_base_url").trim_end_matches('/');
    if base.is_empty() { return Err(CmdError::new("invalid_argument", "LLM base URL is empty")); }
    let endpoint = format!("{}/models", base);
    let mut command = crate::tools::new_command("curl.exe");
    command.args(["--config", "-", "-m", "10", "-sS", "-w", "\n%{http_code}", endpoint.as_str()]);
    let key_path = get("llm_api_key_file");
    let key_path = if key_path.is_empty() { None } else {
        manifest::expand(key_path, &root, &app_dir, &values).ok().map(std::path::PathBuf::from)
    };
    let key = key_path.and_then(|path| std::fs::read_to_string(path).ok()).map(|key| key.trim().to_string()).filter(|key| !key.is_empty());
    let mut child = command.stdin(std::process::Stdio::piped()).stdout(std::process::Stdio::piped()).stderr(std::process::Stdio::piped()).spawn().map_err(|error| CmdError::new("internal", format!("Could not start curl.exe: {error}")))?;
    if let Some(mut stdin) = child.stdin.take() {
        if let Some(key) = &key {
            use std::io::Write;
            let escaped = key.replace('\\', "\\\\").replace('"', "\\\"");
            write!(stdin, "header = \"Authorization: Bearer {escaped}\"\n").map_err(|error| CmdError::new("internal", error.to_string()))?;
        }
    }
    let output = child.wait_with_output().map_err(|error| CmdError::new("internal", error.to_string()))?;
    let text = String::from_utf8_lossy(&output.stdout);
    let (body, status) = text.rsplit_once('\n').unwrap_or((&text, "000"));
    let status: u16 = status.trim().parse().unwrap_or(0);
    if check == "endpoint" {
        if status == 0 { return Err(CmdError::new("internal", "Could not reach LLM endpoint")); }
        return Ok(format!("HTTP {status}"));
    }
    if check != "model" && check != "models" { return Err(CmdError::new("invalid_argument", "Unknown LLM check")); }
    if !(200..300).contains(&status) { return Err(CmdError::new("internal", body.lines().next().unwrap_or("LLM request failed").trim().chars().take(180).collect::<String>())); }
    let json: serde_json::Value = serde_json::from_str(body).map_err(|_| CmdError::new("internal", "Invalid models response"))?;
    let ids: Vec<&str> = json["data"].as_array().map(|items| items.iter().filter_map(|item| item["id"].as_str()).collect()).unwrap_or_default();
    if check == "models" { return serde_json::to_string(&ids).map_err(|error| CmdError::new("internal", error.to_string())); }
    let model = get("llm_model");
    if ids.iter().any(|id| id.eq_ignore_ascii_case(model)) {
        Ok("Model available".into())
    } else { Err(CmdError::new("invalid_argument", format!("Model not found: {model}"))) }
}

#[tauri::command(rename_all = "snake_case")]
fn save_app_prefs(app: AppHandle, shared: TauriState<'_, Arc<Shared>>, app_id: String, prefs: Prefs) -> CmdResult<State> {
    apps::save_app_prefs(&app, &shared, &app_id, prefs)?;
    Ok(apps::wire(&shared))
}

#[tauri::command(rename_all = "snake_case")]
fn create_shortcut(app: AppHandle, shared: TauriState<'_, Arc<Shared>>, app_id: String) -> CmdResult<State> {
    let record = apps::app_record(&shared, &app_id)?;
    let mut exe = std::env::current_exe().map_err(|e| CmdError::new("internal", e.to_string()))?;
    if exe.to_string_lossy().to_lowercase().contains(r"\target\debug\") {
        let release = exe.parent().and_then(|dir| dir.parent()).map(|target| target.join("release").join("autark.exe"));
        match release.filter(|path| path.exists()) {
            Some(path) => exe = path,
            None => return Err(CmdError::new("invalid_argument", "this is a development build; run cargo build --release and create the shortcut from the release exe")),
        }
    }
    let root = shared.root();
    let app_dir = manifest::app_dir(&root, &app_id);
    let shortcut_icon = app_dir.join("shortcut.ico");
    let icon = record.manifest.icon_path(&record.manifest_dir);
    let icon_script = icon.map(|path| format!(
        "Add-Type -AssemblyName System.Drawing; $bitmap = [System.Drawing.Image]::FromFile('{}'); $handle = $bitmap.GetHicon(); $icon = [System.Drawing.Icon]::FromHandle($handle); $stream = [System.IO.File]::Create('{}'); $icon.Save($stream); $stream.Close(); $bitmap.Dispose();",
        ps_quote(&path), ps_quote(&shortcut_icon.to_string_lossy())
    )).unwrap_or_default();
    let args = format!("--app {} --shortcut", ps_quote(&app_id));
    let script = format!(
        "$ErrorActionPreference='Stop'; $desktop=[Environment]::GetFolderPath('Desktop'); {icon_script} $s=(New-Object -ComObject WScript.Shell).CreateShortcut((Join-Path $desktop '{}.lnk')); $s.TargetPath='{}'; $s.Arguments='{}'; $s.IconLocation='{}'; $s.Save(); Write-Output $s.FullName",
        ps_quote(&record.manifest.name), ps_quote(&exe.to_string_lossy()), ps_quote(&args),
        ps_quote(&if shortcut_icon.exists() { shortcut_icon.to_string_lossy().to_string() } else { exe.to_string_lossy().to_string() })
    );
    let output = tools::new_command("powershell").args(["-NoProfile", "-ExecutionPolicy", "Bypass", "-Command", &script]).output()
        .map_err(|e| CmdError::new("invalid_argument", e.to_string()))?;
    if !output.status.success() { return Err(CmdError::new("invalid_argument", String::from_utf8_lossy(&output.stderr).to_string())); }
    let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
    apps::log(&app, &app_id, format!("shortcut created at {path}"));
    Ok(apps::wire(&shared))
}

fn ps_quote(value: &str) -> String { value.replace('\'', "''") }

fn show_window(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") { let _ = window.show(); let _ = window.set_focus(); }
}

fn reconcile_tray(app: &AppHandle, settings: &Settings) {
    if !settings.tray_enabled {
        let _ = app.remove_tray_by_id(TRAY_ID);
        return;
    }
    if app.tray_by_id(TRAY_ID).is_some() { return; }
    let show = MenuItem::with_id(app, "show", "Show Autark", true, None::<&str>).unwrap();
    let quit = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>).unwrap();
    let menu = Menu::with_items(app, &[&show, &quit]).unwrap();
    let handle = app.clone();
    let mut builder = TrayIconBuilder::with_id(TRAY_ID).tooltip("Autark").menu(&menu)
        .on_menu_event(move |app, event| match event.id().as_ref() {
            "show" => show_window(app),
            "quit" => { if let Some(shared) = app.try_state::<Arc<Shared>>() { apps::stop_all(&shared); } app.exit(0); },
            _ => {}
        })
        .on_tray_icon_event(move |tray, event| {
            if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. } = event { show_window(tray.app_handle()); }
        });
    if let Some(icon) = app.default_window_icon() { builder = builder.icon(icon.clone()); }
    let _ = builder.build(app);
    let _ = handle;
}

fn apply_startup(_app: &AppHandle, settings: &Settings) {
    let mut command = tools::new_command("reg");
    if settings.start_with_windows && settings.tray_enabled {
        if let Ok(exe) = std::env::current_exe() {
            let value = format!("\"{}\" --background", exe.display());
            let _ = command.args(["add", r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run", "/v", "Autark", "/t", "REG_SZ", "/d", &value, "/f"]).output();
        }
    } else {
        let _ = command.args(["delete", r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run", "/v", "Autark", "/f"]).output();
    }
}

#[tauri::command(rename_all = "snake_case")]
fn cancel_operation(
    app: AppHandle,
    shared: TauriState<'_, Arc<Shared>>,
    app_id: String,
) -> CmdResult<State> {
    apps::cancel(&app_id);
    apps::log(&app, &app_id, "cancelling the running operation");
    Ok(apps::wire(&shared))
}

#[tauri::command(rename_all = "snake_case")]
fn reveal_path(
    shared: TauriState<'_, Arc<Shared>>,
    target: String,
    app_id: Option<String>,
    path: Option<String>,
) -> CmdResult<()> {
    let root = shared.root();
    let path = match target.as_str() {
        "root" => root.clone(),
        "marketplace" => marketplace::dir(&root),
        "settings" => settings::settings_path(),
        "file" => std::path::PathBuf::from(
            path.ok_or_else(|| CmdError::new("invalid_argument", "path is required"))?,
        ),
        "app" => {
            let id = app_id
                .clone()
                .ok_or_else(|| CmdError::new("invalid_argument", "app_id is required"))?;
            manifest::app_dir(&root, &id)
        }
        other => {
            return Err(CmdError::new(
                "invalid_argument",
                format!("unknown reveal target {other}"),
            ))
        }
    };
    let mut command = tools::new_command("explorer");
    if (target == "settings" || target == "file") && path.exists() {
        command.arg(format!("/select,{}", path.display()));
    } else {
        command.arg(&path);
    }
    command
        .spawn()
        .map_err(|error| CmdError::new("internal", error.to_string()))?;
    Ok(())
}

#[tauri::command(rename_all = "snake_case")]
fn open_url(url: String) -> CmdResult<()> {
    if !(url.starts_with("https://") || url.starts_with("http://")) {
        return Err(CmdError::new(
            "invalid_argument",
            "only http and https links can be opened",
        ));
    }
    tauri_plugin_opener::open_url(url, None::<&str>)
        .map_err(|error| CmdError::new("internal", error.to_string()))
}

#[tauri::command]
fn open_terminal(shared: TauriState<'_, Arc<Shared>>) -> CmdResult<String> {
    let inner = shared.inner().clone();
    let root = inner.root();
    let shell = inner.plugin_settings().terminal;
    let shells = inner
        .core
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .terminal
        .clone();
    terminal::open(&root, &shell, &shells)?;
    Ok(format!("Terminal opened in {}", root.display()))
}

#[tauri::command(rename_all = "snake_case")]
fn exit_app(app: AppHandle, shared: TauriState<'_, Arc<Shared>>) -> CmdResult<()> {
    apps::stop_all(&shared);
    app.exit(0);
    Ok(())
}

fn main() {
    let args = parse_args(std::env::args());
    let _ = ARGS.set(args.clone());
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, argv, _cwd| {
            let mut args = parse_args(argv.into_iter());
            if let Some(shared) = app.try_state::<Arc<Shared>>() {
                args = resolve_launch(args, &shared.root());
            }
            show_window(app);
            let _ = app.emit("focus_app", args.app.clone());
            if let Some(id) = args.app {
                if let Some(shared) = app.try_state::<Arc<Shared>>() {
                    let _ = apps::open_tab(app, &shared, &id);
                    if args.start {
                        let shared = shared.inner().clone();
                        let handle = app.clone();
                        tauri::async_runtime::spawn_blocking(move || {
                            if let Err(error) = apps::start(&handle, &shared, &id) { apps::log(&handle, &id, format!("start failed: {error:#}")); }
                        });
                    }
                }
            }
        }))
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_notification::init())
        .manage(Arc::new(Shared::new()))
        .invoke_handler(tauri::generate_handler![
            get_state,
            save_settings,
            reset_autark,
            set_mise_path,
            install_mise,
            update_mise,
            install_git,
            update_git,
            open_terminal,
            set_bin_path,
            remove_bin_entry,
            pick_directory,
            pick_file,
            add_local_app,
            replace_local_app,
            set_active_tab,
            open_app_tab,
            close_tab,
            refresh_marketplace,
            refresh_app_source,
            migrate_pg0,
            install_pg0,
            uninstall_pg0,
            check_updates,
            install_app,
            update_app,
            reinstall_app,
            restore_app,
            update_all,
            start_app,
            run_action,
            stop_app,
            open_app,
            remove_app,
            save_app_settings,
            probe_llm,
            save_app_prefs,
            create_shortcut,
            cancel_operation,
            reveal_path,
            open_url,
            exit_app
        ])
        .setup(|app| {
            let startup_handle = app.handle().clone();
            let startup_shared = app.state::<Arc<Shared>>().inner().clone();
            tauri::async_runtime::spawn_blocking(move || {
                if let Err(error) = apps::daily_update_check(&startup_handle, &startup_shared) {
                    apps::log(&startup_handle, "marketplace", format!("startup update check failed: {error:#}"));
                }
            });
            let handle = app.handle().clone();
            let settings = apps::wire(&app.state::<Arc<Shared>>()).settings;
            reconcile_tray(&handle, &settings);
            apply_startup(&handle, &settings);
            let args = resolve_launch(ARGS.get().cloned().unwrap_or_default(), &app.state::<Arc<Shared>>().root());
            if args.background && settings.tray_enabled {
                if let Some(window) = app.get_webview_window("main") { let _ = window.hide(); }
            }
            if let Some(id) = args.app.clone() {
                let _ = apps::open_tab(&handle, &app.state::<Arc<Shared>>(), &id);
                if args.start {
                    let shared = app.state::<Arc<Shared>>().inner().clone();
                    let handle = handle.clone();
                    tauri::async_runtime::spawn_blocking(move || {
                        if let Err(error) = apps::start(&handle, &shared, &id) { apps::log(&handle, &id, format!("start failed: {error:#}")); }
                    });
                }
            } else {
                let state = apps::wire(&app.state::<Arc<Shared>>());
                for record in state.apps.into_iter().filter(|item| item.prefs.auto_start && item.installed && item.phase != Phase::Running) {
                    let shared = app.state::<Arc<Shared>>().inner().clone();
                    let handle = handle.clone();
                    tauri::async_runtime::spawn_blocking(move || {
                        if let Err(error) = apps::start(&handle, &shared, &record.id) { apps::log(&handle, &record.id, format!("auto-start failed: {error:#}")); }
                    });
                }
            }
            if let Some(window) = app.get_webview_window("main") {
                let handle = handle.clone();
                window.on_window_event(move |event| {
                    if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                        if let Some(shared) = handle.try_state::<Arc<Shared>>() {
                            let settings = apps::wire(&shared).settings;
                            if settings.close_to_tray && settings.tray_enabled {
                                api.prevent_close();
                                if let Some(window) = handle.get_webview_window("main") { let _ = window.hide(); }
                                return;
                            }
                            apps::stop_all(&shared);
                        }
                        handle.exit(0);
                    }
                });
            }
            let handle = app.handle().clone();
            tauri::async_runtime::spawn_blocking(move || loop {
                std::thread::sleep(std::time::Duration::from_secs(24 * 60 * 60));
                if let Some(shared) = handle.try_state::<Arc<Shared>>() {
                    if let Err(error) = apps::daily_update_check(&handle, &shared) {
                        apps::log(&handle, "marketplace", format!("daily update check failed: {error:#}"));
                    }
                }
            });
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running autark");
}

#[allow(dead_code)]
fn _phase_marker(phase: Phase) -> Phase {
    phase
}

#[tauri::command(rename_all = "snake_case")]
async fn migrate_pg0(
    app: AppHandle,
    shared: TauriState<'_, Arc<Shared>>,
    app_id: String,
    to_shared: bool,
) -> CmdResult<State> {
    let inner = shared.inner().clone();
    let handle = app.clone();
    let id = app_id.clone();
    tauri::async_runtime::spawn_blocking(move || apps::migrate_pg0(&handle, &inner, &id, to_shared))
        .await
        .map_err(|error| CmdError::new("internal", error.to_string()))??;
    Ok(apps::wire(&shared))
}

#[tauri::command]
async fn install_pg0(
    app: AppHandle,
    shared: TauriState<'_, Arc<Shared>>,
    force: bool,
) -> CmdResult<State> {
    apps::reserve(&shared, "pg0.exe", "install")?;
    let settings = shared.inner().plugin_settings();
    let root = settings.root_path();
    let handle = app.clone();
    let result = tauri::async_runtime::spawn_blocking(move || {
        crate::tools::install_pg0(&handle, "pg0.exe", &settings, &root, force)
    })
    .await
    .map_err(|error| CmdError::new("internal", error.to_string()))
    .and_then(|result| result.map_err(CmdError::from));
    if result.is_ok() {
        let root = shared.inner().plugin_settings().root_path();
        let mut core = shared.core.lock().unwrap_or_else(|error| error.into_inner());
        core.pg0 = crate::tools::probe_pg0(&root);
    }
    apps::release(&shared, "pg0.exe");
    result?;
    Ok(apps::wire(&shared))
}

#[tauri::command]
async fn uninstall_pg0(app: AppHandle, shared: TauriState<'_, Arc<Shared>>) -> CmdResult<State> {
    apps::reserve_pg0_removal(&shared)?;
    if let Err(error) = apps::pg0_removal_preflight(&shared) {
        return Err(error.into());
    }
    let settings = shared.inner().plugin_settings();
    let root = settings.root_path();
    let handle = app.clone();
    let result = tauri::async_runtime::spawn_blocking(move || {
        crate::tools::uninstall_pg0(&handle, "pg0.exe", &settings, &root)
    })
    .await
    .map_err(|error| CmdError::new("internal", error.to_string()))
    .and_then(|result| result.map_err(CmdError::from));
    {
        let current_root = shared.inner().plugin_settings().root_path();
        let mut core = shared.core.lock().unwrap_or_else(|error| error.into_inner());
        core.pg0 = crate::tools::probe_pg0(&current_root);
    }
    let state = apps::wire(&shared);
    apps::release(&shared, "pg0.exe");
    if result.is_err() {
        apps::emit_state(&app, &shared);
    }
    result?;
    Ok(state)
}
