import { createEffect, createMemo, createSignal, For, onCleanup, onMount, Show } from "solid-js";
import type { AppSetting, AppState, Phase, State } from "./bridge";
import { api, mockMode } from "./bridge";

const [upstreamVersions, setUpstreamVersions] = createSignal<Record<string, string | null>>({});
const upstreamInFlight = new Set<string>();
function compareVersions(left: string, right: string) {
  const parts = (value: string) => value.replace(/^[vV]/, "").replace(/^[^\d]*/, "").split(/[.+-]/).map(part => /^\d+$/.test(part) ? Number(part) : part.toLowerCase());
  const a = parts(left), b = parts(right);
  for (let i = 0; i < Math.max(a.length, b.length); i++) {
    const x = a[i] ?? 0, y = b[i] ?? 0;
    if (x === y) continue;
    if (typeof x === "number" && typeof y === "number") return x > y ? 1 : -1;
    return String(x).localeCompare(String(y));
  }
  return 0;
}
async function fetchUpstream(upstream: string): Promise<string | null> {
  if (upstream.startsWith("npm:")) {
    // Only the package path is encoded; the registry expects the slash encoded.
    const pkg = upstream.slice(4).replace("/", "%2F");
    try {
      const response = await fetch(`https://registry.npmjs.org/${pkg}/latest`);
      if (!response.ok) return null;
      const version = (await response.json()).version;
      return typeof version === "string" ? version : null;
    } catch { return null; }
  }
  const [owner, repo] = upstream.split("/");
  if (!owner || !repo) return null;
  for (const endpoint of [`releases/latest`, `releases?per_page=1`, `tags?per_page=1`]) {
    try {
      const response = await fetch(`https://api.github.com/repos/${owner}/${repo}/${endpoint}`);
      if (!response.ok) continue;
      const json = await response.json();
      const tag = Array.isArray(json) ? json[0]?.tag_name ?? json[0]?.name : json.tag_name ?? json.name;
      if (typeof tag === "string") return tag;
    } catch { /* Try the next GitHub endpoint. */ }
  }
  return null;
}
function upstreamNewer(app: AppState) {
  if (!app.upstream || !app.version_app) return false;
  if (!(app.id in upstreamVersions()) && !upstreamInFlight.has(app.id)) {
    upstreamInFlight.add(app.id);
    void fetchUpstream(app.upstream).then(version => setUpstreamVersions(versions => ({ ...versions, [app.id]: version }))).finally(() => upstreamInFlight.delete(app.id));
  }
  const latest = upstreamVersions()[app.id];
  // version_check may append detail after " | "; compare the version alone.
  const installed = app.version_app.split(/\s*\|\s*/)[0];
  return Boolean(latest && compareVersions(latest, installed) > 0);
}
import { previewLogs, previewState, previewVariants } from "./mock";
import miseLogo from "./mise.png";
import gitLogo from "./git.svg";
import pg0Logo from "./pg0.svg";

type InlineAction = "install" | "restore" | "reinstall" | "remove" | `action:${string}` | null;
const working = (p: Phase) => ["installing", "starting", "stopping", "updating", "removing"].includes(p);
const errText = (e: unknown) => {
  if (e instanceof Error) return e.message;
  if (e && typeof e === "object" && "message" in e) return String((e as { message: unknown }).message);
  return String(e);
};
const sentence = (text: string) => text ? text[0].toUpperCase() + text.slice(1) : text;
const hue = (id: string) => { let h = 0; for (const c of id) h = (h * 31 + c.charCodeAt(0)) | 0; return Math.abs(h) % 360; };
const duration = (s: number) => `${Math.floor(s / 60)}m ${String(Math.floor(s % 60)).padStart(2, "0")}s`;
const calver = (version: string | null) => version ? version.match(/^\d+\.\d+\.\d+/)?.[0] ?? version : "version unknown";
const AUTARK_TAB = "__autark__";
// Built-in tools with a real logo; everything else falls back to its first letter.
const builtinLogo = (name: string) => {
  const lower = name.toLowerCase();
  return lower === "mise.exe" ? miseLogo : lower === "git.exe" ? gitLogo : lower === "pg0.exe" ? pg0Logo : null;
};

