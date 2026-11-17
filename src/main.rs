#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod apps;
mod manifest;
mod marketplace;
mod settings;

use apps::{Phase, SetupKind, Shared, State};
use serde::Serialize;
use settings::Settings;
use std::sync::Arc;
use tauri::{AppHandle, Manager, State as TauriState};

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
        .operation
        .lock()
        .map(|operation| operation.is_some())
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
fn save_settings(
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
    let current = apps::wire(&shared).settings;
    settings.open_tabs = current.open_tabs;
    settings.active_tab = current.active_tab;
    settings.marketplace_last_refresh = current.marketplace_last_refresh;
    settings.apply_folder_name(move_folder.unwrap_or(false))?;
    settings.save()?;
    apps::reload_settings(&shared, settings)?;
    apps::emit_state(&app, &shared);
    Ok(apps::wire(&shared))
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
    if operation_running(&inner) {
        return Err(CmdError::new(
            "operation_in_progress",
            "another operation is running",
        ));
    }
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
    apps::release(&inner);
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
    apps::release(&inner);
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
fn remove_app(
    app: AppHandle,
    shared: TauriState<'_, Arc<Shared>>,
    app_id: String,
    confirm: String,
) -> CmdResult<State> {
    apps::remove(&app, &shared, &app_id, &confirm)?;
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
fn cancel_operation(app: AppHandle, shared: TauriState<'_, Arc<Shared>>) -> CmdResult<State> {
    apps::cancel();
    apps::log(&app, "marketplace", "cancelling the running operation");
    Ok(apps::wire(&shared))
}

#[tauri::command(rename_all = "snake_case")]
fn reveal_path(
    shared: TauriState<'_, Arc<Shared>>,
    target: String,
    app_id: Option<String>,
) -> CmdResult<()> {
    let root = shared.root();
    let path = match target.as_str() {
        "root" => root.clone(),
        "marketplace" => marketplace::dir(&root),
        "settings" => settings::settings_path(),
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
    let mut command = std::process::Command::new("explorer");
    if target == "settings" && path.exists() {
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

#[tauri::command(rename_all = "snake_case")]
fn exit_app(app: AppHandle, shared: TauriState<'_, Arc<Shared>>) -> CmdResult<()> {
    apps::stop_all(&shared);
    app.exit(0);
    Ok(())
}

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .manage(Arc::new(Shared::new()))
        .invoke_handler(tauri::generate_handler![
            get_state,
            save_settings,
            pick_directory,
            set_active_tab,
            open_app_tab,
            close_tab,
            refresh_marketplace,
            check_updates,
            install_app,
            update_app,
            update_all,
            start_app,
            stop_app,
            open_app,
            remove_app,
            save_app_settings,
            cancel_operation,
            reveal_path,
            open_url,
            exit_app
        ])
        .setup(|app| {
            let handle = app.handle().clone();
            if let Some(window) = app.get_webview_window("main") {
                let handle = handle.clone();
                window.on_window_event(move |event| {
                    if let tauri::WindowEvent::CloseRequested { .. } = event {
                        if let Some(shared) = handle.try_state::<Arc<Shared>>() {
                            apps::stop_all(&shared);
                        }
                        handle.exit(0);
                    }
                });
            }
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running autark");
}

#[allow(dead_code)]
fn _phase_marker(phase: Phase) -> Phase {
    phase
}
