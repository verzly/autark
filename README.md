# Autark

Portable Windows launcher for apps that install themselves into one folder.

Autark is a single `autark.exe` with no installer, no service and no machine-wide
state. Next to the exe it keeps a pointer file, and everything an app needs lives
under a root folder you choose: its own toolchain, its own `HOME`, `APPDATA`,
`TEMP` and cache directories, its own user data. Delete the folder and the app is
gone.

Autark ships no apps. Apps are recipes in
[verzly/autark-marketplace](https://github.com/verzly/autark-marketplace): public,
MIT licensed, cloned on demand and refreshed manually. A recipe pins the tools it
needs with `mise`, declares its settings, its setup steps and its launch command.

## How it works

- **Tabbed launcher.** The window follows the OpenCode Desktop layout: one tab per
  open app, `+` at the end of the strip to add an app from your installed apps or
  from the marketplace.
- **Marketplace first.** `+` refreshes the marketplace clone, then lists every
  recipe it publishes. Nothing is bundled with Autark.
- **Folder-local by construction.** Each app gets `<root>/apps/<id>` with its own
  `source/`, `user-data/`, `runtime/` and `mise.toml`. Autark clears the parent
  environment and passes only a small whitelist, so a stray `PYTHONHOME` or
  `HF_HOME` cannot leak in.
- **Manual versions.** Each app shows its installed version and the marketplace
  version. Version checks and updates are explicit, never background jobs.
- **Update all.** Manual. Running apps are stopped, updated one at a time and stay
  unusable until their update finishes.

```
<exe folder>/launcher-root.json     pointer to the root folder
<root>/launcher-settings.json
<root>/marketplace/                 clone of the marketplace
<root>/runtime/                     shared mise state, cache and temporary files
<root>/apps/<id>/                   one app: recipe snapshot, settings, source, data
```

## Requirements

- Windows 10/11 (x64) with WebView2 (present on current Windows builds).
- `git` on `PATH` for the marketplace clone and marketplace-declared sources.
- `mise` is downloaded into the root folder on first use when it is missing.

## Building

```powershell
pnpm --dir ui install
pnpm --dir ui build
cargo build --release
```

The binary lands in `target\release\autark.exe`. Releases are built by
`.github/workflows/release.yml`; run `scripts/calver.ps1` locally if you need to
prepare a version bump by hand.

## Repository

- `docs/marketplace-v1.md` — marketplace layout and `app.json` schema.
- `docs/architecture.md` — portable layout, IPC contract, app state machine.
- `ui/` — SolidJS frontend (static build, no runtime dependency).
- `src/` — Tauri 2 host: settings, manifest, marketplace, app lifecycle.

## License

AGPL-3.0. See [LICENSE](LICENSE). The marketplace is MIT licensed.
