import { createEffect, createMemo, createSignal, For, onCleanup, onMount, Show } from "solid-js";
import type { AppSetting, AppState, Phase, State } from "./bridge";
import { api, mockMode } from "./bridge";
import { previewLogs, previewState, previewVariants } from "./mock";

type InlineAction = "install" | "remove" | null;
const working = (p: Phase) => ["installing", "starting", "stopping", "updating", "removing"].includes(p);
const errText = (e: unknown) => e instanceof Error ? e.message : String(e);
const hue = (id: string) => { let h = 0; for (const c of id) h = (h * 31 + c.charCodeAt(0)) | 0; return Math.abs(h) % 360; };
const duration = (s: number) => `${Math.floor(s / 60)}m ${String(Math.floor(s % 60)).padStart(2, "0")}s`;

export default function App() {
  const [state, setState] = createSignal<State>(previewState);
  const [active, setActive] = createSignal<string | null>(previewState.settings.active_tab);
  const [settingsOpen, setSettingsOpen] = createSignal(false);
  const [chooser, setChooser] = createSignal(false);
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
  const [clock, setClock] = createSignal(Date.now() / 1000);
  let dialogInput: HTMLInputElement | undefined;
  let output: HTMLDivElement | undefined;
  const currentApp = createMemo(() => state().apps.find(a => a.id === active()) ?? null);
  const openIds = () => state().settings.open_tabs;
  const available = createMemo(() => state().apps.filter(a => !openIds().includes(a.id)).filter(a => `${a.name} ${a.description} ${a.id}`.toLowerCase().includes(search().toLowerCase())));

  onMount(() => {
    let offState: (() => void) | undefined, offLog: (() => void) | undefined;
    if (!mockMode()) api.getState().then(s => { setState(s); setActive(s.settings.active_tab); }).catch(e => setRequestError(errText(e)));
    api.onState(s => { setState(s); setRequestError(""); }).then(f => offState = f);
    api.onLog(({ app_id, line }) => setBuffers(prev => ({ ...prev, [app_id]: [...(prev[app_id] ?? []), line].slice(-500) }))).then(f => offLog = f);
    if ("__TAURI_INTERNALS__" in window) import("@tauri-apps/api/app").then(({ getVersion }) => getVersion().then(setVersion)).catch(() => {});
    onCleanup(() => { offState?.(); offLog?.(); });
  });
  const [version, setVersion] = createSignal("dev");
  createEffect(() => { if (chooser()) queueMicrotask(() => dialogInput?.focus()); });
  createEffect(() => { if (output && follow()) output.scrollTop = output.scrollHeight; });
  createEffect(() => {
    const app = currentApp();
    if (!app || !working(app.phase)) return;
    setClock(Date.now() / 1000);
    const id = window.setInterval(() => setClock(Date.now() / 1000), 1000);
    onCleanup(() => window.clearInterval(id));
  });
  const logs = () => buffers()[active() ?? ""] ?? [];
  const run = async (fn: () => Promise<State | void>) => {
    if (mockMode()) return;
    setBusy(true); setRequestError("");
    try { const next = await fn(); if (next) { setState(next); setActive(next.settings.active_tab); } }
    catch (e) { setRequestError(errText(e)); }
    finally { setBusy(false); }
  };
  const selectTab = (id: string | null) => {
    setActive(id); setSettingsOpen(false);
    if (!mockMode()) void run(() => api.setActiveTab(id));
  };
  const openApp = (id: string) => {
    if (mockMode()) { setState(s => ({ ...s, settings: { ...s.settings, open_tabs: [...s.settings.open_tabs, id], active_tab: id } })); setActive(id); }
    else void run(() => api.openAppTab(id));
    setChooser(false); setSearch(""); setSelected(0);
  };
  const closeTab = (id: string, event: MouseEvent) => {
    event.stopPropagation();
    if (mockMode()) { const tabs = openIds().filter(x => x !== id); setState(s => ({ ...s, settings: { ...s.settings, open_tabs: tabs, active_tab: tabs[tabs.length - 1] ?? null } })); if (active() === id) setActive(openIds().find(x => x !== id) ?? null); }
    else void run(() => api.closeTab(id));
  };
  const action = (kind: "install" | "update" | "start" | "stop" | "open" | "remove" | "cancel", app: AppState) => {
    if (mockMode()) {
      if (kind === "install" || kind === "update") setState(s => ({ ...s, apps: s.apps.map(a => a.id === app.id ? { ...a, ...previewVariants.installing } : a) }));
      else if (kind === "start") setState(s => ({ ...s, apps: s.apps.map(a => a.id === app.id ? { ...a, phase: "running", url: "http://127.0.0.1:8888" } : a) }));
      else if (kind === "stop") setState(s => ({ ...s, apps: s.apps.map(a => a.id === app.id ? { ...a, phase: "installed", url: null } : a) }));
      else if (kind === "remove") setState(s => ({ ...s, apps: s.apps.map(a => a.id === app.id ? { ...a, installed: false, phase: "not_installed", version_installed: null, version_app: null } : a) }));
      return;
    }
    const id = app.id;
    void run(() => kind === "install" ? api.installApp(id, true) : kind === "update" ? api.updateApp(id, true) : kind === "start" ? api.startApp(id) : kind === "stop" ? api.stopApp(id) : kind === "open" ? api.openApp(id) : kind === "remove" ? api.removeApp(id, "REMOVE") : api.cancelOperation(id));
  };
  const appBusy = (a: AppState) => busy() || working(a.phase);
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
  const saveDraft = (app: AppState) => {
    const settings = drafts()[app.id] ?? app.settings;
    if (mockMode()) setState(s => ({ ...s, apps: s.apps.map(a => a.id === app.id ? { ...a, settings } : a) }));
    else void run(() => api.saveAppSettings(app.id, settings));
    setDrafts(d => { const next = { ...d }; delete next[app.id]; return next; });
  };
  const draftValue = (app: AppState, key: string) => (drafts()[app.id] ?? app.settings)[key];
  const setDraftValue = (app: AppState, key: string, value: unknown) => setDrafts(d => ({ ...d, [app.id]: { ...(d[app.id] ?? app.settings), [key]: value } }));
  const dirty = (app: AppState) => JSON.stringify(drafts()[app.id] ?? app.settings) !== JSON.stringify(app.settings);
  const chooseFolder = async (app: AppState, field: AppSetting) => {
    if (field.key !== "root") return;
    try { const dir = mockMode() ? "D:\\Portable\\Data" : await api.pickDirectory("root_parent"); if (dir) setDraftValue(app, field.key, dir); }
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
    if (e.key === "Escape") setChooser(false);
    if (e.key === "ArrowDown") { e.preventDefault(); setSelected(i => Math.min(i + 1, available().length - 1)); }
    if (e.key === "ArrowUp") { e.preventDefault(); setSelected(i => Math.max(i - 1, 0)); }
    if (e.key === "Enter" && available()[selected()]) openApp(available()[selected()].id);
  };

  return <div class="app">
    <header class="chrome" data-tauri-drag-region>
      <button class={`gear ${settingsOpen() ? "selected" : ""}`} aria-label="Settings" aria-pressed={settingsOpen()} onClick={() => { setSettingsOpen(true); setActive(null); }}><Icon name="gear" /></button>
      <div class="tabs" role="tablist" aria-label="Open apps">
        <For each={openIds()}>{id => {
          const app = () => state().apps.find(a => a.id === id);
          return <button class={`tab ${active() === id && !settingsOpen() ? "active" : ""}`} role="tab" aria-selected={active() === id && !settingsOpen()} onClick={() => selectTab(id)}>
            <span class="badge" style={{ "--hue": hue(id) }}>{id[0]?.toUpperCase()}</span><span class="tab-name">{app()?.name ?? id}</span>
            <span class={`dot ${app()?.phase ?? "not_installed"}`} />
            <span class="tab-close" role="button" aria-label={`Close ${app()?.name ?? id}`} onClick={e => closeTab(id, e)}><Icon name="close" /></span>
          </button>;
        }}</For>
      </div>
      <button class="add-tab" aria-label="Add an app" onClick={() => { setChooser(true); setSearch(""); setSelected(0); }}>+</button>
      <div class="chrome-space" data-tauri-drag-region />
      <div class="window-controls">
        <button aria-label="Minimize" onClick={() => void windowAction("minimize")}><Icon name="min" /></button>
        <button aria-label="Maximize" onClick={() => void windowAction("maximize")}><Icon name="max" /></button>
        <button class="close-window" aria-label="Close" onClick={() => void windowAction("close")}><Icon name="close" /></button>
      </div>
    </header>
    <main class="card">
      <Show when={settingsOpen()}><section class="settings-view">
        <div class="view-heading"><div><span class="eyebrow">AUTARK</span><h1>Settings</h1></div></div>
        <section class="setting-row"><div><h2>Root folder</h2><p>Apps and managed tools live here.</p><button class="path" onClick={() => reveal("root")}>{state().settings.root}</button></div><button class="button" disabled={state().busy} onClick={() => void changeRoot()}>Change</button></section>
        <section class="setting-row"><div><h2>Settings file</h2><p>Portable launcher preferences.</p><button class="path" onClick={() => reveal("settings")}>{state().settings_path}</button></div></section>
        <section class="setting-row"><div><h2>Marketplace</h2><p>{state().marketplace.exists ? <button class="path" onClick={() => reveal("marketplace")}>{state().marketplace.path}</button> : "Marketplace has not been loaded."}</p><Show when={state().marketplace.last_refresh}><small>Last refreshed {new Date(state().marketplace.last_refresh! * 1000).toLocaleString()}</small></Show></div><button class="button" disabled={state().busy} onClick={refresh}>{state().marketplace.exists ? "Reload marketplace" : "Load marketplace"}</button></section>
        <section class="setting-row"><div><h2>Updates</h2><p>Check marketplace versions manually.</p></div><button class="button" disabled={state().busy} onClick={() => mockMode() ? undefined : void run(() => api.checkUpdates())}>Check for updates</button></section>
        <section class="updates"><h2>Update all</h2><p>Running apps stop during updates and cannot be used until each update finishes.</p>
          <div class="update-table"><div class="update-head"><span>App</span><span>App version</span><span>Recipe</span><span>Latest recipe</span><span>State</span></div>
            <For each={state().apps.filter(a => a.installed)}>{a => <div class="update-row"><span>{a.name}</span><span>{a.version_app ?? "unknown"}</span><span>{a.version_installed ?? "not installed"}</span><span>{a.version_latest ?? "unknown"}</span><span>{a.phase}</span></div>}</For>
          </div>
          <Show when={!allConfirm()}><button class="button primary" disabled={state().busy || !state().apps.some(a => a.installed && a.update_available)} onClick={() => setAllConfirm(true)}>Update all</button></Show>
          <Show when={allConfirm()}><div class="confirm-strip"><span>Update every installed app now?</span><button class="button" onClick={() => setAllConfirm(false)}>Cancel</button><button class="button primary" disabled={state().busy} onClick={doUpdateAll}>Confirm</button></div></Show>
        </section>
        <footer class="about"><span>Autark <b>v{version()}</b></span><span>AGPL-3.0</span><button onClick={() => openUrl("https://github.com/verzly/autark")}>Source</button><button onClick={() => openUrl("https://github.com/verzly/autark-marketplace")}>Marketplace</button></footer>
      </section></Show>
      <Show when={!settingsOpen() && !currentApp()}><section class="empty-state"><div class="empty-mark">autark</div><p>Apps you install live in tabs here.</p><button class="button primary" onClick={() => setChooser(true)}>Add an app</button></section></Show>
      <Show when={!settingsOpen() && currentApp()}>{app => {
        const a = () => app();
        const p = () => a().phase;
        const busyApp = () => appBusy(a());
        const elapsed = () => a().elapsed_secs ?? (a().started_at ? Math.max(0, clock() - a().started_at!) : null);
        const canStart = () => !busyApp() && a().installed && p() === "installed";
        const canStop = () => !busyApp() && (p() === "running" || p() === "starting");
        const confirmInstall = () => { setPanel(null); action(a().installed ? "update" : "install", a()); };
        return <section class="app-view">
          <div class="app-header"><div class="app-title"><span class="badge large" style={{ "--hue": hue(a().id) }}>{a().id[0]?.toUpperCase()}</span><div><h1>{a().name}</h1><div class="version-line">{a().version_app ? `app ${a().version_app}` : "app version unknown"}{a().version_installed ? ` · recipe ${a().version_installed}` : ""}<Show when={a().update_available}> · latest recipe {a().version_latest} <span class="update-pill">update</span></Show></div></div></div>
            <div class="links"><Show when={a().homepage}><button onClick={() => openUrl(a().homepage!)}>Homepage ↗</button></Show><Show when={a().license}><span>{a().license}</span></Show></div></div>
          <div class="status-line"><span class={`dot ${p()}`} /><Show when={working(p())}><span class="progress"><i /></span></Show><span class="phase-label">{a().step ? `(${a().step?.index}/${a().step?.total}) ${a().step?.name}` : p().replace("_", " ")}</span><Show when={working(p()) && elapsed() != null}><span class="elapsed">run {duration(elapsed()!)}</span></Show></div>
          <Show when={a().last_error}><div class="error-banner">{a().last_error?.message}</div></Show>
          <Show when={requestError()}><div class="error-banner">{requestError()}</div></Show>
          <div class="actions">
            <button class="button primary" disabled={busyApp() || a().installed} onClick={() => setPanel("install")}>Install</button>
            <button class="button primary" disabled={busyApp() || !a().update_available} onClick={() => setPanel("install")}>Update</button>
            <button class="button" disabled={!canStart()} onClick={() => action("start", a())}>Start</button>
            <button class="button" disabled={!canStop()} onClick={() => action("stop", a())}>Stop</button>
            <button class="button" disabled={busyApp() || !a().url} onClick={() => action("open", a())}>Open</button>
            <button class="button danger-quiet" disabled={busyApp() || !a().installed} onClick={() => { setRemoveText(""); setPanel("remove"); }}>Remove</button>
            <Show when={working(p())}><button class="button" onClick={() => action("cancel", a())}>Cancel operation</button></Show>
          </div>
          <Show when={panel() === "install"}><div class="inline-panel"><p>Setup may install system components through mise and requires network access.</p><div><button class="button" onClick={() => setPanel(null)}>Cancel</button><button class="button primary" disabled={busyApp()} onClick={confirmInstall}>Confirm</button></div></div></Show>
          <Show when={panel() === "remove"}><div class="inline-panel remove-panel"><label>Type REMOVE<input value={removeText()} onInput={e => setRemoveText(e.currentTarget.value)} /></label><div><button class="button" onClick={() => setPanel(null)}>Cancel</button><button class="button danger" disabled={busyApp() || removeText() !== "REMOVE"} onClick={() => { setPanel(null); action("remove", a()); }}>Remove</button></div></div></Show>
          <section class="app-settings"><div class="section-head"><h2>Settings</h2><Show when={dirty(a())}><button class="button primary" disabled={busyApp()} onClick={() => saveDraft(a())}>Save</button></Show></div>
            <Show when={a().settings_schema.length} fallback={<p class="quiet">No app settings.</p>}>
              <div class="fields"><For each={a().settings_schema}>{field => <SettingField app={a()} field={field} value={draftValue(a(), field.key)} setValue={value => setDraftValue(a(), field.key, value)} choose={() => void chooseFolder(a(), field)} />}</For></div>
            </Show>
          </section>
          <section class="output-section"><div class="output-head"><h2>Output</h2><button onClick={() => { setBuffers(b => ({ ...b, [a().id]: [] })); setFollow(true); }}>Clear</button></div>
            <div class="output-wrap"><div class="output" ref={output} onScroll={() => output && setFollow(output.scrollHeight - output.scrollTop - output.clientHeight < 24)}>
              <Show when={logs().length} fallback={<span class="quiet">Operation output will appear here.</span>}><For each={logs()}>{line => <div class={`log ${lineTone(line)}`}>{line || "\u00a0"}</div>}</For></Show>
            </div><Show when={!follow() && logs().length}><button class="jump" onClick={() => { if (output) output.scrollTop = output.scrollHeight; setFollow(true); }}>Jump to latest</button></Show></div>
          </section>
          <footer class="metadata"><button title={state().settings.root + "\\apps\\" + a().id} onClick={() => reveal("app", a().id)}>{`${state().settings.root}\\apps\\${a().id}`}</button><span>{a().version_app ? `app ${a().version_app}` : "app version unknown"}</span><span>recipe {a().version_installed ?? "not installed"}</span><code>{a().id}</code></footer>
        </section>;
      }}</Show>
    </main>
    <Show when={chooser()}><div class="backdrop" onClick={() => setChooser(false)}><section class="dialog" role="dialog" aria-modal="true" aria-labelledby="chooser-title" onClick={e => e.stopPropagation()} onKeyDown={chooserKeys}>
      <div class="dialog-title"><div><span class="eyebrow">LIBRARY</span><h2 id="chooser-title">Add an app</h2></div><button class="icon-button" aria-label="Close chooser" onClick={() => setChooser(false)}><Icon name="close" /></button></div>
      <input ref={dialogInput} class="search" placeholder="Search apps…" value={search()} onInput={e => { setSearch(e.currentTarget.value); setSelected(0); }} />
      <Show when={!state().marketplace.exists} fallback={
        <div class="chooser-list">
          <h3>Installed</h3><For each={available().filter(a => a.installed)}>{(a, i) => <Choice app={a} index={i()} selected={selected() === i()} disabled={busy()} onClick={() => openApp(a.id)} />}</For>
          <div class="group-heading"><h3>Marketplace</h3><button onClick={refresh} disabled={busy() || state().marketplace.refreshing}>Reload marketplace</button></div>
          <Show when={state().marketplace.last_refresh}><small class="refresh-time">Last refreshed {new Date(state().marketplace.last_refresh! * 1000).toLocaleString()}</small></Show>
          <Show when={state().marketplace.error}><p class="market-error">{state().marketplace.error?.message}</p></Show>
          <For each={available().filter(a => !a.installed)}>{(a, i) => {
            const index = () => available().filter(x => x.installed).length + i();
            return <Choice app={a} index={index()} selected={selected() === index()} disabled={busy()} onClick={() => openApp(a.id)} />;
          }}</For>
          <Show when={!available().length}><p class="quiet no-results">No apps match your search.</p></Show>
        </div>
      }><div class="load-market"><p>Load marketplace to browse available apps.</p><button class="button primary" disabled={busy()} onClick={refresh}>Load marketplace</button></div></Show>
    </section></div></Show>
  </div>;
}