export default function App() {
  const [state, setState] = createSignal<State>(previewState);
  const [active, setActive] = createSignal<string | null>(previewState.settings.active_tab ?? AUTARK_TAB);
  const [settingsOpen, setSettingsOpen] = createSignal(false);
  const [settingsTab, setSettingsTab] = createSignal<"General" | "Apps" | "Preview" | "Display" | "Experimental" | "About">("General");
  const [settingsReturn, setSettingsReturn] = createSignal<string | null>(null);
  const [appSettingsOpen, setAppSettingsOpen] = createSignal<string | null>(null);
  const [chooser, setChooser] = createSignal(false);
  const [chooserReturn, setChooserReturn] = createSignal<string | null>(null);
  const [search, setSearch] = createSignal("");
  const [selected, setSelected] = createSignal(0);
  const [panel, setPanel] = createSignal<InlineAction>(null);
  const [removeText, setRemoveText] = createSignal("");
  const [requestError, setRequestError] = createSignal("")
  const [terminalNotice, setTerminalNotice] = createSignal("")
  // The shell picker hangs off the terminal button, opened with a right click.
  const [terminalMenu, setTerminalMenu] = createSignal<{ x: number; y: number } | null>(null);
  const [forceTerminalConfirm, setForceTerminalConfirm] = createSignal(false);
  const [busy, setBusy] = createSignal(false);
  const [buffers, setBuffers] = createSignal<Record<string, string[]>>(mockMode() ? { "unsloth-studio": previewLogs } : {});
  const [follow, setFollow] = createSignal(true);
  const [drafts, setDrafts] = createSignal<Record<string, Record<string, unknown>>>({});
  const [root, setRoot] = createSignal("");
  const [allConfirm, setAllConfirm] = createSignal(false);
  const [appTab, setAppTab] = createSignal<"output" | "faq">("output");
  const [sourceDrafts, setSourceDrafts] = createSignal<Record<string, string>>({});
  const [appSettingsTab, setAppSettingsTab] = createSignal("General");
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
    if (!hasLocalMise) entries.unshift({ name: "mise.exe", path: "", version: null, size: 0 });
    // git lives in Autark's mise installs, not in bin, so its card is added here.
    if (!entries.some(entry => entry.name.toLowerCase() === "git.exe")) entries.push({ name: "git.exe", path: state().git.path ?? "", version: state().git.version, size: 0 });
    // pg0 ships inside an app Python environment until it is copied into Autark bin.
    if (!entries.some(entry => entry.name.toLowerCase() === "pg0.exe")) entries.push({ name: "pg0.exe", path: state().pg0.path ?? "", version: state().pg0.version, size: 0 });
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
    api.onLog(({ app_id, line }) => setBuffers(prev => {
      const next = [...(prev[app_id] ?? []), line].slice(-500);
      // Only an environment-shaped failure gets the reinstall hint; a real app error does not.
      if (/^app exited \(exit code: [1-9]/.test(line) && /(ModuleNotFoundError|No module named|ImportError|FileNotFoundError|WinError 2\b|not recognized as an internal or external command|Access is denied|Permission denied)/i.test(next.join("\n"))) {
        next.push("Tip: a broken install can cause this. Reinstall in the app settings rebuilds it from scratch.");
      }
      return { ...prev, [app_id]: next };
    })).then(f => offLog = f);
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
      if (next.mise.available && !next.settings.wizard_complete) setWizardStep(4);
      return next;
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
    try { const next = await fn(); if (next) { const current = active(); setState(next); if (!isBuiltin(current) && current !== AUTARK_TAB) setActive(next.settings.active_tab ?? (current === null ? null : AUTARK_TAB)); } return true; }
    catch (e) { setRequestError(errText(e)); return false; }
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
    // Focus the new tab before the request, otherwise the reply puts the new-tab view back.
    setActive(id);
    if (mockMode()) { setState(s => ({ ...s, settings: { ...s.settings, open_tabs: [...s.settings.open_tabs, id], active_tab: id } })); }
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
  // Never leave the window empty: a closed or unknown tab opens the Autark tab again.
  createEffect(() => {
    if (!state().settings.wizard_complete || !state().mise.available || miseScreen() || settingsOpen() || chooser()) return;
    const id = active();
    if (id === AUTARK_TAB || isBuiltin(id) || currentApp()) return;
    setAutarkTab(true);
    setActive(AUTARK_TAB);
  });
  const action = (kind: "install" | "update" | "reinstall" | "start" | "stop" | "open" | "remove" | "cancel", app: AppState) => {
    if (mockMode()) {
      if (kind === "install" || kind === "update" || kind === "reinstall") setState(s => ({ ...s, apps: s.apps.map(a => a.id === app.id ? { ...a, ...previewVariants.installing } : a) }));
      else if (kind === "start") setState(s => ({ ...s, apps: s.apps.map(a => a.id === app.id ? { ...a, phase: "running", url: "http://127.0.0.1:8888" } : a) }));
      else if (kind === "stop") setState(s => ({ ...s, apps: s.apps.map(a => a.id === app.id ? { ...a, phase: "installed", url: null } : a) }));
      else if (kind === "remove") setState(s => ({ ...s, apps: s.apps.map(a => a.id === app.id ? { ...a, installed: false, phase: "not_installed", version_installed: null, version_app: null, preview: null, restore_available: false } : a) }));
      return;
    }
    const id = app.id;
    void run(() => kind === "install" ? api.installApp(id, true) : kind === "update" ? api.updateApp(id, true) : kind === "reinstall" ? api.reinstallApp(id, true) : kind === "start" ? api.startApp(id) : kind === "stop" ? api.stopApp(id) : kind === "open" ? api.openApp(id) : kind === "remove" ? api.removeApp(id, "REMOVE") : api.cancelOperation(id));
  };
  const appBusy = (a: AppState) => working(a.phase) || state().operations.some(op => op.app_id === a.id);
  // Tabs show yellow while installing or updating, green for other running commands, red after a failure.
  const tabDot = (a: AppState | undefined) => {
    if (!a) return "";
    const ops = state().operations.filter(op => op.app_id === a.id).map(op => op.kind);
    if (["installing", "updating", "removing"].includes(a.phase) || ops.some(kind => ["install", "update", "reinstall", "remove"].includes(kind))) return "busy";
    if (appBusy(a)) return "live";
    return a.phase === "failed" ? "failed" : "";
  };
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
  createEffect(() => {
    if (!terminalMenu()) return;
    const close = () => { setTerminalMenu(null); setForceTerminalConfirm(false); };
    const escape = (e: KeyboardEvent) => { if (e.key === "Escape") setTerminalMenu(null); };
    const timer = setTimeout(() => window.addEventListener("mousedown", close), 0);
    window.addEventListener("keydown", escape);
    onCleanup(() => { clearTimeout(timer); window.removeEventListener("mousedown", close); window.removeEventListener("keydown", escape); });
  });
  const setTerminal = (terminal: string) => {
    const settings = { ...state().settings, terminal };
    if (mockMode()) { setState(s => ({ ...s, settings })); return; }
    void run(() => api.saveSettings(settings));
  };
  const migratePg0 = (app: AppState, toShared: boolean) => void run(async () => {
    const target = toShared ? "Autark's shared folder" : `${app.name}'s own folder`;
    if (!window.confirm(`Copy the pg0 PostgreSQL binaries into ${target}? The current copy is backed up and stays in place.`)) return;
    if (mockMode()) return;
    await api.migratePg0(app.id, toShared);
  });  const setForceTerminal = (force_terminal_operations: boolean) => {
    const settings = { ...state().settings, force_terminal_operations };
    if (mockMode()) { setState(s => ({ ...s, settings })); return; }
    void run(() => api.saveSettings(settings));
  };  const setGlobalPref = (key: "tray_enabled" | "start_with_windows" | "close_to_tray" | "experimental_local_apps" | "force_local_mise" | "animations" | "developer_mode", next: boolean) => {
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
      const dir = mockMode() ? `D:\\Portable\\Autark\\apps\\${app.id}\\${field.kind === "file" ? "llm-api-key.txt" : "huggingface"}` : field.kind === "file" ? await api.pickFile(`Choose ${field.label}`, "any") : await api.pickDirectory("root_parent");
      if (dir) setDraftValue(app, field.key, asPortable(dir, `${state().settings.root}\\apps\\${app.id}`));
    }
    catch (e) { setRequestError(errText(e)); }
  };
  // A click must say something happened; the shell itself opens in its own window.
  const openTerminal = () => {
    if (mockMode()) { setTerminalNotice("Terminal opened in the preview folder."); return; }
    void api.openTerminal().then(message => setTerminalNotice(message)).catch(e => setRequestError(errText(e)));
  };
  createEffect(() => {
    if (!terminalNotice()) return;
    const timer = setTimeout(() => setTerminalNotice(""), 6000);
    onCleanup(() => clearTimeout(timer));
  });
  const openUrl = (url: string) => { if (!mockMode()) void api.openUrl(url).catch(e => setRequestError(errText(e))); };
  const revealFile = (path: string) => { if (!mockMode()) void api.revealFile(path).catch(e => setRequestError(errText(e))); };
  const reveal = (target: "root" | "marketplace" | "app" | "settings", id?: string) => { if (!mockMode()) void api.revealPath(target, id).catch(e => setRequestError(errText(e))); };
  const refresh = () => mockMode() ? undefined : void run(() => api.refreshMarketplace());
  const doUpdateAll = () => {
    if (mockMode()) return;
    setAllConfirm(false); void run(() => api.updateAll(true));
  };
  const sourceValue = (a: AppState) => sourceDrafts()[a.id] ?? a.preview ?? "";
  const setSourceValue = (a: AppState, value: string) => setSourceDrafts(d => ({ ...d, [a.id]: value }));
  const clearSourceDraft = (id: string, submitted: string) => setSourceDrafts(d => {
    // Keep text the user typed while the refresh was running.
    if ((d[id] ?? "").trim() !== submitted) return d;
    const next = { ...d };
    delete next[id];
    return next;
  });
  const applyAppSource = async (a: AppState) => {
    const source = sourceValue(a).trim();
    setSourceValue(a, source);
    if (mockMode()) {
      setState(s => ({ ...s, apps: s.apps.map(x => x.id === a.id ? { ...x, preview: source || null, restore_available: !!source } : x) }));
      clearSourceDraft(a.id, source);
      return;
    }
    await run(() => api.refreshAppSource(a.id, source));
    // Let the input follow the stored source instead of keeping the entered text.
    clearSourceDraft(a.id, source);
  };
  const reloadAppSource = (a: AppState) => { if (!mockMode()) void run(() => api.refreshAppSource(a.id)); };
  // Manifest groups become tabs; preview is a reserved id, so a group named
  // "Preview" keeps its own tab and its settings stay editable.
  const PREVIEW_TAB = "__preview";
  const tabLabel = (tab: string) => tab === PREVIEW_TAB ? "Preview" : tab;
  const appTabs = (a: AppState) => ["General", ...new Set(a.settings_schema.map(f => f.group).filter((g): g is string => Boolean(g))), ...(a.local || !a.installed ? [] : [PREVIEW_TAB])];
  const tabFields = (a: AppState, tab: string) => a.settings_schema.filter(f => (f.group ?? "General") === tab);
  const currentTab = (a: AppState) => appTabs(a).includes(appSettingsTab()) ? appSettingsTab() : "General";
  const restoreApp = (a: AppState) => {
    if (mockMode()) {
      setState(s => ({ ...s, apps: s.apps.map(x => x.id === a.id ? { ...x, preview: null, restore_available: false } : x) }));
      return;
    }
    void run(() => api.restoreApp(a.id, true));
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
  const pane = () => state().settings.wizard_complete || miseScreen() ? misePane() : wizardStep() === 1 ? introPane() : wizardStep() === 2 ? folderPane() : wizardStep() === 3 ? misePane() : gitPane();
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
          <button class="path path-edit" title="Change Autark folder" onClick={() => void chooseRoot()}><span>{state().settings.root}</span><span class="icon-button"><Icon name="pencil" /></span></button>
          <button class="button primary" onClick={() => setWizardStep(3)}>Next</button>
    </>
  );
  const misePane = () => (
    <>
      <div class="empty-mark" aria-hidden="true">autark</div>
          <div><h1>{state().mise.available ? "Choose mise" : "mise is required"}</h1><p>Autark runs every app through <button class="inline-link" onClick={() => openUrl("https://github.com/jdx/mise")}>mise</button> and the <code>mise</code> command is not available on this machine.</p></div>
          <section><h2>I already have mise</h2><p>Point Autark at the existing <code>mise.exe</code>.</p><Show when={state().mise.detected.length}><div class="mise-detected" role="list" aria-label="Detected mise"><For each={state().mise.detected}>{entry => <button class={`mise-detected-row ${state().mise.path === entry.path ? "selected" : ""}`} role="listitem" title={entry.path} disabled={busy()} onClick={() => void run(() => pickMise(entry.path))}><span title={entry.path}>{entry.path}</span><small>{calver(entry.version)}</small></button>}</For></div></Show><button class="button" disabled={busy()} onClick={() => void run(async () => { const path = await api.pickFile("Choose mise.exe", "exe"); return path ? api.setMisePath(path) : undefined; })}>Choose another mise.exe</button></section>
          <section><h2>Install it for Autark</h2><p>Autark downloads mise into its own folder and keeps it there. Nothing is installed machine-wide.</p><footer><span>Local folder: {state().mise.local_path}{state().mise.force_local ? " · Always use Autark's own mise" : ""}</span><button class="icon-button" aria-label="Change Autark folder" title="Change Autark folder" onClick={() => void chooseRoot()}><Icon name="pencil" /></button></footer><Show when={localInstalled()}><div class={`mise-detected-row ${localSelected() ? "selected" : ""}`} role="listitem"><button class="mise-pick" title={state().mise.local_path} disabled={busy()} onClick={() => void run(() => pickMise(state().mise.local_path))}><span title={state().mise.local_path}>{state().mise.local_path}</span><small>{calver(state().mise.version)}</small></button><button class="icon-button" aria-label="Remove Autark's mise" title="Remove Autark's mise" disabled={busy()} onClick={() => void run(() => api.removeBinEntry("mise.exe"))}><Icon name="trash" /></button></div></Show><Show when={!localInstalled()}><Show when={downloading()}><div class="mise-bar" role="progressbar" aria-valuemin="0" aria-valuemax="100" aria-valuenow={misePercent() ?? 0}><span style={{ width: `${misePercent() ?? 0}%` }} /></div><small class="mise-progress">{misePercent() ?? 0}%</small></Show><button class="button primary" disabled={busy()} onClick={() => { setMiseInstalling(true); setBuffers(b => ({ ...b, "mise.exe": [] })); void run(() => api.installMise()); }}>Install</button></Show></section>
          <Show when={state().mise.available}><button class="button primary" disabled={busy()} onClick={() => setWizardStep(4)}>Next</button></Show>
    </>
  );
  const gitPane = () => (
    <>
      <div class="empty-mark" aria-hidden="true">autark</div>
      <div><h1>One more tool: git</h1><p>Autark installs <button class="inline-link" onClick={() => openUrl("https://git-scm.com")}>git</button> once, through its own mise, so it stays in the same folder as everything else.</p></div>
      <Show when={state().git.installed} fallback={
        <section><h2>Install git for Autark</h2><p>PortableGit lands in Autark's mise installs and is never added machine-wide.</p>
          <Show when={state().git.external}>{external => <div class="mise-detected" role="list" aria-label="Detected git"><div class="mise-detected-row" role="listitem"><span title={external().path}>{external().path}</span><small>{calver(external().version)}</small></div></div>}</Show>
          <button class="button primary" disabled={busy()} onClick={() => { setBuffers(b => ({ ...b, "git.exe": [] })); void run(() => api.installGit()); }}>Install git</button></section>
      }>
        <section><h2>git is ready</h2><div class="mise-detected" role="list" aria-label="Autark git"><div class="mise-detected-row selected" role="listitem"><button class="mise-pick" title={state().git.path ?? ""} disabled={busy()} onClick={() => openBuiltin("git.exe")}><span title={state().git.path ?? ""}>{state().git.path}</span><small>{calver(state().git.version)}</small></button></div></div></section>
      </Show>
      <Show when={!state().git.installed && state().git.external}>{external => <p class="market-unavailable">Autark keeps its own copy. The git at <button class="inline-link" onClick={() => revealFile(external().path)}>{external().path}</button> stays untouched.</p>}</Show>
      <button class="button primary" disabled={busy() || (!state().git.installed && !state().git.external)} onClick={() => { if (mockMode()) { setState(s => ({ ...s, settings: { ...s.settings, wizard_complete: true } })); return; } void run(() => api.saveSettings({ ...state().settings, wizard_complete: true }, false)); }}>Next</button>
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
      <button class="add-tab terminal-launch" aria-label="Open terminal" title="Open the Autark terminal (right click to pick the shell)" onClick={openTerminal} onContextMenu={e => {
        e.preventDefault();
        const rect = e.currentTarget.getBoundingClientRect();
        setTerminalMenu(terminalMenu() ? null : { x: rect.left, y: rect.bottom + 6 });
      }}>&gt;_</button>
      <div class="tabs" role="tablist" aria-label="Open apps">
        <For each={openIds()}>{id => {
          const app = () => state().apps.find(a => a.id === id);
          return <button class={`tab ${active() === id && !settingsOpen() ? "active" : ""}`} role="tab" aria-selected={active() === id && !settingsOpen()} onClick={() => selectTab(id)}>
            <span class="badge" style={{ "--hue": hue(id) }}><Show when={app()?.icon_data} fallback={id[0]?.toUpperCase()}>{src => <img src={src()} alt="" />}</Show></span><span class="tab-name">{app()?.name ?? id}</span>
            <span class={`dot ${tabDot(app())}`} />
            <span class="tab-close" role="button" aria-label={`Close ${app()?.name ?? id}`} onClick={e => closeTab(id, e)}><Icon name="close" /></span>
          </button>;
        }}</For>
        <For each={builtinTabs()}>{name => <button class={`tab ${active() === name && !settingsOpen() ? "active" : ""}`} role="tab" aria-selected={active() === name && !settingsOpen()} onClick={() => openBuiltin(name)}><span class="badge" style={{ "--hue": hue(name) }}><Show when={builtinLogo(name)} fallback={builtinName(name)[0]?.toUpperCase()}>{src => <img src={src()} alt="" />}</Show></span><span class="tab-name">{builtinName(name)}</span><span class="tab-close" role="button" aria-label={`Close ${builtinName(name)}`} onClick={e => closeBuiltin(name, e)}><Icon name="close" /></span></button>}</For>
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
    <Show when={terminalMenu()}>{menu => {
      const pick = (value: string, label: string) => {
        const missing = () => (value === "sh" && !state().terminal.sh) || (value === "bash" && !state().terminal.bash);
        return <button role="menuitemradio" aria-checked={state().settings.terminal === value} disabled={missing()} onMouseDown={e => e.stopPropagation()} onClick={() => { setTerminalMenu(null); setTerminal(value); }}>
          <span class="terminal-menu-check">{state().settings.terminal === value ? "\u2713" : ""}</span>
          <span>{label}</span>
          <Show when={missing()}><small>not found</small></Show>
        </button>;
      };

      return <div class="terminal-menu" role="menu" aria-label="Terminal shell" style={{ left: `${menu().x}px`, top: `${menu().y}px` }} onMouseDown={e => e.stopPropagation()}>
        <span class="terminal-menu-head">Terminal</span>
        {pick("system", "System default")}
        {pick("powershell", "PowerShell")}
        {pick("cmd", "cmd")}
        {pick("sh", "sh")}
        {pick("bash", "bash")}
        <small class="terminal-menu-note">System default uses bash when git is installed.</small>
        <div class="terminal-menu-sep" />
        <button role="menuitemcheckbox" aria-checked={state().settings.force_terminal_operations} onClick={() => state().settings.force_terminal_operations ? setForceTerminal(false) : setForceTerminalConfirm(true)}>
          <span class="terminal-menu-check">{state().settings.force_terminal_operations ? "\u2713" : ""}</span>
          <span>Force this terminal for operations</span>
        </button>
        <small class="terminal-menu-note">Commands open in a window that outlives Autark.</small>
        <Show when={forceTerminalConfirm()}>
          <div class="terminal-menu-confirm">
            <p>Every command, an app start included, opens its own window and keeps it open after Autark exits. Autark cannot show that output in its log, a multi-step install opens several windows, and closing a window before the command finishes marks the operation failed.</p>
            <div><button class="button" onClick={() => setForceTerminalConfirm(false)}>Cancel</button><button class="button primary" onClick={() => { setForceTerminalConfirm(false); setForceTerminal(true); }}>Enable</button></div>
          </div>
        </Show>
      </div>;
    }}</Show>
    <main class="card">
      <Show when={!state().settings.wizard_complete || !state().mise.available || miseScreen()} fallback={<>
      <Show when={settingsOpen()}><section class="settings-view">
        <button class="back-button" onClick={() => { const target = settingsReturn() ?? AUTARK_TAB; setSettingsOpen(false); setActive(target); setChooser(false); setAppSettingsOpen(null); }}>← back to homepage</button>
        <div class="view-heading"><div><span class="eyebrow">AUTARK</span><h1>Settings</h1></div></div>
        <nav class="settings-tabs" role="tablist" aria-label="Settings sections">
          <For each={["General", "Apps", "Preview", "Display", "Experimental", "About"] as const}>{tab => <button class={`settings-tab ${settingsTab() === tab ? "active" : ""}`} role="tab" aria-selected={settingsTab() === tab} onClick={() => setSettingsTab(tab)}>{tab}</button>}</For>
        </nav>
        <div class="settings-content">
        <Show when={settingsTab() === "General"}>
        <section class="setting-row"><div><h2>Root folder</h2><p>Apps and managed tools live here.</p><button class="path" onClick={() => reveal("root")}>{state().settings.root}</button></div><button class="button" disabled={state().busy} onClick={() => void changeRoot()}>Change</button></section>
        <section class="setting-row"><div><h2>mise</h2><p>{state().mise.path ?? "not resolved"}{state().mise.version ? ` · ${state().mise.version}` : ""}</p></div><button class="button" disabled={state().busy} onClick={() => { setMiseScreen(true); setSettingsOpen(false); setActive(null); setChooser(false); }}>Change</button></section>
        <section class="preference-section"><h2>Terminal</h2><p>The &gt;_ button opens this shell in the Autark folder. System default uses bash when git is available.</p>
          <div class="market-filters" role="group" aria-label="Terminal shell">
            <For each={[["system", "System default"], ["powershell", "PowerShell"], ["cmd", "cmd"], ["sh", "sh"], ["bash", "bash"]] as const}>{([value, label]) => {
              const missing = () => (value === "sh" && !state().terminal.sh) || (value === "bash" && !state().terminal.bash);
              return <button class={`market-filter ${state().settings.terminal === value ? "active" : ""}`} aria-pressed={state().settings.terminal === value} disabled={missing()} title={missing() ? `${label} was not found on this machine` : `Open ${label}`} onClick={() => setTerminal(value)}>{label}</button>;
            }}</For>
          </div>
        </section>
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
        <section class="setting-row"><div><h2>Marketplace</h2><p>{state().marketplace.exists ? <button class="path" onClick={() => reveal("marketplace")}>{state().marketplace.path}</button> : "Marketplace has not been loaded."}</p><Show when={state().marketplace.last_refresh}><small>Last refreshed {new Date(state().marketplace.last_refresh! * 1000).toLocaleString()}</small></Show></div><span class="row-buttons"><button class="button" disabled={state().busy} onClick={refresh}>{state().marketplace.exists ? "Reload marketplace" : "Load marketplace"}</button></span></section>
        <section class="setting-row"><div><h2>Updates</h2><p>Check marketplace versions manually.</p></div><button class="button" disabled={state().busy} onClick={() => mockMode() ? undefined : void run(() => api.checkUpdates())}>Check for updates</button></section>
        <section class="updates"><h2>Update all</h2><p>Running apps stop during updates and cannot be used until each update finishes.</p>
          <div class="update-table">
            <For each={state().apps.filter(a => a.installed)}>{a => <article class="update-app"><h3>{a.name}<Show when={a.preview}>{(preview) => <> <span class="preview-pill">preview ({preview()})</span></>}</Show></h3><div class="update-details"><span>App version <b>{a.version_app ?? "unknown"}</b></span><span>Installed <b>{a.version_installed ?? "not installed"}</b></span><span>Marketplace <b>{a.version_latest ?? "unknown"}</b></span><span>State <b>{a.phase}</b></span></div></article>}</For>
          </div>
          <Show when={!allConfirm()}><button class="button primary" disabled={state().busy || !state().apps.some(a => a.installed && a.update_available)} onClick={() => setAllConfirm(true)}>Update all</button></Show>
          <Show when={allConfirm()}><div class="confirm-strip"><span>Update every installed app now?</span><button class="button" onClick={() => setAllConfirm(false)}>Cancel</button><button class="button primary" disabled={state().busy} onClick={doUpdateAll}>Confirm</button></div></Show>
        </section>
        </Show>
        <Show when={settingsTab() === "Preview"}>
          <section class="preference-section"><h2>Developer mode</h2><p>Adds preview controls to every app: load one app's recipe from a branch or pull request, refresh it, or restore it to the default branch.</p>
            <label class="preference-toggle"><input type="checkbox" role="switch" checked={state().settings.developer_mode} onChange={e => setGlobalPref("developer_mode", e.currentTarget.checked)} /><span><b>Developer mode</b><small>Show the preview tab in the app settings and the preview actions on the app page. Remembered until you turn it off.</small></span></label>
            <Show when={!state().settings.developer_mode}><small class="preference-note">Preview controls are hidden while this is off.</small></Show>
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
        <footer class="about"><span>Autark <b>v{version()}{import.meta.env.DEV ? " (dev)" : ""}</b></span><span>AGPL-3.0</span><button onClick={() => openUrl("https://github.com/verzly/autark")}>Source</button><button onClick={() => openUrl("https://github.com/verzly/autark-marketplace")}>Marketplace</button></footer>
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
                   return <article role="listitem" class={`market-card ${selected() === i() ? "market-selected" : ""}`} onMouseEnter={() => setSelected(i())}>
                      <div class="market-card-head"><span class="badge large" style={{ "--hue": hue(a.id) }}><Show when={a.icon_data} fallback={a.id[0]?.toUpperCase()}>{src => <img src={src()} alt="" />}</Show></span><div><h2>{a.name}</h2><code>{a.id}</code></div></div>
                      <p class="market-description">{a.description}</p><div class="market-versions"><span>Installed <b>{a.version_installed ?? "not installed"}</b><Show when={a.preview}>{preview => <> <span class="preview-pill">preview ({preview()})</span></>}</Show></span><span>Marketplace <b>{a.local ? "local" : a.version_latest ?? "unknown"}</b></span></div>
                     <div class="market-requirements"><Show when={a.requirements.disk_gb != null}><span>{a.requirements.disk_gb} GB disk</span></Show><Show when={a.requirements.webview2}><span>WebView2</span></Show></div>
                     <div class="market-links"><Show when={a.license}><span>{a.license}</span></Show><Show when={a.homepage}><button onClick={() => openUrl(a.homepage!)}>Homepage ↗</button></Show></div>
                      <Show when={a.installed} fallback={<div class="market-actions"><button class="button market-view-action" disabled={working(a.phase)} onClick={() => openApp(a.id)}>View</button><button class="button primary market-action" disabled={working(a.phase)} aria-label={`Install ${a.name}`} title="Install" onClick={() => installMarketplaceApp(a)}><Icon name="download" /></button></div>}>
                        <div class="market-actions"><Show when={a.local}><button class="button market-view-action" disabled={busy()} onClick={() => replaceLocalApp(a)}>Edit</button></Show><Show when={!a.local && (a.update_available || upstreamNewer(a))}><button class="icon-button update-button" disabled={working(a.phase)} aria-label={`Update ${a.name}`} title="Update" onClick={() => installMarketplaceApp(a)}><Icon name="download" /></button></Show><button class={`button market-action icon-only ${opened() ? "opened-action" : "primary"}`} disabled={!opened() && working(a.phase)} aria-label={opened() ? `Show ${a.name}` : `Open ${a.name}`} title={opened() ? "Show window" : "Open"} onClick={() => opened() ? openApp(a.id) : activateMarketplaceApp(a)}><Icon name="eye" /></button></div>
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
               return <article role="listitem" class={`market-card ${selected() === i() ? "market-selected" : ""}`} onMouseEnter={() => setSelected(i())}>
                  <div class="market-card-head"><span class="badge large" style={{ "--hue": hue(a.id) }}><Show when={a.icon_data} fallback={a.id[0]?.toUpperCase()}>{src => <img src={src()} alt="" />}</Show></span><div><h2>{a.name}</h2><code>{a.id}</code></div><span class="local-tag">local</span></div>
                  <p class="market-description">{a.description}</p><div class="market-versions"><span>Installed <b>{a.version_installed ?? "not installed"}</b></span><span>Marketplace <b>local</b></span></div>
                 <div class="market-links"><Show when={a.license}><span>{a.license}</span></Show><Show when={a.homepage}><button onClick={() => openUrl(a.homepage!)}>Homepage ↗</button></Show></div>
                 <div class="market-actions"><button class="button market-view-action" disabled={working(a.phase)} onClick={() => replaceLocalApp(a)}>Edit</button>
                   <Show when={a.installed} fallback={<button class="button primary market-action" disabled={working(a.phase)} onClick={() => installMarketplaceApp(a)}>Install</button>}>
                     <button class={`button market-action icon-only ${opened() ? "opened-action" : "primary"}`} disabled={!opened() && working(a.phase)} aria-label={opened() ? `Show ${a.name}` : `Open ${a.name}`} title={opened() ? "Show window" : "Open"} onClick={() => opened() ? openApp(a.id) : activateMarketplaceApp(a)}><Icon name="eye" /></button>
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
                  const isGit = entry.name.toLowerCase() === "git.exe";
                  const isPg0 = entry.name.toLowerCase() === "pg0.exe";
                  const git = state().git;
                  const systemMise = isResolvedMise(entry.name);
                  const tag = isGit ? (git.installed ? "By Autark" : git.external?.from_mise ? "By mise" : git.external ? "By your machine" : "Not installed") : isPg0 ? (state().pg0.available && state().pg0.path?.startsWith(state().settings.root) ? "By Autark" : "Not installed") : systemMise ? "Not installed" : "By Autark";
                  const external = isGit ? git.external?.path ?? null : isPg0 ? state().pg0.source ?? state().pg0.path ?? null : state().mise.path;
                  return <article role="listitem" class={`market-card ${selected() === i() ? "market-selected" : ""}`} onMouseEnter={() => setSelected(i())}>
                    <div class="market-card-head"><span class="badge large" style={{ "--hue": hue(entry.name) }}><Show when={builtinLogo(entry.name)} fallback={title[0]?.toUpperCase()}>{src => <img src={src()} alt="" />}</Show></span><div><h2>{title}</h2><code>{entry.name}</code></div><span class="local-tag">{tag}</span></div>
                    <p class="market-description">{isGit ? "Installed once by Autark's mise, next to the rest of its runtime." : isPg0 ? "Installed by Autark's mise. Apps share the payload only after you move it into Autark." : systemMise ? "Autark runs every app through this mise." : "Managed in Autark's own bin folder."}</p>
                    <Show when={isGit ? git.installed : isPg0 ? state().pg0.available : !systemMise}><div class="market-versions"><span>Version <b>{entry.version ?? "unknown"}</b></span><span>By <b>{isGit ? "Autark" : isPg0 ? "Autark" : state().mise.source}</b></span></div>
                      <div class="market-links"><span>{isGit ? git.path : isPg0 ? state().pg0.path ?? "not resolved" : state().mise.bin_path}</span></div></Show>
                    <div class="market-actions">
                      <Show when={!systemMise}><button class="button market-action icon-only" aria-label={`Show ${title}`} title="Show" onClick={() => openBuiltin(entry.name)}><Icon name="eye" /></button></Show>
                      <Show when={!isGit && !isPg0 && !systemMise}><button class="button primary market-action" disabled={busy()} onClick={() => { openBuiltin(entry.name); void run(() => api.updateMise()); }}>Update</button></Show>
                      <Show when={isGit}><Show when={git.installed} fallback={<button class="button primary market-action" disabled={busy() || !state().mise.available} onClick={() => { openBuiltin(entry.name); void run(() => api.installGit()); }}>Install</button>}><button class="button primary market-action" disabled={busy()} onClick={() => { openBuiltin(entry.name); void run(() => api.updateGit()); }}>Update</button></Show></Show>
                      <Show when={isPg0}><Show when={state().pg0.available} fallback={<button class="button primary market-action" disabled={busy()} title="Install pg0 with Autark's mise" onClick={() => void run(() => api.installPg0(false))}>Install</button>}><button class="button primary market-action" disabled={busy()} title="Install the latest pg0 with Autark's mise" onClick={() => void run(() => api.installPg0(true))}>Update</button></Show></Show>
                      <Show when={!systemMise && !isGit && !isPg0}>
                        <Show when={binRemoveConfirm() !== entry.name}><button class="button market-view-action" onClick={() => setBinRemoveConfirm(entry.name)}>Remove</button></Show>
                        <Show when={binRemoveConfirm() === entry.name}><div class="confirm-strip"><span>Remove {entry.name} from Autark's bin folder?</span><button class="button" onClick={() => setBinRemoveConfirm(null)}>Cancel</button><button class="button" disabled={busy()} onClick={() => { setBinRemoveConfirm(null); void run(() => api.removeBinEntry(entry.name)); }}>Remove</button></div></Show>
                      </Show>
                      <Show when={systemMise}><p class="market-unavailable">{external ? <>Autark currently uses the external mise at <button class="inline-link" onClick={() => revealFile(external)}>{external}</button></> : "No mise found on this machine."}</p></Show>
                      <Show when={isGit && !git.installed}><p class="market-unavailable">{external ? <>Autark currently uses the {git.external?.from_mise ? "git installed by mise" : "git"} at <button class="inline-link" onClick={() => revealFile(external)}>{external}</button></> : "No git found on this machine."}</p></Show>
                    </div>
                  </article>;
                }}</For></div>
              </Show>
            </section>
          </Show>
       </section></Show>
      <Show when={!chooser() && !settingsOpen() && isBuiltin(active())}><section class="app-view">
        <div class="app-header"><div class="app-title"><span class="badge large"><Show when={builtinLogo(active() ?? "")} fallback={builtinName(active())[0]?.toUpperCase()}>{src => <img src={src()} alt="" />}</Show></span><div><h1>{builtinName(active())}</h1><div class="version-line">version {active()?.toLowerCase() === "git.exe" ? state().git.version ?? "unknown" : state().mise.version ?? "unknown"} · {active()?.toLowerCase() === "git.exe" ? (state().git.installed ? "By Autark" : state().git.external?.from_mise ? "By mise" : state().git.external ? "By your machine" : "Not installed") : (state().mise.source === "local" ? "By Autark" : "By your machine")}</div></div></div></div>
        <div class="actions"><button class="button" onClick={() => { setMarketFilter("builtin"); showChooser(); }}>Back to built-in apps</button><Show when={active()?.toLowerCase() !== "git.exe" && active()?.toLowerCase() !== "pg0.exe"}><Show when={localInstalled()}><button class="button primary" disabled={busy()} onClick={() => void run(() => api.updateMise())}>Update</button></Show><button class="button" disabled={busy()} onClick={() => setMiseScreen(true)}>Change mise.exe</button><Show when={localInstalled()}><button class="button danger" disabled={busy()} onClick={() => void run(() => api.removeBinEntry("mise.exe"))}>Remove</button></Show></Show><Show when={active()?.toLowerCase() === "git.exe"}><Show when={state().git.installed} fallback={<button class="button primary" disabled={busy()} onClick={() => void run(() => api.installGit())}>Install</button>}><button class="button primary" disabled={busy()} onClick={() => void run(() => api.updateGit())}>Update</button></Show></Show></div>
        <div class="output-wrap"><div class="output" ref={output} onScroll={() => output && setFollow(output.scrollHeight - output.scrollTop - output.clientHeight < 24)}>
          <For each={logEntries()}>{entry => entry.spacer ? <div class="log-spacer" aria-hidden="true" /> : <div class={`log ${lineTone(entry.line)}`}><LogText line={entry.line} /></div>}</For>
        </div></div>
        <footer class="metadata"><span>{active()?.toLowerCase() === "git.exe" ? state().git.path ?? "not resolved" : state().mise.path ?? "not resolved"}</span><code>{builtinName(active())}</code></footer>
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
          <div class="app-header"><div class="app-title"><span class="badge large" style={{ "--hue": hue(a().id) }}><Show when={a().icon_data} fallback={a().id[0]?.toUpperCase()}>{src => <img src={src()} alt="" />}</Show></span><div><h1>{a().name}</h1><div class="version-line">{a().version_app ? `app ${a().version_app}` : "app version unknown"} · {a().local ? "local" : `recipe ${a().version_installed ?? "not installed"}`}<Show when={!a().local && a().update_available}> · latest recipe {a().version_latest ?? "unknown"} <Show when={!upstreamNewer(a())}><span class="update-pill">update</span></Show></Show><Show when={!a().local && upstreamNewer(a())}> · upstream release {upstreamVersions()[a().id]}</Show><Show when={a().preview}>{(preview) => <> · <span class="preview-pill">preview ({preview()})</span></>}</Show></div></div></div>
            <div class="links"><Show when={a().homepage}><button onClick={() => openUrl(a().homepage!)}>Homepage ↗</button></Show><Show when={a().license}><span>{a().license}</span></Show><Show when={a().preview && a().installed && !a().local}>{(preview) => <div class="preview-actions"><button class="button" disabled={busyApp()} onClick={() => setPanel("restore")}>Restore to stable</button><button class="reload-button" disabled={busyApp()} aria-label={`Refresh ${a().name} from ${preview()}`} title={`Refresh the recipe from ${preview()} without running setup`} onClick={() => reloadAppSource(a())}><Icon name="reload" /></button></div>}</Show></div></div>
          <div class="status-line"><span class={`dot ${p()}`} /><Show when={working(p())}><span class="progress"><i /></span></Show><span class="phase-label">{a().step ? `(${a().step?.index}/${a().step?.total}) ${a().step?.name}` : p().replace("_", " ")}</span><Show when={working(p()) && elapsed() != null}><span class="elapsed">run {duration(elapsed()!)}</span></Show></div>
          <div class="actions">
              <Show when={!a().local}><button class={`button primary ${a().installed && (a().update_available || upstreamNewer(a())) ? "update-ready" : ""}`} disabled={busyApp()} onClick={() => setPanel("install")}>{a().installed ? "Update" : "Install"}</button></Show>

            <Show when={a().can_start}>
              <button class="button" disabled={!canStart()} onClick={() => action("start", a())}>Start</button>
              <button class="button" disabled={!canStop()} onClick={() => action("stop", a())}>Stop</button>
            </Show>
            <button class="button" disabled={busyApp() || !a().url} onClick={() => action("open", a())}>Open</button>
             <button class="button" aria-label={`${a().name} settings`} onClick={() => { setPanel(null); setAppSettingsTab("General"); setAppSettingsOpen(a().id); }}><Icon name="menu" /> Settings</button>
             <For each={a().actions}>{item => <button class="button" disabled={busyApp() || !a().installed} onClick={() => item.confirm ? setPanel(`action:${item.id}`) : void run(() => api.runAction(a().id, item.id))}>{item.label}</button>}</For>
             <Show when={working(p())}><button class="button" onClick={() => action("cancel", a())}>Cancel operation</button></Show>
           </div>
           <Show when={panel() === "install"}><div class="inline-panel"><p>Setup may install system components through mise and requires network access.</p><div><button class="button" onClick={() => setPanel(null)}>Cancel</button><button class="button primary" disabled={busyApp()} onClick={confirmInstall}>Confirm</button></div></div></Show>
           <Show when={panel() === "restore"}><div class="inline-panel"><p>Reinstall this app from the marketplace's default branch? The preview build is replaced. Settings and user data are preserved.</p><div><button class="button" onClick={() => setPanel(null)}>Cancel</button><button class="button primary" disabled={busyApp()} onClick={() => { setPanel(null); restoreApp(a()); }}>Restore</button></div></div></Show>
           <For each={a().actions.filter(item => item.confirm && panel() === `action:${item.id}`)}>{item => <div class="inline-panel"><p>Run {item.label}?</p><div><button class="button" onClick={() => setPanel(null)}>Cancel</button><button class="button primary" disabled={busyApp() || !a().installed} onClick={() => { setPanel(null); void run(() => api.runAction(a().id, item.id)); }}>Confirm</button></div></div>}</For>
          <div class="app-tabs" role="tablist"><button class={`app-tab ${appTab() === "output" ? "active" : ""}`} role="tab" aria-selected={appTab() === "output"} onClick={() => setAppTab("output")}>Output</button><Show when={a().faq.length}><button class={`app-tab ${appTab() === "faq" ? "active" : ""}`} role="tab" aria-selected={appTab() === "faq"} onClick={() => setAppTab("faq")}>FAQ</button></Show><Show when={!a().faq.length || appTab() === "output"}><button class="app-tab app-tab-end" onClick={() => void run(() => navigator.clipboard.writeText(logs().join("\n") as string))}>Copy</button><button class="app-tab app-tab-clear" onClick={() => { setBuffers(b => ({ ...b, [a().id]: [] })); setFollow(true); }}>Clear</button></Show></div>
          <Show when={a().faq.length && appTab() === "faq"}><section class="faq"><For each={a().faq}>{item => <details class="faq-item"><summary>{item.question}</summary><p>{item.answer}</p></details>}</For></section></Show>
          <Show when={!a().faq.length || appTab() === "output"}>
          <section class="output-section">
            <div class="output-wrap"><div class="output" ref={output} onScroll={() => output && setFollow(output.scrollHeight - output.scrollTop - output.clientHeight < 24)}>
              <Show when={logs().length} fallback={<span class="quiet">Operation output will appear here.</span>}><For each={logEntries()}>{entry => <div class={entry.spacer ? "log-spacer" : `log ${lineTone(entry.line)}`} aria-hidden={entry.spacer || undefined}>{entry.spacer ? "\u00a0" : <LogText line={entry.line} />}</div>}</For></Show>
            </div><Show when={!follow() && logs().length}><button class="jump" onClick={() => { if (output) output.scrollTop = output.scrollHeight; setFollow(true); }}>Jump to latest</button></Show></div>
          </section>
          </Show>
          <footer class="metadata"><button title={state().settings.root + "\\apps\\" + a().id} onClick={() => reveal("app", a().id)}>{`${state().settings.root}\\apps\\${a().id}`}</button><span>{a().version_app ? `app ${a().version_app}` : "app version unknown"} {a().update_available ? `latest ${a().version_latest ?? "unknown"}` : "latest"}<button class="check-button" title="Check the marketplace for a newer version" onClick={() => void run(() => api.checkUpdates())}>(check)</button></span><span>{a().local ? "local" : `recipe ${a().version_installed ?? "not installed"} ${a().update_available ? `latest ${a().version_latest ?? "unknown"}` : "latest"}`}{a().local ? null : <button class="check-button" title="Check the marketplace for a newer version" onClick={() => void run(() => api.checkUpdates())}>(check)</button>}</span><code>{a().id}</code></footer>
        </section>}>
          <section class="app-settings-page">
            <button class="back-button" onClick={() => { if (dirty(a()) && !confirm("Unsaved settings changes will be lost. Leave without saving?")) return; setAppSettingsOpen(null); }}>← back to app</button>
            <div class="view-heading"><div><span class="eyebrow">{a().name.toUpperCase()}</span><h1>Settings</h1></div><Show when={dirty(a())}><button class="button primary" disabled={busyApp()} onClick={() => saveDraft(a())}>Save</button></Show></div>
            <nav class="settings-tabs" role="tablist" aria-label="App settings sections"><For each={appTabs(a())}>{tab => <button class={`settings-tab ${currentTab(a()) === tab ? "active" : ""}`} role="tab" aria-selected={currentTab(a()) === tab} onClick={() => setAppSettingsTab(tab)}>{tabLabel(tab)}</button>}</For></nav>
            <Show when={a().settings_schema.some(field => field.key === "pg0_installation_dir")}>
              <section class="preference-section"><h2>pg0 binaries</h2>
                <p class="settings-muted">PostgreSQL payload: {String(a().settings.pg0_installation_dir ?? "unknown")}. The databases stay in this app's folder either way.</p>
                <div class="row-buttons"><button class="button" disabled={busyApp()} onClick={() => migratePg0(a(), true)}>Move to Autark</button><button class="button" disabled={busyApp()} onClick={() => migratePg0(a(), false)}>Move into the app</button></div>
              </section>
            </Show>
            <Show when={currentTab(a()) === PREVIEW_TAB} fallback={<Show when={tabFields(a(), currentTab(a())).length} fallback={<p class="quiet">No settings in this tab.</p>}><div class="fields"><For each={tabFields(a(), currentTab(a()))}>{field => <SettingField app={a()} field={field} value={draftValue(a(), field.key)} settings={drafts()[a().id] ?? a().settings} lock={a().settings_locked.find(lock => lock.key === field.key)} setValue={value => setDraftValue(a(), field.key, value)} choose={() => void chooseFolder(a(), field)} />}</For></div></Show>}>
              <section class="preference-section"><h2>Preview source</h2>
                <Show when={state().settings.developer_mode} fallback={<><p class="settings-muted">Developer mode is off. Preview sources are hidden.</p><button class="button" onClick={() => { setAppSettingsOpen(null); setSettingsReturn(a().id); setSettingsOpen(true); setSettingsTab("Preview"); }}>Open settings</button></>}>
                  <p class="settings-muted">Load only this app's recipe from a branch or pull request. Every other app stays on the default branch. Leave empty for the default branch.</p>
                  <div class="field-control"><input class="search" aria-label="Branch or pull request for this app" placeholder="Default branch" value={sourceValue(a())} disabled={busyApp()} onInput={e => setSourceValue(a(), e.currentTarget.value)} /><button class="button" disabled={busyApp()} onClick={() => void applyAppSource(a())}>Apply</button><button class="icon-button" disabled={busyApp() || !a().preview} aria-label={`Refresh ${a().name} from its preview source`} title="Refresh the recipe from the current source" onClick={() => reloadAppSource(a())}><Icon name="reload" /></button></div>
                  <p class="settings-muted">Current source: {a().preview ? <span class="preview-pill">preview ({a().preview})</span> : "default branch"}</p>
                  <Show when={a().preview}><Show when={panel() !== "restore"}><button class="button" disabled={busyApp()} onClick={() => setPanel("restore")}>Restore to stable</button></Show><Show when={panel() === "restore"}><div class="inline-panel"><p>Reinstall this app from the marketplace's default branch? The preview build is replaced. Settings and user data are preserved.</p><div><button class="button" onClick={() => setPanel(null)}>Cancel</button><button class="button primary" disabled={busyApp()} onClick={() => { setPanel(null); restoreApp(a()); }}>Restore</button></div></div></Show></Show>
                </Show>
              </section>
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
            <footer class="about"><span>Autark <b>v{version()}{import.meta.env.DEV ? " (dev)" : ""}</b></span><span>AGPL-3.0</span><button onClick={() => openUrl("https://github.com/verzly/autark")}>Source</button><button onClick={() => openUrl("https://github.com/verzly/autark-marketplace")}>Marketplace</button></footer>
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
        <Show when={requestError()}><div class="toast error-banner" role="alert">{sentence(requestError())}</div></Show>
        <Show when={terminalNotice()}><div class="toast notice-banner" role="status">{terminalNotice()}</div></Show>
        <Show when={currentApp()?.last_error}>{err => <div class="toast error-banner" role="alert">{sentence(err().message)}</div>}</Show>
        <Show when={state().marketplace.error}>{err => <div class="toast error-banner" role="alert">{sentence(err().message)}</div>}</Show>
      </div>
    </Show>
  </div>;
}

