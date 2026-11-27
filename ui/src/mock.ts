import type { State } from "./bridge";
const common = { settings: {}, settings_schema: [], settings_locked: [], prefs: { auto_start: false, shortcut_autostart: false, shortcut_background: false }, actions: [], can_start: true, homepage: "https://github.com/verzly/autark", license: "MIT", icon_path: null, icon_data: null, version_app: null, upstream: null, requirements: { disk_gb: 2, webview2: true }, faq: [], last_error: null, step: null, started_at: null, elapsed_secs: null, local: false, preview: null, restore_available: false };
export const previewState: State = {
  settings: { root: "D:\\Portable\\Autark", folder_name: "Autark", force_local_mise: true, animations: true, open_tabs: ["unsloth-studio", "cognee"], active_tab: "unsloth-studio", tray_enabled: false, start_with_windows: false, close_to_tray: false, experimental_local_apps: false, wizard_complete: true, terminal: "system", developer_mode: true, force_terminal_operations: false },
  settings_path: "D:\\Portable\\Autark\\launcher-settings.json", settings_exists: true,
  mise: { available: true, source: "local", path: "C:\\Portable\\Autark\\bin\\mise.exe", version: "2026.11.1", local_path: "C:\\Portable\\Autark\\bin\\mise.exe", bin_path: "C:\\Portable\\Autark\\bin", bin_in_path: false, bin: [{ name: "mise.exe", path: "C:\\Portable\\Autark\\bin\\mise.exe", version: "2026.11.1", size: 41231360 }], detected: [{ path: "C:\\Program Files\\mise\\mise.exe", version: "2026.11.2" }], force_local: false },
  git: { installed: true, version: "2.50.1", path: "C:\\Portable\\Autark\\runtime\\mise-data\\installs\\git\\2.50.1\\cmd\\git.exe", external: { path: "C:\\Program Files\\Git\\cmd\\git.exe", version: "2.49.0", from_mise: false } },
  terminal: { bash: "C:\\Portable\\Autark\\runtime\\mise-data\\installs\\git\\2.50.1\\bin\\bash.exe", sh: null },
  pg0: { available: false, version: null, path: null, installation: "D:\\Portable\\Autark\\runtime\\pg0\\installation", source: "D:\\Portable\\Autark\\apps\\hindsight\\venv\\Lib\\site-packages\\pg0\\bin\\pg0.exe" },
  marketplace: { path: "D:\\Portable\\Autark\\marketplace", url: "https://github.com/verzly/autark-marketplace", exists: true, refreshing: false, last_refresh: 1794931200, source: "", error: null },
  busy: false, operations: [],
  apps: [
    { ...common, id: "unsloth-studio", name: "Unsloth Studio", description: "Local interface for fine-tuning and running language models.", version_installed: "1.2.3", version_latest: "1.2.4", update_available: true, installed: true, phase: "running", preview: "feature/new-tuning", restore_available: true, url: "http://127.0.0.1:8888", actions: [{ id: "download-model", label: "Download model", confirm: false }, { id: "clear-cache", label: "Clear cache", confirm: true }], settings: { port: 8888, data_dir: "D:\\Portable\\Autark\\apps\\unsloth-studio\\data", hf_home: "D:\\models\\huggingface" }, settings_locked: [{ key: "hf_home", var: "HF_HOME", value: "D:\\models\\huggingface" }], settings_schema: [{ key: "port", label: "Port", kind: "port", default: 8888, help: "Local web interface port." }, { key: "data_dir", label: "Data folder", kind: "path", default: "" }, { key: "hf_home", label: "Hugging Face folder", kind: "path", default: "" }] },
    { ...common, id: "cognee", name: "Cognee", description: "Build and query a memory graph from your data.", version_installed: "0.4.1", version_latest: "0.4.1", update_available: false, installed: true, phase: "installed", url: null, license: "Apache-2.0" },
    { ...common, id: "local-assistant", name: "Local Assistant", description: "A private assistant that runs on your machine.", version_installed: null, version_latest: "0.1.0", update_available: false, installed: false, phase: "not_installed", url: null },
    { ...common, id: "my-local-app", name: "My Local App", description: "An app from a manifest you supplied.", local: true, version_installed: null, version_latest: null, update_available: false, installed: false, phase: "not_installed", url: null },
  ],
};
export const previewLogs = ["Preparing managed environment", "mise install", "  installing python@3.12.8", "Installing application dependencies", "Starting local service", "Ready at http://127.0.0.1:8888"];
export const previewVariants = {
  installing: { ...previewState.apps[2], phase: "installing" as const, step: { index: 3, total: 5, name: "Installing tools" }, started_at: 1794931200 },
  failed: { ...previewState.apps[2], phase: "failed" as const, last_error: { code: "start_failed", message: "The app exited before its local service became available." } },
  notInstalled: { ...previewState.apps[2], phase: "not_installed" as const, installed: false },
};