function SettingField(props: { app: AppState; field: AppSetting; value: unknown; setValue: (v: unknown) => void; choose: () => void }) {
  return <label class="field"><span>{props.field.label}</span><Show when={props.field.kind === "boolean"} fallback={
    <div class="field-control"><input type={props.field.kind === "number" || props.field.kind === "port" ? "number" : "text"} value={String(props.value ?? props.field.default ?? "")} onInput={e => props.setValue(props.field.kind === "number" || props.field.kind === "port" ? Number(e.currentTarget.value) : e.currentTarget.value)} /><Show when={props.field.kind === "path"}><button class="button" disabled={props.field.key !== "root"} onClick={props.choose}>Choose</button></Show></div>
  }><input type="checkbox" role="switch" checked={Boolean(props.value ?? props.field.default)} onChange={e => props.setValue(e.currentTarget.checked)} /></Show><Show when={props.field.help}><small>{props.field.help}</small></Show></label>;
}
function Choice(props: { app: AppState; index: number; selected: boolean; disabled: boolean; onClick: () => void }) {
  return <button class={`choice ${props.selected ? "choice-selected" : ""}`} disabled={props.disabled || working(props.app.phase)} aria-selected={props.selected} onMouseEnter={() => {}} onClick={props.onClick}><span class="badge" style={{ "--hue": hue(props.app.id) }}>{props.app.id[0]?.toUpperCase()}</span><span class="choice-copy"><b>{props.app.name}</b><small>{props.app.description}</small><small class="choice-version">{props.app.version_latest ? `v${props.app.version_latest}` : "version unavailable"}</small></span><span class="chevron">›</span></button>;
}
function lineTone(line: string) {
  return /error|failed|fatal|stopped/i.test(line) ? "tone-error" : /warning|warn/i.test(line) ? "tone-warning" : /installed|complete|ready/i.test(line) ? "tone-success" : /downloading|extracting|^\s*\d+%/i.test(line) ? "tone-dim" : "";
}
function Icon(props: { name: "gear" | "close" | "min" | "max" }) {
  return <svg viewBox="0 0 16 16" aria-hidden="true">{props.name === "gear" ? <><circle cx="8" cy="8" r="2.5" /><path d="M8 1.5v1.2m0 10.6v1.2M1.5 8h1.2m10.6 0h1.2M3.4 3.4l.9.9m7.4 7.4.9.9m0-9.2-.9.9m-7.4 7.4-.9.9" /></> : props.name === "close" ? <path d="m4 4 8 8m0-8-8 8" /> : props.name === "min" ? <path d="M3 8h10" /> : <rect x="3.2" y="3.2" width="9.6" height="9.6" rx="1" />}</svg>;
}
