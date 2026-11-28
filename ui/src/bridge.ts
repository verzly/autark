export type Phase = "not_installed" | "installing" | "installed" | "starting" | "running" | "stopping" | "updating" | "removing" | "failed";
export interface Settings { root: string; folder_name: string; force_local_mise: boolean; animations: boolean; open_tabs: string[]; active_tab: string | null; tray_enabled: boolean; start_with_windows: boolean; close_to_tray: boolean; experimental_local_apps: boolean; wizard_complete: boolean; terminal: string; developer_mode: boolean; force_terminal_operations: boolean }
export interface BinEntry { name: string; path: string; version: string | null; size: number }
export interface DetectedMise { path: string; version: string | null }
export interface MiseInfo { available: boolean; source: string; path: string | null; version: string | null; local_path: string; bin_path: string; bin_in_path: boolean; bin: BinEntry[]; detected: DetectedMise[]; force_local: boolean }
export interface GitExternal { path: string; version: string | null; from_mise: boolean }
export interface GitInfo { installed: boolean; version: string | null; path: string | null; external: GitExternal | null }
export interface BinInfo { available: boolean; installed: boolean; external: boolean; path: string | null; local_path: string; install_dir: string; detected: DetectedMise[] }
export interface AppSetting { key: string; label: string; kind: "text" | "number" | "port" | "boolean" | "path" | "file" | "folders"; default: unknown; help?: string | null; group?: string | null }
export interface AppAction { id: string; label: string; confirm: boolean }
export interface SettingsLock { key: string; var: string; value: string }
export interface AppPrefs { auto_start: boolean; shortcut_autostart: boolean; shortcut_background: boolean; source_ref: string | null }
export interface AppState {
  id: string; name: string; description: string; version_installed: string | null; version_app: string | null; version_latest: string | null;
    update_available: boolean; installed: boolean; local: boolean; upstream: string | null; latest_upstream: string | null; source_ref: string | null; source_branch: string | null; installed_source_ref: string | null; source_reinstall_required: boolean; phase: Phase; running: boolean; preview: string | null; restore_available: boolean; settings: Record<string, unknown>; settings_schema: AppSetting[]; settings_locked: SettingsLock[]; prefs: AppPrefs; actions: AppAction[]; can_start: boolean;
   url: string | null; homepage: string | null; license: string | null; icon_path: string | null; icon_data: string | null;
  requirements: { disk_gb: number | null; webview2: boolean };
  faq: { question: string; answer: string }[];
  last_error: { code: string; message: string } | null; step: { index: number; total: number; name: string } | null;
  started_at: number | null; elapsed_secs: number | null;
}
export interface State {
  settings: Settings; settings_path: string; settings_exists: boolean;
  marketplace: { path: string; url: string; exists: boolean; refreshing: boolean; last_refresh: number | null; source: string; error: { code: string; message: string } | null };
  apps: AppState[]; operations: { app_id: string; kind: string }[]; busy: boolean; mise: MiseInfo; bin: BinInfo; git: GitInfo; terminal: { bash: string | null; sh: string | null }; pg0: { available: boolean; version: string | null; path: string | null; installation: string; source: string | null; used_by: string[] };
}
export const mockMode = () => import.meta.env.DEV && !("__TAURI_INTERNALS__" in window);
const isTauri = () => "__TAURI_INTERNALS__" in window;
async function invoke<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  if (!isTauri()) throw new Error("Actions require the Windows launcher.");
  const { invoke: call } = await import("@tauri-apps/api/core");
  return call<T>(command, args);
}
export const api = {
  getState: () => invoke<State>("get_state"),
  saveSettings: (settings: Settings, moveFolder?: boolean) => invoke<State>("save_settings", { settings, ...(moveFolder === undefined ? {} : { move_folder: moveFolder }) }),
  pickDirectory: (target: string) => invoke<string | null>("pick_directory", { target }),
  pickFile: (title: string, kind?: "json" | "exe" | "any") => invoke<string | null>("pick_file", { title, kind }),
  setMisePath: (path: string | null) => invoke<State>("set_mise_path", { path }),
  installMise: () => invoke<State>("install_mise"),
  updateMise: () => invoke<State>("update_mise"),
  installGit: () => invoke<State>("install_git"),
  installPg0: (force: boolean) => invoke<State>("install_pg0", { force }),
  uninstallPg0: () => invoke<State>("uninstall_pg0"),
  installBin: () => invoke<State>("install_bin"),
  chooseBinPath: (path: string | null) => invoke<State>("choose_bin_path", { path }),
  updateBin: (name?: string) => invoke<State>("update_bin", { name: name ?? null }),
  wslDistros: () => invoke<string[]>("wsl_distros"),
  updateGit: () => invoke<State>("update_git"),
  setBinPath: (enabled: boolean) => invoke<State>("set_bin_path", { enabled }),
  removeBinEntry: (name: string) => invoke<State>("remove_bin_entry", { name }),
  addLocalApp: (path: string) => invoke<State>("add_local_app", { path }),
  replaceLocalApp: (app_id: string, path: string) => invoke<State>("replace_local_app", { app_id, path }),
  setActiveTab: (app_id: string | null) => invoke<State>("set_active_tab", { app_id }),
  closeTab: (app_id: string) => invoke<State>("close_tab", { app_id }),
  openAppTab: (app_id: string) => invoke<State>("open_app_tab", { app_id }),
  refreshMarketplace: () => invoke<State>("refresh_marketplace"),
  refreshAppSource: (app_id: string, source?: string) => invoke<State>("refresh_app_source", { app_id, source: source ?? null }),
  migratePg0: (app_id: string, to_shared: boolean) => invoke<State>("migrate_pg0", { app_id, to_shared }),
  checkUpdates: () => invoke<State>("check_updates"),
  installApp: (app_id: string, approve_effects: boolean) => invoke<State>("install_app", { app_id, approve_effects }),
  updateApp: (app_id: string, approve_effects: boolean) => invoke<State>("update_app", { app_id, approve_effects }),
  reinstallApp: (app_id: string, approve_effects: boolean) => invoke<State>("reinstall_app", { app_id, approve_effects }),
  restoreApp: (app_id: string, approve_effects: boolean) => invoke<State>("restore_app", { app_id, approve_effects }),
  updateAll: (approve_effects: boolean) => invoke<State>("update_all", { approve_effects }),
  startApp: (app_id: string) => invoke<State>("start_app", { app_id }),
  stopApp: (app_id: string) => invoke<State>("stop_app", { app_id }),
  runAction: (app_id: string, action_id: string) => invoke<State>("run_action", { app_id, action_id }),
  openApp: (app_id: string) => invoke<State>("open_app", { app_id }),
  removeApp: (app_id: string, confirm: string) => invoke<State>("remove_app", { app_id, confirm }),
  resetAutark: (confirm: string) => invoke<State>("reset_autark", { confirm }),
  saveAppSettings: (app_id: string, settings: Record<string, unknown>) => invoke<State>("save_app_settings", { app_id, settings }),
  probeLlm: (app_id: string, check: "endpoint" | "model" | "models", settings?: Record<string, unknown>) => invoke<string>("probe_llm", { app_id, check, settings }),
  saveAppPrefs: (app_id: string, prefs: AppState["prefs"]) => invoke<State>("save_app_prefs", { app_id, prefs }),
  createShortcut: (app_id: string) => invoke<State>("create_shortcut", { app_id }),
  cancelOperation: (app_id: string) => invoke<State>("cancel_operation", { app_id }),
  revealPath: (target: "root" | "marketplace" | "app" | "settings", app_id?: string) => invoke<void>("reveal_path", { target, ...(app_id ? { app_id } : {}) }),
  revealFile: (path: string) => invoke<void>("reveal_path", { target: "file", path }),
  openUrl: (url: string) => invoke<void>("open_url", { url }),
  exitApp: () => invoke<void>("exit_app"),
  openTerminal: () => invoke<string>("open_terminal"),
  onFocus: async (fn: (app_id: string | null) => void) => {
    if (!isTauri()) return () => {};
    const { listen } = await import("@tauri-apps/api/event");
    return listen<string | null>("focus_app", event => fn(event.payload ?? null));
  },
  onState: async (fn: (state: State) => void) => {
    if (!isTauri()) return () => {};
    const { listen } = await import("@tauri-apps/api/event");
    return listen<State>("state", event => fn(event.payload));
  },
  onLog: async (fn: (event: { app_id: string; line: string }) => void) => {
    if (!isTauri()) return () => {};
    const { listen } = await import("@tauri-apps/api/event");
    return listen<{ app_id: string; line: string }>("log", event => fn(event.payload));
  },

};
