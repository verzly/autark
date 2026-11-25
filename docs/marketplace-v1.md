# Autark marketplace v1

`verzly/autark-marketplace` is a public, MIT-licensed repo. It is the only place where app knowledge lives:
Autark ships no app-specific code, only the machinery that reads these manifests.

Autark clones the repo into `<root>/marketplace` on first use and refreshes it with
`git fetch --depth 1 origin main` + `git reset --hard FETCH_HEAD`. Refreshes are always manual.

## Repo layout

```
marketplace.json          index; lists app ids in display order
apps/<id>/app.json        one manifest per app
apps/<id>/icon.png        optional 256x256 icon, referenced by the manifest
LICENSE                   MIT
README.md
```

`marketplace.json`:

```json
{ "schema": 1, "name": "Autark Marketplace", "apps": ["unsloth-studio"] }
```

## app.json

```json
{
  "schema": 1,
  "id": "unsloth-studio",
  "name": "Unsloth Studio",
  "description": "Fine-tune and run local models with a bundled web UI.",
  "version": "2026.11.0",
  "homepage": "https://github.com/unslothai/unsloth",
  "upstream": "unslothai/unsloth",
  "license": "AGPL-3.0",
  "icon": "icon.png",
  "requirements": { "disk_gb": 12, "webview2": true },
  "settings": [
    {
      "key": "port",
      "label": "Port",
      "kind": "port",
      "default": 8888,
      "help": "Local port for the app web UI."
    }
  ],
  "setup": {
    "tools": { "python": "3.13", "uv": "0.13.1", "node": "24", "bun": "1", "cmake": "latest" },
    "source": { "url": "https://github.com/unslothai/unsloth.git", "branch": "main" },
    "installed_check": "{app}/app/unsloth_studio/Scripts/python.exe",
    "version_check": ["{app}/app/unsloth_studio/Scripts/python.exe", "-I", "-c", "import importlib.metadata as m; print(m.version('unsloth'))"],
    "steps": [
      {
        "name": "Running app setup",
        "run": ["powershell", "-NoProfile", "-ExecutionPolicy", "Bypass", "-File", "{source}/install.ps1", "--local", "--skip-autostart", "--isolated-uv-cache"]
      }
    ]
  },
  "launch": {
    "run": ["{app}/app/unsloth_studio/Scripts/python.exe", "-I", "-m", "unsloth_cli", "studio", "-H", "127.0.0.1", "-p", "{settings.port}"],
    "stop": ["{app}/app/unsloth_studio/Scripts/python.exe", "-I", "-m", "unsloth_cli", "studio", "shutdown"],
    "url": "http://127.0.0.1:{settings.port}"
  },
  "env": { "UNSLOTH_SKIP_AUTOSTART": "1", "HF_HOME": "{app}/huggingface" },
  "env_keep_parent": ["HF_HOME", "HF_HUB_CACHE", "HUGGINGFACE_HUB_CACHE"]
}
```

### Fields

| Field | Required | Meaning |
| --- | --- | --- |
| `schema` | yes | `1` |
| `id` | yes | `[a-z0-9-]+`, must equal the folder name, stable forever |
| `name` | yes | Display name used in the tab strip |
| `description` | yes | One sentence, shown in the `+` chooser |
| `version` | yes | Recipe version owned by the marketplace; bump in the same PR that changes behavior |
| `homepage` | no | https link, opened from the app panel |
| `upstream` | no | GitHub `owner/repository`; latest release or tag can enable app-version updates |
| `license` | no | SPDX id of the app itself |
| `icon` | no | File next to `app.json`; png, 256x256 |
| `requirements` | no | `disk_gb` (number), `webview2` (bool); informational only, shown before install |
| `faq` | no | Array of `{ "question", "answer" }`; shown on the app page. An empty or missing list hides the FAQ section. |
| `settings` | no | Per-app user settings, see below |
| `setup` | yes | Everything needed to make the app runnable, see below |
| `setup.tools` | no | Map of mise tool → version. Written to `<app>/mise.toml` and installed with `mise install` |
| `setup.source` | no | Git source cloned into `<app>/source`; refreshed with fetch+reset, never re-cloned |
| `setup.installed_check` | no | Path template; when set, the app counts as installed only if the path exists |
| `setup.version_check` | no | Argument vector; its last non-empty stdout line is stored as the app's real version |
| `setup.steps` | yes | Ordered commands run after tool install and source fetch |
| `launch` | no | Command used by Start, optional `stop` for a graceful shutdown, plus the optional local URL for Open. Omit it when the app has nothing to start |
| `actions` | no | App-specific buttons and commands |
| `env` | no | Extra environment variables for setup steps and launch |
| `env_keep_parent` | no | Env names that keep the parent process value when the machine already sets any of them |

Hindsight settings provide **Test** for endpoint reachability (`OK: HTTP <status>`, including 401/403; `FAIL` if the request cannot complete) and **Check** for the configured model. Model Check requests `${llm_base_url}/models` and returns `OK: Model available`, `FAIL: Model not found: <id>`, or the first line of an HTTP error body. **Models** lists the model IDs that endpoint reports; click one to use it as the model ID. Checks use current on-screen settings and Windows `curl.exe`; the base URL must include its OpenAI-compatible path, such as `/v1` (the recipe default does).

