import type { State } from "./bridge";
const common = { settings: {}, settings_schema: [], homepage: "https://github.com/verzly/autark", license: "MIT", icon_path: null, version_app: null, requirements: { disk_gb: 2, webview2: true }, last_error: null, step: null, started_at: null, elapsed_secs: null };
export const previewState: State = {
  settings: { root: "D:\\Portable\\Autark", folder_name: "Autark", force_local_mise: true, open_tabs: ["unsloth-studio", "cognee"], active_tab: "unsloth-studio" },
  settings_path: "D:\\Portable\\Autark\\launcher-settings.json", settings_exists: true,
  marketplace: { path: "D:\\Portable\\Autark\\marketplace", url: "https://github.com/verzly/autark-marketplace", exists: true, refreshing: false, last_refresh: 1794931200, error: null },
  busy: false, operation: null,
  apps: [
    { ...common, id: "unsloth-studio", name: "Unsloth Studio", description: "Local interface for fine-tuning and running language models.", version_installed: "1.2.3", version_latest: "1.2.4", update_available: true, installed: true, phase: "running", url: "http://127.0.0.1:8888", settings: { port: 8888, data_dir: "D:\\Portable\\Autark\\apps\\unsloth-studio\\data" }, settings_schema: [{ key: "port", label: "Port", kind: "port", default: 8888, help: "Local web interface port." }, { key: "data_dir", label: "Data folder", kind: "path", default: "" }] },
    { ...common, id: "cognee", name: "Cognee", description: "Build and query a memory graph from your data.", version_installed: "0.4.1", version_latest: "0.4.1", update_available: false, installed: true, phase: "installed", url: null, license: "Apache-2.0" },
    { ...common, id: "local-assistant", name: "Local Assistant", description: "A private assistant that runs on your machine.", version_installed: null, version_latest: "0.1.0", update_available: false, installed: false, phase: "not_installed", url: null },
  ],
};
export const previewLogs = ["Preparing managed environment", "mise install", "  installing python@3.12.8", "Installing application dependencies", "Starting local service", "Ready at http://127.0.0.1:8888"];
export const previewVariants = {
  installing: { ...previewState.apps[2], phase: "installing" as const, step: { index: 3, total: 5, name: "Installing tools" }, started_at: 1794931200 },
  failed: { ...previewState.apps[2], phase: "failed" as const, last_error: { code: "start_failed", message: "The app exited before its local service became available." } },
  notInstalled: { ...previewState.apps[2], phase: "not_installed" as const, installed: false },
};
