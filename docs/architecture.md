# Autark architecture

Autark is one portable Windows executable: Tauri 2 window, SolidJS UI, Rust core. It installs and runs
apps that are described by the marketplace (see `docs/marketplace-v1.md`). Autark itself contains no app.

## Portable layout

```
<exe folder>/launcher-root.json      pointer: { "root": "<absolute path>" }
<root>/launcher-settings.json        Autark settings (root, folder name, open tabs)
<root>/marketplace/                  git clone of verzly/autark-marketplace
<root>/local-apps/<key>/             user-supplied app.json and optional icon
<root>/runtime/mise-data|mise-config|mise-cache|mise-state|tmp|bin
<root>/bin/                         Autark-owned portable binaries, including mise.exe and bin.exe
<root>/apps/<id>/app.json            manifest snapshot copied at install time
<root>/apps/<id>/installed.json      { "version": "recipe", "app_version": "real", "installed_at": 0 }
<root>/apps/<id>/settings.json       per-app user settings
<root>/apps/<id>/launcher.json       launcher preferences (auto-start and shortcut flags)
<root>/apps/<id>/mise.toml           generated from setup.tools
<root>/apps/<id>/source/             upstream git source (optional)
<root>/apps/<id>/user-data/          app HOME/APPDATA sandbox, survives removal
<root>/apps/<id>/runtime/            per-app temp + caches
```

`launcher-root.json` lives next to the exe when that folder is writable, otherwise in
`%LOCALAPPDATA%/Autark/`. Everything else moves with `root`.

Autark owns binaries in `<root>/bin`; it does not install tools machine-wide. Mise resolution prefers
`<root>/bin/mise.exe`, then legacy `<root>/runtime/bin/mise.exe`, then an existing `Settings.mise_path`,
`MISE_BIN`, and `mise` on PATH. `force_local_mise` prevents fallback when no local binary exists.
`Settings.bin_in_path` records the requested registration; `State.mise.bin_in_path` reports actual HKCU PATH state.
`State.mise` includes availability, source, path, version, local/bin paths, built-in binary entries, and force-local state.

When an app runs, its environment is scoped like this:

| Variable | Value |
| --- | --- |
| `HOME`, `USERPROFILE`-derived app home | `{app}/app` |
| `APPDATA` | `{app}/user-data/roaming` |
| `LOCALAPPDATA` | `{app}/user-data/local` |
| `TEMP`, `TMP` | `{app}/runtime/tmp` |
| `MISE_DATA_DIR` | `{root}/runtime/mise-data` |
| `MISE_CONFIG_DIR` | `{root}/runtime/mise-config` |
| `MISE_CACHE_DIR` | `{root}/runtime/mise-cache` |
| `MISE_STATE_DIR` | `{root}/runtime/mise-state` |
| `BIN_INSTALL_DIR` | `{root}/bin` |
| `BIN_CONFIG` | `{root}/runtime/bin-config/config.json` |
| `UV_CACHE_DIR` | `{app}/runtime/uv-cache` |

The parent environment is cleared first and only a whitelist is passed through (PATH, SystemRoot, WINDIR,
COMSPEC, PATHEXT, SystemDrive, ProgramData, ProgramFiles(x86), NUMBER_OF_PROCESSORS, USERNAME, TEMP/TMP
sources). `{root}/bin` is prepended to PATH, so setup steps and launched apps call bin and the binaries it
installs by name. Manifest `env` entries are applied last. `root` is configurable and lives in settings.

## crates

- `settings.rs` — root pointer, `launcher-settings.json`, folder rename rules.
- `local.rs` — user-supplied local app manifests.
- `manifest.rs` — marketplace index + `app.json` parsing, validation, template expansion.
- `marketplace.rs` — clone/fetch/reset of the marketplace repo, catalogue scan, version compare.
- `apps.rs` — per-app record (installed version, settings, phase, child process), install/update/start/
  stop/remove, log streaming, mise bootstrap, `update_all` queue.
