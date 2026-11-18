import { createEffect, createMemo, createSignal, For, onCleanup, onMount, Show } from "solid-js";
import type { AppSetting, AppState, Phase, State } from "./bridge";
import { api, mockMode } from "./bridge";
import { previewLogs, previewState, previewVariants } from "./mock";
import miseLogo from "./mise.png";

type InlineAction = "install" | "reinstall" | "remove" | `action:${string}` | null;
const working = (p: Phase) => ["installing", "starting", "stopping", "updating", "removing"].includes(p);
const errText = (e: unknown) => {
  if (e instanceof Error) return e.message;
  if (e && typeof e === "object" && "message" in e) return String((e as { message: unknown }).message);
  return String(e);
};
const hue = (id: string) => { let h = 0; for (const c of id) h = (h * 31 + c.charCodeAt(0)) | 0; return Math.abs(h) % 360; };
const duration = (s: number) => `${Math.floor(s / 60)}m ${String(Math.floor(s % 60)).padStart(2, "0")}s`;
const calver = (version: string | null) => version ? version.match(/^\d+\.\d+\.\d+/)?.[0] ?? version : "version unknown";
const AUTARK_TAB = "__autark__";

export default function App() {
  const [state, setState] = createSignal<State>(previewState);
  const [active, setActive] = createSignal<string | null>(previewState.settings.active_tab ?? AUTARK_TAB);
  const [settingsOpen, setSettingsOpen] = createSignal(false);
  const [settingsTab, setSettingsTab] = createSignal<"General" | "Apps" | "Display" | "Experimental" | "About">("General");
  const [settingsReturn, setSettingsReturn] = createSignal<string | null>(null);
  const [appSettingsOpen, setAppSettingsOpen] = createSignal<string | null>(null);
  const [chooser, setChooser] = createSignal(false);
  const [chooserReturn, setChooserReturn] = createSignal<string | null>(null);
  const [search, setSearch] = createSignal("");
  const [selected, setSelected] = createSignal(0);
  const [panel, setPanel] = createSignal<InlineAction>(null);
  const [removeText, setRemoveText] = createSignal("");
  const [requestError, setRequestError] = createSignal("");
  const [busy, setBusy] = createSignal(false);
  const [buffers, setBuffers] = createSignal<Record<string, string[]>>(mockMode() ? { "unsloth-studio": previewLogs } : {});
  const [follow, setFollow] = createSignal(true);
  const [drafts, setDrafts] = createSignal<Record<string, Record<string, unknown>>>({});
  const [root, setRoot] = createSignal("");
  const [allConfirm, setAllConfirm] = createSignal(false);
  const [localAppsConfirm, setLocalAppsConfirm] = createSignal(false);
  const [binRemoveConfirm, setBinRemoveConfirm] = createSignal<string | null>(null);
  const [wizardStep, setWizardStep] = createSignal(1);
  const [resetConfirm, setResetConfirm] = createSignal(false);
  const [resetText, setResetText] = createSignal("");
  // A mise that lives outside Autark's bin still gets a card, otherwise Built-in looks empty.
  const builtinApps = createMemo(() => {
    const mise = state().mise;
    const entries = mise.bin.slice();
    const hasLocalMise = entries.some(entry => entry.name.toLowerCase() === "mise.exe");
    const path = mise.path;
    if (!hasLocalMise) entries.unshift({ name: "mise.exe", path: path ?? "", version: mise.version, size: 0 });
    return entries.filter(entry => `${entry.name} ${entry.version ?? ""}`.toLowerCase().includes(search().toLowerCase()));
  });
  const isResolvedMise = (name: string) => name.toLowerCase() === "mise.exe" && !state().mise.bin.some(entry => entry.name.toLowerCase() === "mise.exe");
  const [marketFilter, setMarketFilter] = createSignal<"builtin" | "marketplace" | "local">("marketplace");
  const [clock, setClock] = createSignal(Date.now() / 1000);
  let dialogInput: HTMLInputElement | undefined;
  let output: HTMLDivElement | undefined;
  const currentApp = createMemo(() => state().apps.find(a => a.id === active()) ?? null);
  const openIds = () => state().settings.open_tabs;
  const [autarkTab, setAutarkTab] = createSignal(true);
  // Settings can reopen the mise screen instead of duplicating the picker.
  const [miseScreen, setMiseScreen] = createSignal(false);
  const [builtinTabs, setBuiltinTabs] = createSignal<string[]>([]);
  const [shortcutDone, setShortcutDone] = createSignal<string[]>([]);
  const [shortcutBusy, setShortcutBusy] = createSignal<string[]>([]);
  const available = createMemo(() => state().apps.filter(a => `${a.name} ${a.description} ${a.id}`.toLowerCase().includes(search().toLowerCase())));
  const marketplaceApps = createMemo(() => state().apps.filter(a => (marketFilter() === "local" ? a.local : !a.local) && `${a.name} ${a.description} ${a.id}`.toLowerCase().includes(search().toLowerCase())));
  const showChooser = () => { setSettingsOpen(false); setActive(null); setChooser(true); setSearch(""); setSelected(0); };
  const hideChooser = () => { setChooser(false); setAutarkTab(true); setActive(AUTARK_TAB); setSearch(""); setSelected(0); };

  onMount(() => {
    let offState: (() => void) | undefined, offLog: (() => void) | undefined, offFocus: (() => void) | undefined;
    if (!mockMode()) api.getState().then(s => { setState(s); setActive(s.settings.active_tab ?? AUTARK_TAB); }).catch(e => setRequestError(errText(e)));
    api.onState(s => { setState(s); setRequestError(""); }).then(f => offState = f);
    api.onFocus(id => { if (!id) return; setSettingsOpen(false); setAppSettingsOpen(null); setChooser(false); setActive(id); }).then(f => offFocus = f);
    api.onLog(({ app_id, line }) => setBuffers(prev => ({ ...prev, [app_id]: [...(prev[app_id] ?? []), line].slice(-500) }))).then(f => offLog = f);
    const keys = (e: KeyboardEvent) => {
      if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "t" && !e.altKey && !e.shiftKey && !(e.target instanceof HTMLElement && ["INPUT", "TEXTAREA", "SELECT"].includes(e.target.tagName))) { e.preventDefault(); setAutarkTab(true); setActive(AUTARK_TAB); setChooser(false); return; }
      if (chooser()) chooserKeys(e);
    };
    window.addEventListener("keydown", keys);
    if ("__TAURI_INTERNALS__" in window) import("@tauri-apps/api/app").then(({ getVersion }) => getVersion().then(setVersion)).catch(() => {});
    onCleanup(() => { offState?.(); offLog?.(); window.removeEventListener("keydown", keys); });
  });
  const [version, setVersion] = createSignal("dev");
  createEffect(() => { if (chooser()) queueMicrotask(() => dialogInput?.focus()); });
  createEffect(() => { logs(); if (output && follow()) output.scrollTop = output.scrollHeight; });
  createEffect(() => {
    const app = currentApp();
    if (!app || !working(app.phase)) return;
    setClock(Date.now() / 1000);
    const id = window.setInterval(() => setClock(Date.now() / 1000), 1000);
    onCleanup(() => window.clearInterval(id));
  });
  const logs = () => buffers()[active() ?? ""] ?? [];
  const miseLogs = () => buffers()["mise.exe"] ?? [];
  // The download script prints "mise download <n>%", so the bar follows real bytes.
  const misePercent = () => {
    const lines = miseLogs();
    for (let i = lines.length - 1; i >= 0; i--) {
      const found = lines[i].match(/(\d{1,3})%/);
      if (found) return Number(found[1]);
    }
    return null;
  };
  // The bar runs 0 to 100 only while Autark downloads its own mise.
  const [miseInstalling, setMiseInstalling] = createSignal(false);
  const downloading = () => miseInstalling() && busy();
  const loadingPercent = () => (miseInstalling() ? (misePercent() ?? 0) : pickPercent());
  createEffect(() => { if (!busy()) { setMiseInstalling(false); setResetting(false); } });
  createEffect(() => document.body.classList.toggle("is-loading", miseInstalling() || misePicking() || resetting()));
  const localInstalled = () => state().mise.bin.some(entry => entry.name.toLowerCase() === "mise.exe");
  const localSelected = () => {
    const selected = state().mise.path;
    return !!selected && selected.toLowerCase() === state().mise.local_path.toLowerCase();
  };
  // Picking a working mise moves on, so the step is not a dead end.
  const [misePicking, setMisePicking] = createSignal(false);
  const [misePickTarget, setMisePickTarget] = createSignal<string | null>(null);
  const [resetting, setResetting] = createSignal(false);
  const [pickPercent, setPickPercent] = createSignal(0);
  createEffect(() => {
    if (!misePicking() && !resetting()) return;
    setPickPercent(0);
    const timer = setInterval(() => setPickPercent(value => (value >= 90 ? 90 : value + 6)), 40);
    onCleanup(() => clearInterval(timer));
  });
  const pickMise = async (path: string) => {
    setMisePicking(true);
    setMisePickTarget(path);
    try {
      const next = await api.setMisePath(path);
      if (!next.mise.available || next.settings.wizard_complete) return next;
      return await api.saveSettings({ ...next.settings, wizard_complete: true }, false);
    } finally {
      setMisePicking(false);
      setMiseScreen(false);
    }
  };
  const logEntries = () => {
    const lines = logs();
    return lines.flatMap((line, index) => {
      const notice = lineTone(line) === "tone-error" || lineTone(line) === "tone-warning" || /\btip\b/i.test(line);
      const previous = lines[index - 1] ?? "";
      const previousNotice = lineTone(previous) === "tone-error" || lineTone(previous) === "tone-warning" || /\btip\b/i.test(previous);
      const spacer = index > 0 && (/^\$ /.test(line) || (notice && !previousNotice));
      return [...(spacer ? [{ id: `${index}-spacer`, line: "", spacer: true }] : []), { id: `${index}-line`, line, spacer: false }];
    });
  };
  const run = async (fn: () => Promise<State | void>) => {
    setBusy(true); setRequestError("");
    try { const next = await fn(); if (next) { const current = active(); setState(next); if (!isBuiltin(current) && current !== AUTARK_TAB) setActive(next.settings.active_tab ?? (current === null ? null : AUTARK_TAB)); } }
    catch (e) { setRequestError(errText(e)); }
    finally { setBusy(false); }
  };
  const selectTab = (id: string | null) => {
    if (id === AUTARK_TAB) { setActive(AUTARK_TAB); setChooser(false); }
    else { setActive(id); setChooser(false); }
    setSettingsOpen(false);
    if (!mockMode()) void run(() => api.setActiveTab(id === AUTARK_TAB ? null : id));
  };
  const openApp = (id: string) => {
    if (openIds().includes(id)) {
      selectTab(id);
      setChooser(false);
      return;
    }
    if (mockMode()) { setState(s => ({ ...s, settings: { ...s.settings, open_tabs: [...s.settings.open_tabs, id], active_tab: id } })); setActive(id); }
    else void run(() => api.openAppTab(id));
    setAutarkTab(false);
    setChooser(false);
  };
  const closeTab = (id: string, event: MouseEvent) => {
    event.stopPropagation();
    if (mockMode()) {
      const tabs = openIds().filter(x => x !== id), nextActive = active() === id ? tabs[tabs.length - 1] ?? AUTARK_TAB : active();
      setState(s => ({ ...s, settings: { ...s.settings, open_tabs: tabs, active_tab: nextActive === AUTARK_TAB ? null : nextActive } }));
      if (active() === id) { setActive(nextActive); setChooser(false); }
      if (tabs.length === 0) { setAutarkTab(true); setActive(AUTARK_TAB); }
    } else void run(async () => {
      const next = await api.closeTab(id);
      if (active() === id) { const nextActive = next.settings.active_tab ?? AUTARK_TAB; setActive(nextActive); setChooser(false); }
      if (next.settings.open_tabs.length === 0) { setAutarkTab(true); setActive(AUTARK_TAB); }
      return next;
    });
  };
  const builtinName = (id: string | null) => (id ?? "").replace(/\.exe$/i, "");
  const isBuiltin = (id: string | null) => builtinTabs().includes(id ?? "");
  const openBuiltin = (name: string) => {
    if (!builtinTabs().includes(name)) setBuiltinTabs(tabs => [...tabs, name]);
    setActive(name); setAutarkTab(false); setChooser(false); setSettingsOpen(false); setAppSettingsOpen(null);
  };
  const closeBuiltin = (name: string, event: MouseEvent) => {
    event.stopPropagation();
    setBuiltinTabs(tabs => tabs.filter(x => x !== name));
    if (active() !== name) return;
    const last = openIds()[openIds().length - 1];
    if (last) { setActive(last); if (!mockMode()) void run(() => api.setActiveTab(last)); }
    else { setAutarkTab(true); setActive(AUTARK_TAB); }
  };
  const closeAutarkTab = (event: MouseEvent) => {
    event.stopPropagation();
    setAutarkTab(false);
    if (openIds().length === 0) { setAutarkTab(true); setActive(AUTARK_TAB); return; }
    if (active() === AUTARK_TAB) {
      const last = openIds()[openIds().length - 1];
      setActive(last);
      if (!mockMode()) void run(() => api.setActiveTab(last));
    }
  };
  const action = (kind: "install" | "update" | "reinstall" | "start" | "stop" | "open" | "remove" | "cancel", app: AppState) => {
    if (mockMode()) {
      if (kind === "install" || kind === "update" || kind === "reinstall") setState(s => ({ ...s, apps: s.apps.map(a => a.id === app.id ? { ...a, ...previewVariants.installing } : a) }));
      else if (kind === "start") setState(s => ({ ...s, apps: s.apps.map(a => a.id === app.id ? { ...a, phase: "running", url: "http://127.0.0.1:8888" } : a) }));
      else if (kind === "stop") setState(s => ({ ...s, apps: s.apps.map(a => a.id === app.id ? { ...a, phase: "installed", url: null } : a) }));
      else if (kind === "remove") setState(s => ({ ...s, apps: s.apps.map(a => a.id === app.id ? { ...a, installed: false, phase: "not_installed", version_installed: null, version_app: null } : a) }));
      return;
    }
    const id = app.id;
    void run(() => kind === "install" ? api.installApp(id, true) : kind === "update" ? api.updateApp(id, true) : kind === "reinstall" ? api.reinstallApp(id, true) : kind === "start" ? api.startApp(id) : kind === "stop" ? api.stopApp(id) : kind === "open" ? api.openApp(id) : kind === "remove" ? api.removeApp(id, "REMOVE") : api.cancelOperation(id));
  };
  const appBusy = (a: AppState) => working(a.phase) || state().operations.some(op => op.app_id === a.id);
  const changeRoot = async () => {
    try {
      const parent = mockMode() ? "D:\\Portable" : await api.pickDirectory("root_parent");
      if (!parent) return;
      const folder = state().settings.folder_name.trim() || "Autark";
      const settings = { ...state().settings, root: `${parent.replace(/[\\/]+$/, "")}\\${folder}` };
      if (mockMode()) setState(s => ({ ...s, settings }));
      else await run(() => api.saveSettings(settings));
    } catch (e) { setRequestError(errText(e)); }
  };
  const chooseRoot = async () => {
    try {
      const picked = mockMode() ? "D:\\Portable" : await api.pickDirectory("root_parent");
      if (!picked) return;
      const settings = { ...state().settings, root: picked };
      if (mockMode()) setState(s => ({ ...s, settings }));
      else await run(() => api.saveSettings({ ...state().settings, root: picked }, false));
    } catch (e) { setRequestError(errText(e)); }
  };
  const setGlobalPref = (key: "tray_enabled" | "start_with_windows" | "close_to_tray" | "experimental_local_apps" | "force_local_mise" | "animations", next: boolean) => {
    const settings = { ...state().settings, [key]: next };
    if (key === "experimental_local_apps" && !next) setMarketFilter("marketplace");
    if (mockMode()) setState(s => ({ ...s, settings }));
    else void run(() => api.saveSettings(settings, false));
  };
  const enableLocalApps = () => {
    setLocalAppsConfirm(false);
    const settings = { ...state().settings, experimental_local_apps: true };
    if (mockMode()) setState(s => ({ ...s, settings }));
    else void run(() => api.saveSettings(settings, false));
  };
  const addLocalApp = () => void run(async () => {
    const path = mockMode() ? "D:\\Manifests\\my-local-app\\app.json" : await api.pickFile("Choose an app manifest");
    if (!path) return;
    if (mockMode()) {
      const app = { ...previewState.apps[3], id: `local-app-${state().apps.length}`, name: "Local app" };
      setState(s => ({ ...s, apps: [...s.apps, app] }));
      return;
    }
    return api.addLocalApp(path);
  });
  const replaceLocalApp = (app: AppState) => void run(async () => {
    const path = mockMode() ? "D:\\Manifests\\replacement\\app.json" : await api.pickFile("Choose a replacement app manifest");
    if (!path) return;
    if (mockMode()) {
      setState(s => ({ ...s, apps: s.apps.map(a => a.id === app.id ? { ...a, name: "Replaced local app" } : a) }));
      return;
    }
    return api.replaceLocalApp(app.id, path);
  });
  const setAppPref = (app: AppState, key: keyof AppState["prefs"], next: boolean) => {
    const prefs = { ...app.prefs, [key]: next };
    if (mockMode()) setState(s => ({ ...s, apps: s.apps.map(a => a.id === app.id ? { ...a, prefs } : a) }));
    else void run(() => api.saveAppPrefs(app.id, prefs));
  };
  const saveDraft = (app: AppState) => {
    const settings = Object.fromEntries(Object.entries(drafts()[app.id] ?? app.settings).filter(([key]) => !app.settings_locked.some(lock => lock.key === key)));
    if (mockMode()) setState(s => ({ ...s, apps: s.apps.map(a => a.id === app.id ? { ...a, settings } : a) }));
    else void run(() => api.saveAppSettings(app.id, settings));
    setDrafts(d => { const next = { ...d }; delete next[app.id]; return next; });
  };
  const draftValue = (app: AppState, key: string) => (drafts()[app.id] ?? app.settings)[key];
  const setDraftValue = (app: AppState, key: string, value: unknown) => setDrafts(d => ({ ...d, [app.id]: { ...(d[app.id] ?? app.settings), [key]: value } }));
  const dirty = (app: AppState) => {
    const editable = (settings: Record<string, unknown>) => Object.fromEntries(Object.entries(settings).filter(([key]) => !app.settings_locked.some(lock => lock.key === key)));
    return JSON.stringify(editable(drafts()[app.id] ?? app.settings)) !== JSON.stringify(editable(app.settings));
  };
  const chooseFolder = async (app: AppState, field: AppSetting) => {
    try {
      const dir = mockMode() ? `D:\\Portable\\Autark\\apps\\${app.id}\\huggingface` : await api.pickDirectory("root_parent");
      if (dir) setDraftValue(app, field.key, asPortable(dir, `${state().settings.root}\\apps\\${app.id}`));
    }
    catch (e) { setRequestError(errText(e)); }
  };
  const openUrl = (url: string) => { if (!mockMode()) void api.openUrl(url).catch(e => setRequestError(errText(e))); };
  const reveal = (target: "root" | "marketplace" | "app" | "settings", id?: string) => { if (!mockMode()) void api.revealPath(target, id).catch(e => setRequestError(errText(e))); };
  const refresh = () => mockMode() ? undefined : void run(() => api.refreshMarketplace());
  const doUpdateAll = () => {
    if (mockMode()) return;
    setAllConfirm(false); void run(() => api.updateAll(true));
  };
  const windowAction = async (what: "minimize" | "maximize" | "close") => {
    if (!("__TAURI_INTERNALS__" in window)) return;
    const { getCurrentWindow } = await import("@tauri-apps/api/window");
    const w = getCurrentWindow();
    if (what === "minimize") await w.minimize(); else if (what === "maximize") await w.toggleMaximize(); else await w.close();
  };
  const chooserKeys = (e: KeyboardEvent) => {
    if (e.key === "Escape") { hideChooser(); setMiseScreen(false); }
    if (e.key === "ArrowDown") { e.preventDefault(); setSelected(i => Math.min(i + 1, available().length - 1)); }
    if (e.key === "ArrowUp") { e.preventDefault(); setSelected(i => Math.max(i - 1, 0)); }
    if (e.key === "Enter" && available()[selected()]) activateMarketplaceApp(available()[selected()]);
  };
  const activateMarketplaceApp = (a: AppState) => {
    if (a.installed) openApp(a.id);
    else installMarketplaceApp(a);
  };
  const installMarketplaceApp = (a: AppState) => {
    setChooser(false); setSettingsOpen(false);
    if (mockMode()) { openApp(a.id); action("install", a); return; }
    void run(async () => {
      const wasOpen = openIds().includes(a.id);
      const opened = wasOpen ? await api.setActiveTab(a.id) : await api.openAppTab(a.id);
      if (!wasOpen) setAutarkTab(false);
      setState(opened); setActive(a.id);
      return api.installApp(a.id, true);
    });
  };

  // One pane at a time: intro, then folder, then mise. A reopened mise screen wins.
  const pane = () => state().settings.wizard_complete || miseScreen() ? misePane() : wizardStep() === 1 ? introPane() : wizardStep() === 2 ? folderPane() : misePane();
  const introPane = () => (
    <>
      <div class="empty-mark" aria-hidden="true">autark</div>
      <h1>Apps installed properly on Windows</h1>
      <div class="intro-copy">
        <p>Autark is free, has no ads and collects no diagnostics or telemetry. Nothing leaves your machine unless you ask for it. The source is public under AGPL-3.0: <button class="inline-link" onClick={() => openUrl("https://github.com/verzly/autark")}>github.com/verzly/autark</button>.</p>
        <p>Built by <button class="inline-link" onClick={() => openUrl("https://github.com/rozsazoltan")}>rozsazoltan</button>.</p>
        <p>Apps with heavy setup steps are usually only practical with Docker on Windows. Autark installs them natively instead, so their dependencies never collide and nothing is scattered across random places on the machine.</p>
        <p>Every app lives in one folder, with its data and dependencies kept in order. Install, update, remove or run an app with one click, plus any extra command the app declares.</p>
        <p>Apps come from the marketplace. They are third-party and their install steps are checked before a recipe is published. Have an app idea or found a bug? Report it in <button class="inline-link" onClick={() => openUrl("https://github.com/verzly/autark/issues")}>autark issues</button> or in the <button class="inline-link" onClick={() => openUrl("https://github.com/verzly/autark-marketplace")}>marketplace</button>.</p>
      </div>
      <button class="button primary" onClick={() => setWizardStep(2)}>Next</button>
    </>
  );
  const folderPane = () => (
    <>
      <div class="empty-mark" aria-hidden="true">autark</div>
      <h1>Where should Autark keep its files?</h1>
          <p class="settings-muted">Settings, the marketplace clone, runtime tools and every app live in this folder.</p>
          <div class="path-row"><button class="path" onClick={() => void chooseRoot()}>{state().settings.root}</button><button class="icon-button" aria-label="Change Autark folder" title="Change Autark folder" onClick={() => void chooseRoot()}><Icon name="pencil" /></button></div>
          <button class="button primary" onClick={() => setWizardStep(3)}>Next</button>
    </>
  );
  const misePane = () => (
    <>
      <div class="empty-mark" aria-hidden="true">autark</div>
          <div><h1>{state().mise.available ? "Choose mise" : "mise is required"}</h1><p>Autark runs every app through <button class="inline-link" onClick={() => openUrl("https://github.com/jdx/mise")}>mise</button> and the <code>mise</code> command is not available on this machine.</p></div>
          <section><h2>I already have mise</h2><p>Point Autark at the existing <code>mise.exe</code>.</p><Show when={state().mise.detected.length}><div class="mise-detected" role="list" aria-label="Detected mise"><For each={state().mise.detected}>{entry => <button class={`mise-detected-row ${state().mise.path === entry.path ? "selected" : ""}`} role="listitem" title={entry.path} disabled={busy()} onClick={() => void run(() => pickMise(entry.path))}><span title={entry.path}>{entry.path}</span><small>{calver(entry.version)}</small></button>}</For></div></Show><button class="button" disabled={busy()} onClick={() => void run(async () => { const path = await api.pickFile("Choose mise.exe", "exe"); return path ? api.setMisePath(path) : undefined; })}>Choose another mise.exe</button></section>
          <section><h2>Install it for Autark</h2><p>Autark downloads mise into its own folder and keeps it there. Nothing is installed machine-wide.</p><footer><span>Local folder: {state().mise.local_path}{state().mise.force_local ? " · Always use Autark's own mise" : ""}</span><button class="icon-button" aria-label="Change Autark folder" title="Change Autark folder" onClick={() => void chooseRoot()}><Icon name="pencil" /></button></footer><Show when={localInstalled()}><div class={`mise-detected-row ${localSelected() ? "selected" : ""}`} role="listitem"><button class="mise-pick" title={state().mise.local_path} disabled={busy()} onClick={() => void run(() => pickMise(state().mise.local_path))}><span title={state().mise.local_path}>{state().mise.local_path}</span><small>{calver(state().mise.version)}</small></button><button class="icon-button" aria-label="Remove Autark's mise" title="Remove Autark's mise" disabled={busy()} onClick={() => void run(() => api.removeBinEntry("mise.exe"))}><Icon name="trash" /></button></div></Show><Show when={!localInstalled()}><Show when={downloading()}><div class="mise-bar" role="progressbar" aria-valuemin="0" aria-valuemax="100" aria-valuenow={misePercent() ?? 0}><span style={{ width: `${misePercent() ?? 0}%` }} /></div><small class="mise-progress">{misePercent() ?? 0}%</small></Show><button class="button primary" disabled={busy()} onClick={() => { setMiseInstalling(true); setBuffers(b => ({ ...b, "mise.exe": [] })); void run(() => api.installMise()); }}>Install</button></Show></section>
    </>
  );
  return <div class={`app ${state().settings.animations ? "" : "no-animation"}`}>
    <header class="chrome" data-tauri-drag-region>
      <Show when={state().mise.available && state().settings.wizard_complete && !miseScreen()}>
      <button class={`gear ${settingsOpen() ? "selected" : ""}`} aria-label="Settings" aria-pressed={settingsOpen()} onClick={() => {
        if (!settingsOpen()) {
          setSettingsReturn(active());
          setSettingsOpen(true); setSettingsTab("General"); setAppSettingsOpen(null); setActive(null); setChooser(false);
        }
      }} onDblClick={() => {
        if (settingsOpen()) {
          const target = settingsReturn() ?? AUTARK_TAB;
          setSettingsOpen(false); setActive(target); setChooser(false); setAppSettingsOpen(null);
        }
      }}><Icon name="menu" /></button>
      <button class={`add-tab marketplace-tab ${chooser() ? "selected" : ""}`} aria-label="Open marketplace" aria-pressed={chooser()} onClick={() => (chooser() ? hideChooser() : showChooser())}><Icon name="gridPlus" /></button>
      <div class="tabs" role="tablist" aria-label="Open apps">
        <For each={openIds()}>{id => {
          const app = () => state().apps.find(a => a.id === id);
          return <button class={`tab ${active() === id && !settingsOpen() ? "active" : ""}`} role="tab" aria-selected={active() === id && !settingsOpen()} onClick={() => selectTab(id)}>
            <span class="badge" style={{ "--hue": hue(id) }}><Show when={app()?.icon_data} fallback={id[0]?.toUpperCase()}>{src => <img src={src()} alt="" />}</Show></span><span class="tab-name">{app()?.name ?? id}</span>
            <span class={`dot ${app()?.phase ?? "not_installed"}`} />
            <span class="tab-close" role="button" aria-label={`Close ${app()?.name ?? id}`} onClick={e => closeTab(id, e)}><Icon name="close" /></span>
          </button>;
        }}</For>
        <For each={builtinTabs()}>{name => <button class={`tab ${active() === name && !settingsOpen() ? "active" : ""}`} role="tab" aria-selected={active() === name && !settingsOpen()} onClick={() => openBuiltin(name)}><span class="badge" style={{ "--hue": hue(name) }}><Show when={name.toLowerCase() === "mise.exe"} fallback={builtinName(name)[0]?.toUpperCase()}>{<img src={miseLogo} alt="" />}</Show></span><span class="tab-name">{builtinName(name)}</span><span class="tab-close" role="button" aria-label={`Close ${builtinName(name)}`} onClick={e => closeBuiltin(name, e)}><Icon name="close" /></span></button>}</For>
        <Show when={autarkTab()}><button class={`tab autark-tab ${active() === AUTARK_TAB && !settingsOpen() ? "active" : ""}`} role="tab" aria-selected={active() === AUTARK_TAB && !settingsOpen()} onClick={() => selectTab(AUTARK_TAB)}>
          <span class="autark-tab-mark" aria-hidden="true">a</span><span class="tab-name">Autark</span>
          <span class="tab-close" role="button" aria-label="Close Autark" onClick={e => closeAutarkTab(e)}><Icon name="close" /></span>
        </button></Show>
      </div>
      <Show when={!autarkTab()}><button class="add-tab" aria-label="New tab" onClick={() => { setAutarkTab(true); setActive(AUTARK_TAB); setChooser(false); setSettingsOpen(false); }}>+</button></Show>
      </Show>
      <div class="chrome-space" data-tauri-drag-region />
      <div class="window-controls">
        <button aria-label="Minimize" onClick={() => void windowAction("minimize")}><Icon name="min" /></button>
        <button aria-label="Maximize" onClick={() => void windowAction("maximize")}><Icon name="max" /></button>
        <button class="close-window" aria-label="Close" onClick={() => void windowAction("close")}><Icon name="close" /></button>
      </div>
    </header>
    <main class="card">
      <Show when={!state().settings.wizard_complete || !state().mise.available || miseScreen()} fallback={<>
      <Show when={settingsOpen()}><section class="settings-view">
        <button class="back-button" onClick={() => { const target = settingsReturn() ?? AUTARK_TAB; setSettingsOpen(false); setActive(target); setChooser(false); setAppSettingsOpen(null); }}>← back to homepage</button>
        <div class="view-heading"><div><span class="eyebrow">AUTARK</span><h1>Settings</h1></div></div>
        <nav class="settings-tabs" role="tablist" aria-label="Settings sections">
          <For each={["General", "Apps", "Display", "Experimental", "About"] as const}>{tab => <button class={`settings-tab ${settingsTab() === tab ? "active" : ""}`} role="tab" aria-selected={settingsTab() === tab} onClick={() => setSettingsTab(tab)}>{tab}</button>}</For>
        </nav>
        <div class="settings-content">
        <Show when={settingsTab() === "General"}>
        <section class="setting-row"><div><h2>Root folder</h2><p>Apps and managed tools live here.</p><button class="path" onClick={() => reveal("root")}>{state().settings.root}</button></div><button class="button" disabled={state().busy} onClick={() => void changeRoot()}>Change</button></section>
        <section class="setting-row"><div><h2>mise</h2><p>{state().mise.path ?? "not resolved"}{state().mise.version ? ` · ${state().mise.version}` : ""}</p></div><button class="button" disabled={state().busy} onClick={() => { setMiseScreen(true); setSettingsOpen(false); setActive(null); setChooser(false); }}>Change</button></section>
        <section class="preference-section"><h2>Reset Autark</h2><p>Deletes the marketplace clone, runtime tools and every installed app in the Autark folder, then starts the wizard again.</p>
          <Show when={!resetConfirm()}><button class="button" disabled={state().busy} onClick={() => { setResetText(""); setResetConfirm(true); }}>Reset Autark</button></Show>
          <Show when={resetConfirm()}><div class="inline-panel remove-panel"><p class="danger-note">This deletes every user setting, preference and file inside the Autark folder.</p><label>Type RESET<input value={resetText()} onInput={e => setResetText(e.currentTarget.value)} /></label><div><button class="button" onClick={() => setResetConfirm(false)}>Cancel</button><button class="button primary" disabled={state().busy || resetText() !== "RESET"} onClick={() => { setResetConfirm(false); setWizardStep(1); const after = () => { setMiseScreen(false); setSettingsOpen(false); setChooser(false); setAppSettingsOpen(null); setActive(null); setBuffers({}); }; if (mockMode()) { setState(s => ({ ...s, settings: { ...s.settings, wizard_complete: false } })); after(); } else { setResetting(true); void run(() => api.resetAutark("RESET")).then(after); } }}>Confirm</button></div></div></Show>
        </section>
         <section class="setting-row"><div><h2>Settings file</h2><p>Portable launcher preferences.</p><button class="path" onClick={() => reveal("settings")}>{state().settings_path}</button></div></section>
          <section class="preference-section window-preferences"><h2>Window</h2>
            <section class="setting-row"><label class="preference-toggle"><input type="checkbox" role="switch" checked={state().settings.tray_enabled} onChange={e => setGlobalPref("tray_enabled", e.currentTarget.checked)} /><span><b>Tray icon</b><small>Keep Autark in the notification area.</small></span></label></section>
            <section class="setting-row"><label class="preference-toggle"><input type="checkbox" role="switch" checked={state().settings.start_with_windows} disabled={!state().settings.tray_enabled} onChange={e => setGlobalPref("start_with_windows", e.currentTarget.checked)} /><span><b>Start with Windows</b><small>Starts hidden in the tray when you sign in.</small><Show when={!state().settings.tray_enabled}><small class="preference-note warning-note">Needs the tray icon.</small></Show></span></label></section>
            <section class="setting-row"><label class="preference-toggle"><input type="checkbox" role="switch" checked={state().settings.close_to_tray} disabled={!state().settings.tray_enabled} onChange={e => setGlobalPref("close_to_tray", e.currentTarget.checked)} /><span><b>Close to tray</b><small>The X button hides the window instead of quitting.</small><Show when={!state().settings.tray_enabled}><small class="preference-note warning-note">Needs the tray icon.</small></Show></span></label></section>
          </section>
        </Show>
        <Show when={settingsTab() === "Display"}>
          <section class="preference-section"><h2>Animations</h2><p>Turn the animated wordmark off to keep Autark as light as possible.</p>
            <div class="market-filters">
              <button class={`market-filter ${!state().settings.animations ? "active" : ""}`} aria-pressed={!state().settings.animations} onClick={() => setGlobalPref("animations", false)}>Performance</button>
              <button class={`market-filter ${state().settings.animations ? "active" : ""}`} aria-pressed={state().settings.animations} onClick={() => setGlobalPref("animations", true)}>Optimized</button>
            </div>
          </section>
        </Show>
        <Show when={settingsTab() === "Experimental"}>
           <section class="preference-section"><h2>Experimental</h2>
             <label class="preference-toggle"><input type="checkbox" role="switch" checked={state().settings.experimental_local_apps} onChange={e => e.currentTarget.checked ? setLocalAppsConfirm(true) : setGlobalPref("experimental_local_apps", false)} /><span><b>Local apps</b><small>Allow apps from manifests you supply.</small><small class="preference-note warning-note">Local app manifests can run commands on this machine.</small></span></label>
            <Show when={localAppsConfirm() && !state().settings.experimental_local_apps}><div class="inline-panel"><p>Local apps run setup commands from a file you supply. Only add manifests you trust.</p><div><button class="button" onClick={() => setLocalAppsConfirm(false)}>Cancel</button><button class="button primary" onClick={enableLocalApps}>Enable local apps</button></div></div></Show>
          </section>
          <section class="preference-section"><h2>Built-ins</h2>
                        <label class="preference-toggle"><input type="checkbox" role="switch" checked={state().settings.force_local_mise} onChange={e => setGlobalPref("force_local_mise", e.currentTarget.checked)} /><span><b>Always use Autark's own mise</b><small>Ignore the mise on this machine and require Autark's own copy.</small></span></label>
            <label class="preference-toggle"><input type="checkbox" role="switch" checked={state().mise.bin_in_path} onChange={e => void run(() => api.setBinPath(e.currentTarget.checked))} /><span><b>Add Autark's bin folder to PATH</b><small>{state().mise.bin_path}</small></span></label>
          </section>
          <p class="settings-muted">Experimental features may change or break.</p>
        </Show>
        <Show when={settingsTab() === "Apps"}>
        <section class="setting-row"><div><h2>Marketplace</h2><p>{state().marketplace.exists ? <button class="path" onClick={() => reveal("marketplace")}>{state().marketplace.path}</button> : "Marketplace has not been loaded."}</p><Show when={state().marketplace.last_refresh}><small>Last refreshed {new Date(state().marketplace.last_refresh! * 1000).toLocaleString()}</small></Show></div><button class="button" disabled={state().busy} onClick={refresh}>{state().marketplace.exists ? "Reload marketplace" : "Load marketplace"}</button></section>
        <section class="setting-row"><div><h2>Updates</h2><p>Check marketplace versions manually.</p></div><button class="button" disabled={state().busy} onClick={() => mockMode() ? undefined : void run(() => api.checkUpdates())}>Check for updates</button></section>
        <section class="updates"><h2>Update all</h2><p>Running apps stop during updates and cannot be used until each update finishes.</p>
          <div class="update-table">
            <For each={state().apps.filter(a => a.installed)}>{a => <article class="update-app"><h3>{a.name}</h3><div class="update-details"><span>App version <b>{a.version_app ?? "unknown"}</b></span><span>Installed <b>{a.version_installed ?? "not installed"}</b></span><span>Marketplace <b>{a.version_latest ?? "unknown"}</b></span><span>State <b>{a.phase}</b></span></div></article>}</For>
          </div>
          <Show when={!allConfirm()}><button class="button primary" disabled={state().busy || !state().apps.some(a => a.installed && a.update_available)} onClick={() => setAllConfirm(true)}>Update all</button></Show>
          <Show when={allConfirm()}><div class="confirm-strip"><span>Update every installed app now?</span><button class="button" onClick={() => setAllConfirm(false)}>Cancel</button><button class="button primary" disabled={state().busy} onClick={doUpdateAll}>Confirm</button></div></Show>
        </section>
        </Show>
        <Show when={settingsTab() === "About"}>
          <section class="settings-about">
            <p class="settings-about-description">Autark is a portable launcher for managing and running your apps. Every app comes from a marketplace manifest or a local file you add, and installs into its own folder with its own tools.</p>
            <div class="settings-about-row"><span>License</span><b>AGPL-3.0</b></div>
            <div class="settings-about-row"><span>Source</span><button onClick={() => openUrl("https://github.com/verzly/autark")}>github.com/verzly/autark</button></div>
            <div class="settings-about-row"><span>Marketplace</span><button onClick={() => openUrl("https://github.com/verzly/autark-marketplace")}>github.com/verzly/autark-marketplace</button></div>
          </section>
        </Show>
        </div>
        <footer class="about"><span>Autark <b>v{version()}</b></span><span>AGPL-3.0</span><button onClick={() => openUrl("https://github.com/verzly/autark")}>Source</button><button onClick={() => openUrl("https://github.com/verzly/autark-marketplace")}>Marketplace</button></footer>
      </section></Show>
      <Show when={chooser()}><section class="marketplace-view" onKeyDown={chooserKeys}>
         <div class="marketplace-heading"><button class="back-button" onClick={hideChooser}>← back to homepage</button><div><span class="eyebrow">MARKETPLACE</span><h1>Add an app</h1></div><div class="marketplace-tools"><input ref={dialogInput} class="search" aria-label="Search marketplace apps" placeholder="Search apps..." value={search()} onInput={e => { setSearch(e.currentTarget.value); setSelected(0); }} /><Show when={marketFilter() === "marketplace"}><button class="button" disabled={state().busy || state().marketplace.refreshing} onClick={refresh}>{state().marketplace.refreshing ? "Refreshing..." : "Reload marketplace"}</button></Show><Show when={marketFilter() === "local" && state().settings.experimental_local_apps}><button class="button" disabled={busy()} onClick={addLocalApp}>Add local app</button></Show><Show when={state().marketplace.last_refresh && marketFilter() === "marketplace"}><small>Last refreshed {new Date(state().marketplace.last_refresh! * 1000).toLocaleString()}</small></Show></div></div>
          <div class="market-filters" role="group" aria-label="App source"><button class={`market-filter ${marketFilter() === "builtin" ? "active" : ""}`} aria-pressed={marketFilter() === "builtin"} onClick={() => setMarketFilter("builtin")}>Built-in</button><button class={`market-filter ${marketFilter() === "marketplace" ? "active" : ""}`} aria-pressed={marketFilter() === "marketplace"} onClick={() => setMarketFilter("marketplace")}>Marketplace</button><Show when={state().settings.experimental_local_apps}><button class={`market-filter ${marketFilter() === "local" ? "active" : ""}`} aria-pressed={marketFilter() === "local"} onClick={() => setMarketFilter("local")}>Local</button></Show></div>

          <Show when={marketFilter() === "marketplace"}>
           <Show when={state().marketplace.exists} fallback={<div class="marketplace-load"><p>Marketplace has not been loaded.</p><button class="button primary" disabled={state().marketplace.refreshing} onClick={refresh}>Load marketplace</button></div>}>
             <Show when={!state().marketplace.refreshing} fallback={<div class="marketplace-load">Refreshing marketplace...</div>}>
               <Show when={marketplaceApps().length} fallback={<div class="marketplace-load">No apps match your search.</div>}>
                 <div class="marketplace-grid" role="list" aria-label="Marketplace apps"><For each={marketplaceApps()}>{(a, i) => {
                   const opened = () => openIds().includes(a.id);
                   return <article role="listitem" class={`market-card ${opened() ? "market-opened" : ""} ${selected() === i() ? "market-selected" : ""}`} onMouseEnter={() => setSelected(i())}>
                      <div class="market-card-head"><span class="badge large" style={{ "--hue": hue(a.id) }}><Show when={a.icon_data} fallback={a.id[0]?.toUpperCase()}>{src => <img src={src()} alt="" />}</Show></span><div><h2>{a.name}</h2><code>{a.id}</code></div><Show when={!a.local && a.update_available}><span class="update-pill">Update</span></Show></div>
                      <p class="market-description">{a.description}</p><div class="market-versions"><span>Installed <b>{a.version_installed ?? "not installed"}</b></span><span>Marketplace <b>{a.local ? "local" : a.version_latest ?? "unknown"}</b></span></div>
                     <div class="market-requirements"><Show when={a.requirements.disk_gb != null}><span>{a.requirements.disk_gb} GB disk</span></Show><Show when={a.requirements.webview2}><span>WebView2</span></Show></div>
                     <div class="market-links"><Show when={a.license}><span>{a.license}</span></Show><Show when={a.homepage}><button onClick={() => openUrl(a.homepage!)}>Homepage ↗</button></Show></div>
                      <Show when={a.installed} fallback={<div class="market-actions"><button class="button market-view-action" disabled={working(a.phase)} onClick={() => openApp(a.id)}>View</button><button class="button primary market-action" disabled={working(a.phase)} onClick={() => installMarketplaceApp(a)}>Install</button></div>}>
                        <div class="market-actions"><Show when={a.local}><button class="button market-view-action" disabled={busy()} onClick={() => replaceLocalApp(a)}>Edit</button></Show><button class={`button market-action ${opened() ? "opened-action" : "primary"}`} disabled={!opened() && working(a.phase)} onClick={() => opened() ? openApp(a.id) : activateMarketplaceApp(a)}>{opened() ? "Opened" : "Open"}</button></div>
                     </Show>
                   </article>;
                 }}</For></div>
               </Show>
             </Show>
           </Show>
         </Show>
          <Show when={marketFilter() === "local"}>
            <Show when={marketplaceApps().length} fallback={<div class="marketplace-load">No local apps yet.</div>}>
             <div class="marketplace-grid" role="list" aria-label="Local apps"><For each={marketplaceApps()}>{(a, i) => {
               const opened = () => openIds().includes(a.id);
               return <article role="listitem" class={`market-card ${opened() ? "market-opened" : ""} ${selected() === i() ? "market-selected" : ""}`} onMouseEnter={() => setSelected(i())}>
                  <div class="market-card-head"><span class="badge large" style={{ "--hue": hue(a.id) }}><Show when={a.icon_data} fallback={a.id[0]?.toUpperCase()}>{src => <img src={src()} alt="" />}</Show></span><div><h2>{a.name}</h2><code>{a.id}</code></div><span class="local-tag">local</span></div>
                  <p class="market-description">{a.description}</p><div class="market-versions"><span>Installed <b>{a.version_installed ?? "not installed"}</b></span><span>Marketplace <b>local</b></span></div>
                 <div class="market-links"><Show when={a.license}><span>{a.license}</span></Show><Show when={a.homepage}><button onClick={() => openUrl(a.homepage!)}>Homepage ↗</button></Show></div>
                 <div class="market-actions"><button class="button market-view-action" disabled={working(a.phase)} onClick={() => replaceLocalApp(a)}>Edit</button>
                   <Show when={a.installed} fallback={<button class="button primary market-action" disabled={working(a.phase)} onClick={() => installMarketplaceApp(a)}>Install</button>}>
                     <button class={`button market-action ${opened() ? "opened-action" : "primary"}`} disabled={!opened() && working(a.phase)} onClick={() => opened() ? openApp(a.id) : activateMarketplaceApp(a)}>{opened() ? "Opened" : "Open"}</button>
                   </Show>
                 </div>
               </article>;
             }}</For></div>
           </Show>
          </Show>
          <Show when={marketFilter() === "builtin"}>
            <section class="builtin-content">
              <Show when={builtinApps().length} fallback={<p class="marketplace-load">No built-in apps yet. Autark's own tools appear here.</p>}>
                <div class="marketplace-grid" role="list" aria-label="Built-in apps"><For each={builtinApps()}>{(entry, i) => {
                  const title = entry.name.replace(/\.exe$/i, "");
                  const systemMise = isResolvedMise(entry.name);
                  return <article role="listitem" class={`market-card ${selected() === i() ? "market-selected" : ""}`} onMouseEnter={() => setSelected(i())}>
                    <div class="market-card-head"><span class="badge large" style={{ "--hue": hue(entry.name) }}><Show when={entry.name.toLowerCase() === "mise.exe"} fallback={title[0]?.toUpperCase()}>{<img src={miseLogo} alt="" />}</Show></span><div><h2>{title}</h2><code>{entry.name}</code></div><span class="local-tag">{systemMise ? (entry.path ? "By your machine" : "Not installed") : "By Autark"}</span></div>
                    <p class="market-description">{systemMise ? "Autark runs every app through this mise." : "Managed in Autark's own bin folder."}</p>
                    <div class="market-versions"><span>Version <b>{entry.version ?? "unknown"}</b></span><span>Size <b>{entry.size ? `${(entry.size / 1048576).toFixed(1)} MB` : state().mise.source}</b></span></div>
                    <div class="market-links"><span>{systemMise ? (entry.path || "not detected") : state().mise.bin_path}</span></div>
                    <div class="market-actions"><Show when={entry.name.toLowerCase() === "mise.exe" && !systemMise}><button class="button primary market-action" disabled={busy()} onClick={() => { openBuiltin(entry.name); void run(() => api.updateMise()); }}>Update</button></Show>
                      <Show when={!systemMise}>
                        <Show when={binRemoveConfirm() !== entry.name}><button class="button market-view-action" onClick={() => setBinRemoveConfirm(entry.name)}>Remove</button></Show>
                        <Show when={binRemoveConfirm() === entry.name}><div class="confirm-strip"><span>Remove {entry.name} from Autark's bin folder?</span><button class="button" onClick={() => setBinRemoveConfirm(null)}>Cancel</button><button class="button" disabled={busy()} onClick={() => { setBinRemoveConfirm(null); void run(() => api.removeBinEntry(entry.name)); }}>Remove</button></div></Show>
                      </Show>
                      <Show when={systemMise}><p class="market-unavailable">Not available: Autark runs the mise selected in Settings.</p></Show>
                    </div>
                  </article>;
                }}</For></div>
              </Show>
            </section>
          </Show>
       </section></Show>
      <Show when={!chooser() && !settingsOpen() && isBuiltin(active())}><section class="app-view">
        <div class="app-header"><div class="app-title"><span class="badge large"><Show when={active()?.toLowerCase() === "mise.exe"} fallback={builtinName(active())[0]?.toUpperCase()}>{<img src={miseLogo} alt="" />}</Show></span><div><h1>{builtinName(active())}</h1><div class="version-line">version {state().mise.version ?? "unknown"} · {state().mise.source === "local" ? "By Autark" : "By your machine"}</div></div></div></div>
        <div class="actions"><button class="button" onClick={() => { setMarketFilter("builtin"); showChooser(); }}>Back to built-in apps</button><Show when={localInstalled()}><button class="button primary" disabled={busy()} onClick={() => void run(() => api.updateMise())}>Update</button></Show><button class="button" disabled={busy()} onClick={() => setMiseScreen(true)}>Change mise.exe</button><Show when={localInstalled()}><button class="button danger" disabled={busy()} onClick={() => void run(() => api.removeBinEntry("mise.exe"))}>Remove</button></Show></div>
        <div class="output-wrap"><div class="output" ref={output} onScroll={() => output && setFollow(output.scrollHeight - output.scrollTop - output.clientHeight < 24)}>
          <For each={logEntries()}>{entry => entry.spacer ? <div class="log-spacer" aria-hidden="true" /> : <div class={`log ${lineTone(entry.line)}`}>{entry.line}</div>}</For>
        </div></div>
        <footer class="metadata"><span>{state().mise.path ?? "not resolved"}</span><code>{builtinName(active())}</code></footer>
      </section></Show>
      <Show when={!chooser() && !settingsOpen() && active() === AUTARK_TAB}><section class="empty-state"><div class="empty-mark">autark</div>
        <div class="landing-app-list" role="list" aria-label="Installed apps">
          <Show when={state().apps.some(a => a.installed)} fallback={<p class="landing-empty" role="listitem">No apps are installed yet.</p>}>
            <For each={state().apps.filter(a => a.installed)}>{a => <button class="landing-app-row" role="listitem" onClick={() => openApp(a.id)}>
              <span class="badge" style={{ "--hue": hue(a.id) }}><Show when={a.icon_data} fallback={a.id[0]?.toUpperCase()}>{src => <img src={src()} alt="" />}</Show></span><span>{a.name}</span><Show when={a.installed}><small>Installed</small></Show>
            </button>}</For>
          </Show>
          <button class="landing-app-row landing-marketplace-action" role="listitem" onClick={showChooser}>Go to marketplace</button>
        </div>
      </section></Show>
      <Show when={!chooser() && !settingsOpen() && currentApp()}>{app => {
        const a = () => app();
        const p = () => a().phase;
        const busyApp = () => appBusy(a());
        const elapsed = () => a().elapsed_secs ?? (a().started_at ? Math.max(0, clock() - a().started_at!) : null);
        const canStart = () => !busyApp() && a().installed && a().can_start && p() === "installed";
        const canStop = () => !busyApp() && a().can_start && (p() === "running" || p() === "starting");
        const confirmInstall = () => { setPanel(null); action(a().installed ? "update" : "install", a()); };
        return <Show when={appSettingsOpen() === a().id} fallback={<section class="app-view">
          <div class="app-header"><div class="app-title"><span class="badge large" style={{ "--hue": hue(a().id) }}><Show when={a().icon_data} fallback={a().id[0]?.toUpperCase()}>{src => <img src={src()} alt="" />}</Show></span><div><h1>{a().name}</h1><div class="version-line">{a().version_app ? `app ${a().version_app}` : "app version unknown"} · {a().local ? "local" : `recipe ${a().version_installed ?? "not installed"}`}<Show when={!a().local && a().update_available}> · latest recipe {a().version_latest ?? "unknown"} <span class="update-pill">update</span></Show></div></div></div>
            <div class="links"><Show when={a().homepage}><button onClick={() => openUrl(a().homepage!)}>Homepage ↗</button></Show><Show when={a().license}><span>{a().license}</span></Show></div></div>
          <div class="status-line"><span class={`dot ${p()}`} /><Show when={working(p())}><span class="progress"><i /></span></Show><span class="phase-label">{a().step ? `(${a().step?.index}/${a().step?.total}) ${a().step?.name}` : p().replace("_", " ")}</span><Show when={working(p()) && elapsed() != null}><span class="elapsed">run {duration(elapsed()!)}</span></Show></div>
          <div class="actions">
             <Show when={!a().local}><button class="button primary" disabled={busyApp() || (a().installed && !a().update_available)} onClick={() => setPanel("install")}>{a().installed ? "Update" : "Install"}</button></Show>
            <Show when={a().can_start}>
              <button class="button" disabled={!canStart()} onClick={() => action("start", a())}>Start</button>
              <button class="button" disabled={!canStop()} onClick={() => action("stop", a())}>Stop</button>
            </Show>
            <button class="button" disabled={busyApp() || !a().url} onClick={() => action("open", a())}>Open</button>
             <button class="button" aria-label={`${a().name} settings`} onClick={() => { setPanel(null); setAppSettingsOpen(a().id); }}><Icon name="menu" /> Settings</button>
             <For each={a().actions}>{item => <button class="button" disabled={busyApp() || !a().installed} onClick={() => item.confirm ? setPanel(`action:${item.id}`) : void run(() => api.runAction(a().id, item.id))}>{item.label}</button>}</For>
             <Show when={working(p())}><button class="button" onClick={() => action("cancel", a())}>Cancel operation</button></Show>
           </div>
           <Show when={panel() === "install"}><div class="inline-panel"><p>Setup may install system components through mise and requires network access.</p><div><button class="button" onClick={() => setPanel(null)}>Cancel</button><button class="button primary" disabled={busyApp()} onClick={confirmInstall}>Confirm</button></div></div></Show>
           <For each={a().actions.filter(item => item.confirm && panel() === `action:${item.id}`)}>{item => <div class="inline-panel"><p>Run {item.label}?</p><div><button class="button" onClick={() => setPanel(null)}>Cancel</button><button class="button primary" disabled={busyApp() || !a().installed} onClick={() => { setPanel(null); void run(() => api.runAction(a().id, item.id)); }}>Confirm</button></div></div>}</For>
          <section class="output-section"><div class="output-head"><h2>Output</h2><button onClick={() => { setBuffers(b => ({ ...b, [a().id]: [] })); setFollow(true); }}>Clear</button></div>
            <div class="output-wrap"><div class="output" ref={output} onScroll={() => output && setFollow(output.scrollHeight - output.scrollTop - output.clientHeight < 24)}>
              <Show when={logs().length} fallback={<span class="quiet">Operation output will appear here.</span>}><For each={logEntries()}>{entry => <div class={entry.spacer ? "log-spacer" : `log ${lineTone(entry.line)}`} aria-hidden={entry.spacer || undefined}>{entry.spacer ? "\u00a0" : entry.line || "\u00a0"}</div>}</For></Show>
            </div><Show when={!follow() && logs().length}><button class="jump" onClick={() => { if (output) output.scrollTop = output.scrollHeight; setFollow(true); }}>Jump to latest</button></Show></div>
          </section>
          <footer class="metadata"><button title={state().settings.root + "\\apps\\" + a().id} onClick={() => reveal("app", a().id)}>{`${state().settings.root}\\apps\\${a().id}`}</button><span>{a().version_app ? `app ${a().version_app}` : "app version unknown"} {a().update_available ? `latest ${a().version_latest ?? "unknown"}` : "latest"}<button class="check-button" title="Check the marketplace for a newer version" onClick={() => void run(() => api.checkUpdates())}>(check)</button></span><span>{a().local ? "local" : `recipe ${a().version_installed ?? "not installed"} ${a().update_available ? `latest ${a().version_latest ?? "unknown"}` : "latest"}`}{a().local ? null : <button class="check-button" title="Check the marketplace for a newer version" onClick={() => void run(() => api.checkUpdates())}>(check)</button>}</span><code>{a().id}</code></footer>
        </section>}>
          <section class="app-settings-page">
            <button class="back-button" onClick={() => setAppSettingsOpen(null)}>← back to app</button>
            <div class="view-heading"><div><span class="eyebrow">{a().name.toUpperCase()}</span><h1>Settings</h1></div><Show when={dirty(a())}><button class="button primary" disabled={busyApp()} onClick={() => saveDraft(a())}>Save</button></Show></div>
            <Show when={a().settings_schema.length} fallback={<p class="quiet">No app settings.</p>}>
              <div class="fields"><For each={a().settings_schema}>{field => <SettingField app={a()} field={field} value={draftValue(a(), field.key)} lock={a().settings_locked.find(lock => lock.key === field.key)} setValue={value => setDraftValue(a(), field.key, value)} choose={() => void chooseFolder(a(), field)} />}</For></div>
            </Show>
            <section class="preference-section"><h2>Shortcut</h2><p>A desktop shortcut that opens this app in Autark.</p>
              <button class="button" disabled={busyApp() || !a().installed || shortcutBusy().includes(a().id)} onClick={() => { if (shortcutBusy().includes(a().id)) return; if (mockMode()) { setShortcutDone(ids => [...ids, a().id]); return; } setShortcutBusy(ids => [...ids, a().id]); void run(async () => { try { const next = await api.createShortcut(a().id); setShortcutDone(ids => ids.includes(a().id) ? ids : [...ids, a().id]); return next; } finally { setShortcutBusy(ids => ids.filter(x => x !== a().id)); } }); }}>Create shortcut</button>
              <Show when={shortcutDone().includes(a().id)}><small class="preference-note">Shortcut created on the desktop. The start options apply every time you open it.</small></Show>
              <label class="preference-toggle"><input type="checkbox" role="switch" checked={a().prefs.shortcut_autostart} onChange={e => setAppPref(a(), "shortcut_autostart", e.currentTarget.checked)} /><span><b>Also start the app</b></span></label>
              <label class="preference-toggle"><input type="checkbox" role="switch" checked={a().prefs.shortcut_background} onChange={e => setAppPref(a(), "shortcut_background", e.currentTarget.checked)} /><span><b>Run hidden in the background</b></span></label>
            </section>
            <section class="preference-section"><h2>Startup</h2><label class="preference-toggle"><input type="checkbox" role="switch" checked={a().prefs.auto_start} onChange={e => setAppPref(a(), "auto_start", e.currentTarget.checked)} /><span><b>Start when Autark starts</b></span></label></section>
            <section class="uninstall-section"><h2>Reinstall app</h2><p>Runs the full setup again with the current recipe. Settings and user data are preserved.</p>
              <Show when={panel() !== "reinstall"}><button class="button" disabled={busyApp() || !a().installed} onClick={() => setPanel("reinstall")}>Reinstall</button></Show>
              <Show when={panel() === "reinstall"}><div class="inline-panel"><div><button class="button" onClick={() => setPanel(null)}>Cancel</button><button class="button primary" disabled={busyApp()} onClick={() => { setPanel(null); action("reinstall", a()); }}>Reinstall</button></div></div></Show>
            </section>
            <section class="uninstall-section"><h2>Uninstall app</h2><p>Remove this app from the launcher. User data and settings are preserved.</p>
              <Show when={panel() !== "remove"}><button class="button danger-quiet" disabled={busyApp() || !a().installed} onClick={() => { setRemoveText(""); setPanel("remove"); }}>Uninstall</button></Show>
              <Show when={panel() === "remove"}><div class="inline-panel remove-panel"><label>Type REMOVE<input value={removeText()} onInput={e => setRemoveText(e.currentTarget.value)} /></label><div><button class="button" onClick={() => setPanel(null)}>Cancel</button><button class="button danger" disabled={busyApp() || removeText() !== "REMOVE"} onClick={() => { setPanel(null); action("remove", a()); }}>Remove</button></div></div></Show>
            </section>
          </section>
        </Show>;
      }}</Show>
      </>}>
      <section class="mise-onboarding">{pane()}</section>
      </Show>
      <Show when={miseInstalling() || misePicking() || resetting()}>
        <div class="mise-loading" role="status" aria-live="polite">
          <div class="empty-mark" aria-hidden="true">autark</div>
          <small class="mise-progress">{resetting() ? "Resetting Autark" : miseInstalling() ? "Installing Autark's mise" : `Using ${misePickTarget() ?? "the selected mise.exe"}`}</small>
          <div class="mise-bar" role="progressbar" aria-valuemin="0" aria-valuemax="100" aria-valuenow={loadingPercent()}><span style={{ width: `${loadingPercent()}%` }} /></div>
          <small class="settings-muted">{loadingPercent()}%</small>
        </div>
      </Show>
    </main>
      <Show when={requestError() || currentApp()?.last_error || state().marketplace.error}>
      <div class="toasts">
        <Show when={requestError()}><div class="toast error-banner" role="alert">{requestError()}</div></Show>
        <Show when={currentApp()?.last_error}>{err => <div class="toast error-banner" role="alert">{err().message}</div>}</Show>
        <Show when={state().marketplace.error}>{err => <div class="toast error-banner" role="alert">{err().message}</div>}</Show>
      </div>
    </Show>
  </div>;
}

