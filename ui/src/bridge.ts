export type Phase = "not_installed" | "installing" | "installed" | "starting" | "running" | "stopping" | "updating" | "removing" | "failed";
export interface Settings { root: string; folder_name: string; force_local_mise: boolean; open_tabs: string[]; active_tab: string | null }
export interface AppSetting { key: string; label: string; kind: "text" | "number" | "port" | "boolean" | "path"; default: unknown; help?: string | null }
export interface AppState {
  id: string; name: string; description: string; version_installed: string | null; version_latest: string | null;
  update_available: boolean; installed: boolean; phase: Phase; settings: Record<string, unknown>; settings_schema: AppSetting[];
  url: string | null; homepage: string | null; license: string | null; icon_path: string | null;
  requirements: { disk_gb: number | null; webview2: boolean };
  last_error: { code: string; message: string } | null; step: { index: number; total: number; name: string } | null;
  started_at: number | null; elapsed_secs: number | null;
}
export interface State {
  settings: Settings; settings_path: string; settings_exists: boolean;
  marketplace: { path: string; url: string; exists: boolean; refreshing: boolean; last_refresh: number | null; error: { code: string; message: string } | null };
  apps: AppState[]; operation: { app_id: string; kind: string } | null; busy: boolean;
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
  pickDirectory: (target: "root_parent") => invoke<string | null>("pick_directory", { target }),
  setActiveTab: (app_id: string | null) => invoke<State>("set_active_tab", { app_id }),
  closeTab: (app_id: string) => invoke<State>("close_tab", { app_id }),
  openAppTab: (app_id: string) => invoke<State>("open_app_tab", { app_id }),
  refreshMarketplace: () => invoke<State>("refresh_marketplace"),
  checkUpdates: () => invoke<State>("check_updates"),
  installApp: (app_id: string, approve_effects: boolean) => invoke<State>("install_app", { app_id, approve_effects }),
  updateApp: (app_id: string, approve_effects: boolean) => invoke<State>("update_app", { app_id, approve_effects }),
  updateAll: (approve_effects: boolean) => invoke<State>("update_all", { approve_effects }),
  startApp: (app_id: string) => invoke<State>("start_app", { app_id }),
  stopApp: (app_id: string) => invoke<State>("stop_app", { app_id }),
  openApp: (app_id: string) => invoke<State>("open_app", { app_id }),
  removeApp: (app_id: string, confirm: string) => invoke<State>("remove_app", { app_id, confirm }),
  saveAppSettings: (app_id: string, settings: Record<string, unknown>) => invoke<State>("save_app_settings", { app_id, settings }),
  cancelOperation: (app_id: string) => invoke<State>("cancel_operation", { app_id }),
  revealPath: (target: "root" | "marketplace" | "app" | "settings", app_id?: string) => invoke<void>("reveal_path", { target, ...(app_id ? { app_id } : {}) }),
  openUrl: (url: string) => invoke<void>("open_url", { url }),
  exitApp: () => invoke<void>("exit_app"),
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