- `main.rs` — Tauri commands, state aggregation, events.

`State` is the single source of truth for the UI. Every command returns the new `State` and Rust also emits
a `state` event on every change. Log lines are emitted as `log` events and are not persisted.

## IPC v1

Commands (`invoke`, snake_case args):

| Command | Args | Returns |
| --- | --- | --- |
| `get_state` | — | `State` |
| `save_settings` | `settings`, `move_folder?` | `State` |
| `reset_autark` | `confirm` | `State`; requires typed word `RESET`, deletes everything inside Autark root, and reopens wizard |
| `set_mise_path` | `path: string \| null` | `State` |
| `install_mise` | — | `State` |
| `update_mise` | — | `State` |
| `set_bin_path` | `enabled: bool` | `State` |
| `remove_bin_entry` | `name` | `State` |
| `pick_directory` | `target: "root_parent"` | `string \| null` |
| `pick_file` | `title` | `string \| null` |
| `add_local_app` | `path` | `State` |
| `replace_local_app` | `app_id`, `path` | `State` |
| `set_active_tab` | `app_id: string \| null` | `State` |
| `close_tab` | `app_id` | `State` |
| `open_app_tab` | `app_id` | `State` |
| `refresh_marketplace` | — | `State` |
| `check_updates` | — | `State` |
| `install_app` | `app_id`, `approve_effects: bool` | `State` |
| `update_app` | `app_id`, `approve_effects: bool` | `State` |
| `update_all` | `approve_effects: bool` | `State` |
| `start_app` | `app_id` | `State` |
| `run_action` | `app_id`, `action_id` | `State` |
| `stop_app` | `app_id` | `State` |
| `open_app` | `app_id` | `State` |
| `remove_app` | `app_id`, `confirm: string` | `State` |
| `save_app_settings` | `app_id`, `settings: Record<string, unknown>` | `State` |
| `probe_llm` | `app_id`, `check: "endpoint" \| "model" \| "models"`, optional `settings` map (unsaved drafts win over stored values) | `endpoint`: `HTTP <status>`; `model`: `Model available`, or the command fails with `Model not found: <id>`; `models`: JSON array of model IDs from `<llm_base_url>/models` |
| `save_app_prefs` | `app_id`, `prefs` | `State` |
| `create_shortcut` | `app_id` | `State` |
| `cancel_operation` | `app_id` | `State` |
| `reveal_path` | `target: "root" \| "marketplace" \| "app" \| "settings"`, `app_id?` | — |
| `open_url` | `url` | — |
| `exit_app` | — | — |

Errors are `{ code, message }` with `code` in `operation_in_progress`, `approval_required`,
`confirm_required`, `invalid_argument`, `marketplace_missing`, `app_not_found`, `internal`.

Events: `state: State`, `log: { app_id: string, line: string }`.

### Types