function SettingField(props: { app: AppState; field: AppSetting; value: unknown; lock?: AppState["settings_locked"][number]; setValue: (v: unknown) => void; choose: () => void }) {
  const value = () => props.lock ? props.lock.value : String(props.value ?? props.field.default ?? "");
  return <label class="field"><span>{props.field.label}</span><Show when={props.field.kind === "boolean"} fallback={
    <div class="field-control"><input classList={{ locked: Boolean(props.lock) }} disabled={Boolean(props.lock)} type={props.field.kind === "number" || props.field.kind === "port" ? "number" : "text"} value={value()} onInput={e => props.setValue(props.field.kind === "number" || props.field.kind === "port" ? Number(e.currentTarget.value) : e.currentTarget.value)} /><Show when={props.field.kind === "path"}><button class="button" disabled={Boolean(props.lock)} onClick={props.choose}>Choose</button></Show></div>
  }><input type="checkbox" role="switch" disabled={Boolean(props.lock)} checked={props.lock ? props.lock.value.toLowerCase() === "true" : Boolean(props.value ?? props.field.default)} onChange={e => props.setValue(e.currentTarget.checked)} /></Show><Show when={props.field.kind === "path"}><small>{"{app}"} is this app's folder, for example {"{app}/huggingface"}. Paths inside it are stored that way.</small></Show><Show when={props.lock}><small class="setting-lock-note">Set by the system environment ({props.lock?.var}). Autark cannot change it.</small></Show><Show when={props.field.help}><small>{props.field.help}</small></Show></label>;
}
/// Keeps a picked folder portable: anything inside the app folder becomes `{app}/...`.
function asPortable(picked: string, appDir: string) {
  const toSlash = (value: string) => value.replace(/[\\/]+/g, "/").replace(/\/+$/, "");
  const dir = toSlash(appDir).toLowerCase();
  const full = toSlash(picked);
  const lower = full.toLowerCase();
  if (lower === dir) return "{app}";
  if (lower.startsWith(`${dir}/`)) return `{app}${full.slice(dir.length)}`;
  return full;
}
function lineTone(line: string) {
  return /error|failed|fatal|stopped/i.test(line) ? "tone-error" : /warning|warn/i.test(line) ? "tone-warning" : /installed|complete|ready/i.test(line) ? "tone-success" : /downloading|extracting|^\s*\d+%/i.test(line) ? "tone-dim" : "";
}
function Icon(props: { name: "menu" | "gridPlus" | "close" | "min" | "max" | "pencil" | "trash" }) {
  return <svg viewBox="0 0 16 16" aria-hidden="true">{props.name === "menu" ? <path d="M2 4h12M2 8h12M2 12h12" /> : props.name === "trash" ? <><path d="M2.6 4.4h10.8" /><path d="M6.2 4.4V2.9h3.6v1.5" /><path d="M4.1 4.4l.7 8.1a1.1 1.1 0 0 0 1.1 1h4.2a1.1 1.1 0 0 0 1.1-1l.7-8.1" /><path d="M6.7 7v4M9.3 7v4" /></> : props.name === "gridPlus" ? <><rect x="1.5" y="1.5" width="5" height="5" rx="1" /><rect x="9.5" y="1.5" width="5" height="5" rx="1" /><rect x="1.5" y="9.5" width="5" height="5" rx="1" /><path d="M12 9v5m-2.5-2.5h5" /></> : props.name === "close" ? <path d="m4 4 8 8m0-8-8 8" /> : props.name === "min" ? <path d="M3 8h10" /> : props.name === "max" ? <rect x="3.2" y="3.2" width="9.6" height="9.6" rx="1" /> : <><path d="M10.4 3.1 13 5.7" /><path d="M2.4 13.6 3.3 10 10.6 2.7a1.3 1.3 0 0 1 1.8 0l.9.9a1.3 1.3 0 0 1 0 1.8L6 12.8l-3.6.8Z" /></>}</svg>;
}