### Settings

```json
{ "key": "port", "label": "Port", "kind": "port", "default": 8888, "help": "..." }
```

`kind` is one of `text`, `number`, `port`, `boolean`, `path`, `file`. `path` opens a folder picker; `file` opens a file picker. Keys must be unique per app. Autark stores the
values in `<app>/settings.json` next to the installed manifest and exposes them to commands through
`{settings.<key>}`. Values are validated before an operation starts: a `port` must be 1-65535. A default value
may itself contain placeholders, for example `"default": "{app}/huggingface"`.

A `path` or `file` setting is stored portably: a path picked (or typed) inside the app folder is rewritten to the
`{app}/...` form before it is saved, so the value keeps working when the Autark root moves.

An optional `"locked_by": ["HF_HOME"]` hands the setting to the machine: when any listed variable exists in the
environment, the app settings page shows that variable's value read-only with a note that global system settings
overrode Autark's scope, the stored value is refused on save, and `env_keep_parent` keeps the variable unchanged
for setup steps and launch.

### Templates

`run` entries, `env` values and `installed_check` accept these placeholders:

| Placeholder | Resolves to |
| --- | --- |
| `{root}` | Autark root |
| `{app}` | `<root>/apps/<id>` |
| `{source}` | `<root>/apps/<id>/source` |
| `{data}` | `<root>/apps/<id>/user-data` |
| `{settings.<key>}` | current value of that app setting |

An unknown placeholder is a manifest error and fails the operation before anything changes.

### Commands

`run` is an argv array. The first item is the program; the rest are literal arguments. Autark does not use a
shell, so no quoting rules apply. When `setup.tools` is non-empty every command runs through
`mise exec --` from `<app>`, which puts the pinned tools on `PATH`.

Setup steps run in order. A step with `"optional": true` may fail without failing the install. A step with
`"when_setting": "key"` runs only when that boolean app setting is on, so an app can offer opt-in work such as
copying an existing user config into the install. Every step streams its output to the app log.

### Launch, stop and Open

`launch` is optional. With a `run` vector Autark shows Start and the app runs until Stop; without it the app has
no Start button at all, which suits recipes that only install tools or expose actions. `launch.url` adds an Open
button that points at that address once the app is running.

Stop never needs a command, because Autark kills the process tree it started. An optional `launch.stop` argv runs
first, so an app that detaches itself or needs a graceful shutdown can clean up before the tree is killed. A stop
command that fails is logged and does not block the kill.

### Actions

Each action has a stable `id` matching `[a-z0-9-]+`, a `label`, and a `run` argument vector. Arguments use usual
placeholders and run through `mise exec` from the app folder when tools are configured. Optional `url` opens
after success; optional `confirm` asks before destructive actions.

```json
"actions": [
  { "id": "refresh", "label": "Refresh data", "run": ["python", "refresh.py"] },
  { "id": "reset", "label": "Reset data", "run": ["python", "reset.py"], "confirm": true }
]
```

`env_keep_parent` names variables that win over the manifest. When the machine already exports any listed name
(for example `HF_HOME`), Autark passes it through unchanged and skips every manifest value from that group.
Otherwise the folder-local values from `env` apply, which keeps models in `{app}` by default.

## Update semantics

- Installed version is recorded in `<app>/installed.json` after a successful setup.
- Latest version is `app.json.version` from the refreshed marketplace.
- `update available` means the marketplace recipe version is strictly greater than the installed version. Only
  the marketplace decides when an app is republished, so bump `version` on every behavior change.
- Update re-runs the whole setup (source refresh, tool install, steps) and reports the new installed version.
- An app cannot be started while it updates; Start resumes after the update.

## One marketplace list

Autark keeps a single app list. The selected branch or pull request only changes where the catalogue data is
read from: entries keep their order, and an app known from an earlier source stays in the list when the
current source omits it. The choice is stored in the settings, so a restart continues from the same branch or
pull request; the Marketplace panel shows a banner with a link to it and a one-click return to the default
branch. While a preview source is active, every installed app shows a reload icon that copies
the source's `app.json` (and icon) into the app folder without running setup, so a recipe change arrives
without downloads or reinstalling. `Update all` stays version-gated: it only takes an app whose marketplace
recipe version is strictly greater than the installed one.

## Local apps

A user-supplied `app.json` is copied into `<root>/local-apps/<key>/`. The marketplace version column shows
`local`; local apps never receive marketplace updates. Duplicate ids are allowed and receive a `-2` suffix
and higher suffixes. Re-uploading a file replaces an existing local entry.

## Security rules for marketplace authors

- Only https URLs, only public repos.
- Setup steps run with the user's own permissions. Never assume admin rights, never write outside `{app}`
  except through the app's own installer, and never disable antivirus or firewall settings.
- Keep machine-global side effects at zero: tools come from mise into the app folder, caches live in the app
  folder, no PATH edits, no registry writes.

## Validation checklist for a PR

1. `app.json` parses, `id` matches the folder, `schema` is 1.
2. Every `{placeholder}` is on the list above.
3. `run` arrays are non-empty and start with a program name.
4. `version` changed when behavior changed.
5. README table updated.