function SettingField(props: { app: AppState; field: AppSetting; value: unknown; settings: Record<string, unknown>; lock?: AppState["settings_locked"][number]; setValue: (v: unknown) => void; choose: () => void }) {
  const [probe, setProbe] = createSignal<{ ok: boolean; text: string } | null>(null);
  const [probing, setProbing] = createSignal(false);
  const [models, setModels] = createSignal<string[] | null>(null);
  const check = props.field.key === "llm_base_url" ? "endpoint" : props.field.key === "llm_model" ? "model" : null;
  const ask = async (kind: "endpoint" | "model" | "models") => mockMode() ? (kind === "endpoint" ? "HTTP 200" : kind === "model" ? "Model available" : JSON.stringify(["gpt-oss-20b", "qwen3-8b"])) : await api.probeLlm(props.app.id, kind, props.settings);
  const runProbe = async () => {
    setProbing(true);
    try { setProbe({ ok: true, text: await ask(check!) }); }
    catch (error) { setProbe({ ok: false, text: String(error).slice(0, 100) }); }
    finally { setProbing(false); }
  };
  const runModels = async () => {
    setProbing(true);
    try { setModels(JSON.parse(await ask("models"))); setProbe(null); }
    catch (error) { setModels(null); setProbe({ ok: false, text: String(error).slice(0, 100) }); }
    finally { setProbing(false); }
  };
  const value = () => props.lock ? props.lock.value : String(props.value ?? props.field.default ?? "");
  return <label class="field"><span>{props.field.label}</span><Show when={props.field.kind === "boolean"} fallback={
    <><div class="field-control"><input classList={{ locked: Boolean(props.lock) }} disabled={Boolean(props.lock)} type={props.field.kind === "number" || props.field.kind === "port" ? "number" : "text"} value={value()} onInput={e => { setProbe(null); props.setValue(props.field.kind === "number" || props.field.kind === "port" ? Number(e.currentTarget.value) : e.currentTarget.value); }} /><Show when={props.field.kind === "path" || props.field.kind === "file"}><button class="button" disabled={Boolean(props.lock)} onClick={props.choose}>Choose</button></Show><Show when={check}><button class="button" disabled={Boolean(props.lock) || probing()} onClick={() => void runProbe()}>{check === "endpoint" ? "Test" : "Check"}</button></Show><Show when={check === "model"}><button class="button" disabled={Boolean(props.lock) || probing()} onClick={() => void runModels()}>Models</button></Show><Show when={probe()}>{result => <span class={`probe-result ${result().ok ? "ok" : "fail"}`} title={result().text}>{result().ok ? "OK" : "FAIL"}: {result().text}</span>}</Show></div><Show when={models()}>{ids => <div class="model-list">{ids().map(id => <button class="button" onClick={() => props.setValue(id)}>{id}</button>)}</div>}</Show></>
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
  // A logged command is never an error, even when its text mentions one.
  if (line.startsWith("$ ")) return "tone-command";
  return /error|failed|fatal|stopped/i.test(line) ? "tone-error" : /warning|warn/i.test(line) ? "tone-warning" : /installed|complete|ready/i.test(line) ? "tone-success" : /downloading|extracting|^\s*\d+%/i.test(line) ? "tone-dim" : "";
}

function LogText(props: { line: string }) {
  if (!props.line) return <>&nbsp;</>;
  if (!props.line.startsWith("$ ")) return <>{props.line}</>;
  return <><span class="prompt">$</span>{props.line.slice(1)}</>;
}
function Icon(props: { name: "menu" | "gridPlus" | "close" | "min" | "max" | "pencil" | "trash" | "reload" | "download" | "eye" }) {
  return <svg viewBox="0 0 16 16" aria-hidden="true">{props.name === "menu" ? <path d="M2 4h12M2 8h12M2 12h12" /> : props.name === "trash" ? <><path d="M2.6 4.4h10.8" /><path d="M6.2 4.4V2.9h3.6v1.5" /><path d="M4.1 4.4l.7 8.1a1.1 1.1 0 0 0 1.1 1h4.2a1.1 1.1 0 0 0 1.1-1l.7-8.1" /><path d="M6.7 7v4M9.3 7v4" /></> : props.name === "gridPlus" ? <><rect x="1.5" y="1.5" width="5" height="5" rx="1" /><rect x="9.5" y="1.5" width="5" height="5" rx="1" /><rect x="1.5" y="9.5" width="5" height="5" rx="1" /><path d="M12 9v5m-2.5-2.5h5" /></> : props.name === "close" ? <path d="m4 4 8 8m0-8-8 8" /> : props.name === "min" ? <path d="M3 8h10" /> : props.name === "max" ? <rect x="3.2" y="3.2" width="9.6" height="9.6" rx="1" /> : props.name === "reload" ? <><path d="M13 8a5 5 0 1 1-1.5-3.6" /><path d="M13.2 2.6v3.2h-3.2" /></> : props.name === "download" ? <><path d="M8 2.4v7.2" /><path d="m5 7 3 3 3-3" /><path d="M2.8 12.6h10.4" /></> : props.name === "eye" ? <><path d="M1.6 8s2.4-4.1 6.4-4.1S14.4 8 14.4 8s-2.4 4.1-6.4 4.1S1.6 8 1.6 8Z" /><circle cx="8" cy="8" r="2.1" /></> : <><path d="M10.4 3.1 13 5.7" /><path d="M2.4 13.6 3.3 10 10.6 2.7a1.3 1.3 0 0 1 1.8 0l.9.9a1.3 1.3 0 0 1 0 1.8L6 12.8l-3.6.8Z" /></>}</svg>;
}