```ts
type Phase =
  | "not_installed" | "installing" | "installed" | "starting"
  | "running" | "stopping" | "updating" | "removing" | "failed";

interface Settings {
  root: string;
  folder_name: string;
  force_local_mise: boolean;
  mise_path: string | null;
  bin_in_path: boolean;
  open_tabs: string[];          // app ids, tab strip order
  active_tab: string | null;
  tray_enabled: boolean;
  start_with_windows: boolean;
  close_to_tray: boolean;
  experimental_local_apps: boolean;
  wizard_complete: boolean;
}

interface AppSetting { key: string; label: string; kind: "text" | "number" | "port" | "boolean" | "path" | "file"; default: unknown; help?: string | null; group?: string | null }

interface AppState {
  id: string; name: string; description: string; local: boolean;
  version_installed: string | null;   // recipe version recorded at install
  version_app: string | null;         // real app version read by setup.version_check
  version_latest: string | null;      // latest recipe version from the marketplace
  update_available: boolean;
  installed: boolean;
  phase: Phase;
  settings: Record<string, unknown>;
  prefs: { auto_start: boolean; shortcut_autostart: boolean; shortcut_background: boolean };
  settings_schema: AppSetting[];
  actions: { id: string; label: string; confirm: boolean }[];
  can_start: boolean; // false when the manifest declares no launch.run, so Start/Stop are hidden
  url: string | null;            // running app URL, else null
  homepage: string | null;
  license: string | null;
  icon_path: string | null;
  requirements: { disk_gb: number | null; webview2: boolean };
  faq: { question: string; answer: string }[];
  last_error: { code: string; message: string } | null;
  step: { index: number; total: number; name: string } | null;
  started_at: number | null;     // unix seconds
  elapsed_secs: number | null;
}

interface State {
  settings: Settings;
  settings_path: string;
  settings_exists: boolean;
  marketplace: {
    path: string;
    url: string;
    exists: boolean;
    refreshing: boolean;
    last_refresh: number | null;
    error: { code: string; message: string } | null;
  };
  apps: AppState[];              // catalogue order, includes not-installed apps
  operation: { app_id: string; kind: string } | null;
  busy: boolean;                 // any operation running, including update_all
  mise: { available: boolean; source: string; path: string | null; version: string | null; local_path: string; bin_path: string; bin_in_path: boolean; bin: { name: string; path: string; version: string | null; size: number }[]; force_local: boolean };
}
```

## Behavior rules

- One mutating operation per app at a time; `update_all` runs apps sequentially and refuses to start while
  another operation holds the queue.
- Start is refused while installing/updating/removing. Installing, updating and removing stop the app first.
- An app with a running child cannot be updated or started again: its phase reports the process truth.
- A child that exits on its own turns the phase into `failed` with `start_failed`, never a stuck `running`.
- Removing deletes everything inside `<app>` except `user-data` and `settings.json`. The tab stays open with
  the Install action.
- Marketplace refresh is manual. Without a marketplace clone, `apps` is empty and the UI offers "Load
  marketplace" instead of a crash.
- Local apps are never updated from the marketplace. Duplicate ids receive `-2`, `-3`, and later suffixes.
- The `+` chooser lists installed apps first, then marketplace apps; a marketplace app that is not installed
  gets a tab with the Install action.
- Closing hides to tray only when `close_to_tray` and `tray_enabled` are both true; otherwise it stops every child and exits.
- Tray can show/focus Autark or stop all children and quit. `start_with_windows` adds an HKCU Run entry only when tray is enabled. Optional PATH registration adds only `<root>/bin` to the HKCU user PATH; these are Autark's only registry writes.
- CLI accepts `--app <id>`, `--start`, and `--background`. Shortcuts launch Autark with these flags from per-app preferences.

## UI contract

OpenCode Desktop visual language: near-black window (`#0a0a0a`), a top chrome row holding the app tab strip
and the window controls, and one rounded content card (`#0d0d0d`, 1px `#262626` border, 12px radius) inset
by 8px that fills the rest of the window. Muted grays for secondary text, one accent color, no browser
chrome, no scrollbars on the chrome row.

- Tab strip: one tab per open app (18px icon + name + status dot), `+` button at the end, then
  minimize/maximize/close on the right. Active tab lighter than the strip.
- `+` opens a centred chooser overlay with a search field, an "Installed" group and a "Marketplace" group
  with a refresh action, plus the last refresh time.
- App panel: title row with name and version (`1.2.3` / `1.2.4` with an "update" badge), status line with
  step progress and elapsed time, action row (Install/Update, Start, Stop, Open, Remove), settings fields,
  output console (monospace, capped height, auto-follow, Clear).
- Empty state: wordmark `autark`, one line of copy, and the `+` chooser trigger.
- A settings tab (gear icon left of the tab strip) holds root folder, marketplace refresh, "Update all",
  version list per installed app, license/source links.
