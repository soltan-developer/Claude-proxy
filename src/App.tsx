import { useCallback, useEffect, useLayoutEffect, useMemo, useRef, useState, type Dispatch, type ReactNode, type SetStateAction } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import packageMetadata from "../package.json";
import licenseText from "../LICENSE.txt?raw";
import { DiagnosticsPage } from "./DiagnosticsPage";
import { UsagePage } from "./UsagePage";
import { ThemeSelect } from "./ThemeSelect";
import { ProxyStatusControl } from "./ProxyStatusControl";
import { localeFor, observeLocalizedContent, translateText, type Language } from "./localization";
import proxyActiveIcon from "./assets/proxy-active.png";
import proxyOfflineIcon from "./assets/proxy-offline.png";
import {
  applyClaudeCli,
  applyClaudeCodeWebMcp,
  applyClaudeDesktop,
  fetchModels,
  getProxyStatus,
  isTauriRuntime,
  loadConfig,
  openCreatorLink,
  pingProvider,
  quitApplication,
  saveConfig,
  startProxy,
  stopProxy,
  testWebAccess,
  type WebProbeResult,
} from "./lib/tauri";
import { emptyConfig, routeRoles, type BridgeConfig, type ModelRoute, type ProxyStatus } from "./types";

type Page = "overview" | "usage" | "connection" | "models" | "clients" | "api-pool" | "advanced" | "diagnostics";
type Draft = BridgeConfig;
type ToastTone = "success" | "error";

const pageTitles: Record<Page, string> = {
  overview: "Overview",
  usage: "Token usage",
  connection: "Provider connection",
  models: "Model routes",
  clients: "Claude clients",
  "api-pool": "API pool",
  advanced: "Advanced settings",
  diagnostics: "Diagnostics",
};

function App() {
  const [language, setLanguage] = useState<Language>(() => window.localStorage.getItem("claude-proxy-language") === "fa" ? "fa" : "en");
  const localeRoot = useRef<HTMLDivElement>(null);
  const [config, setConfig] = useState<BridgeConfig>(emptyConfig);
  const [draft, setDraft] = useState<Draft>(emptyConfig);
  const [secret, setSecret] = useState("");
  const [clearSecret, setClearSecret] = useState(false);
  const [status, setStatus] = useState<ProxyStatus>({ running: false, host: "127.0.0.1", port: 18181, configured: false, mappedRoutes: 0 });
  const [page, setPage] = useState<Page>("overview");
  const [wizard, setWizard] = useState(false);
  const [wizardStep, setWizardStep] = useState(0);
  const [loading, setLoading] = useState(true);
  const [busy, setBusy] = useState("");
  const [connectionFeedback, setConnectionFeedback] = useState<{ ok: boolean; message: string } | null>(null);
  const [error, setError] = useState("");
  const [toast, setToast] = useState("");
  const [toastTone, setToastTone] = useState<ToastTone>("success");
  const [newRole, setNewRole] = useState<string>("");
  const [newModel, setNewModel] = useState("");
  const [privacyOpen, setPrivacyOpen] = useState(false);
  const [closeConfirm, setCloseConfirm] = useState(false);

  function showToast(message: string, tone: ToastTone = "success") {
    setToastTone(tone);
    setToast(message);
  }

  function dismissToast() {
    setToast("");
  }

  const refreshStatus = useCallback(async () => {
    try { setStatus(await getProxyStatus()); } catch { /* preview mode */ }
  }, []);

  useEffect(() => {
    let alive = true;
    Promise.all([loadConfig(), getProxyStatus()]).then(([loaded, proxy]) => {
      if (!alive) return;
      setConfig(loaded);
      setDraft(loaded);
      setStatus(proxy);
      setWizard(!loaded.setupComplete);
    }).catch((reason: unknown) => setError(messageOf(reason))).finally(() => { if (alive) setLoading(false); });
    return () => { alive = false; };
  }, []);

  useEffect(() => {
    const timer = window.setInterval(() => {
      void getProxyStatus().then(setStatus).catch(() => { /* preview mode */ });
    }, 2000);
    return () => window.clearInterval(timer);
  }, []);

  useEffect(() => {
    document.documentElement.dataset.theme = draft.theme;
  }, [draft.theme]);

  useLayoutEffect(() => {
    document.documentElement.lang = localeFor(language);
    document.documentElement.dir = language === "fa" ? "rtl" : "ltr";
    window.localStorage.setItem("claude-proxy-language", language);
  }, [language]);

  useLayoutEffect(() => {
    const root = localeRoot.current;
    return root ? observeLocalizedContent(root, language) : undefined;
  }, [language, loading]);

  useEffect(() => {
    const allowNativeContextMenu = (target: EventTarget | null) => target instanceof Element && Boolean(target.closest("input, textarea, [contenteditable='true'], [data-context-menu='allow']"));
    const suppressContextMenu = (event: MouseEvent) => {
      if (!allowNativeContextMenu(event.target)) event.preventDefault();
    };
    document.addEventListener("contextmenu", suppressContextMenu);
    return () => document.removeEventListener("contextmenu", suppressContextMenu);
  }, []);

  useEffect(() => {
    if (!isTauriRuntime()) return;
    let mounted = true;
    let unlisten: (() => void) | undefined;
    void getCurrentWindow().onCloseRequested((event) => {
      event.preventDefault();
      setCloseConfirm(true);
    }).then((dispose) => {
      if (mounted) unlisten = dispose;
      else dispose();
    }).catch(() => { /* Browser preview has no native window event. */ });
    return () => { mounted = false; unlisten?.(); };
  }, []);

  useEffect(() => {
    if (!toast) return;
    const timer = window.setTimeout(dismissToast, 3800);
    return () => window.clearTimeout(timer);
  }, [toast]);

  const ready = useMemo(() => Boolean(draft.baseUrl.trim() && (secret.trim() || (config.apiKeyConfigured && !clearSecret)) && draft.routes.length), [draft.baseUrl, draft.routes.length, secret, config.apiKeyConfigured, clearSecret]);
  const localUrl = `http://127.0.0.1:${draft.proxyPort || 18181}`;
  const persianLicenseStart = licenseText.indexOf("مجوز غیرتجاری Salmanzadeh");
  const licenseTextForLanguage = language === "fa" && persianLicenseStart >= 0
    ? licenseText.slice(persianLicenseStart)
    : licenseText.slice(0, licenseText.indexOf("\nترجمهٔ فارسی") >= 0 ? licenseText.indexOf("\nترجمهٔ فارسی") : undefined);

  async function persist(next: Draft = draft, setupComplete = next.setupComplete) {
    const saved = await saveConfig({
      ...next,
      setupComplete,
      apiKey: secret.trim() || undefined,
      clearApiKey: clearSecret,
    });
    setConfig(saved);
    setDraft(saved);
    setSecret("");
    setClearSecret(false);
    await refreshStatus();
    return saved;
  }

  async function act(label: string, operation: () => Promise<unknown>, success?: (result: unknown) => string) {
    setBusy(label);
    setError("");
    try {
      const result = await operation();
      await refreshStatus();
      if (success) showToast(success(result));
      return result;
    } catch (reason) {
      setError(messageOf(reason));
      return undefined;
    } finally { setBusy(""); }
  }

  async function saveCurrent(setupComplete = draft.setupComplete) {
    const result = await act("save", async () => {
      const saved = await persist(draft, setupComplete);
      return saved;
    }, () => "Settings saved.");
    return result as BridgeConfig | undefined;
  }

  async function handleTheme() {
    const nextTheme = draft.theme === "light" ? "dark" : "light";
    setDraft((current) => ({ ...current, theme: nextTheme }));
    try {
      const saved = await saveConfig({ ...config, theme: nextTheme });
      setConfig(saved);
      setDraft((current) => ({ ...current, theme: nextTheme }));
    } catch (reason) { setError(messageOf(reason)); }
  }

  async function handleProxyToggle() {
    if (!status.running) {
      if (!status.configured) {
        setError("Add a provider URL, API key, and at least one model route before starting the proxy.");
        setPage("connection");
        return;
      }
      await act("proxy", startProxy, () => "Local proxy started.");
    } else await act("proxy", stopProxy, () => "Proxy stopped.");
  }

  async function saveAndApply(kind: "desktop" | "cli") {
    setBusy(kind);
    setError("");
    try {
      await persist(draft, true);
      const message = kind === "desktop" ? await applyClaudeDesktop() : await applyClaudeCli();
      setWizard(false);
      await refreshStatus();
      showToast(message);
    } catch (reason) { setError(messageOf(reason)); }
    finally { setBusy(""); }
  }

  async function handleFetchModels() {
    setBusy("fetch");
    setError("");
    setConnectionFeedback(null);
    try {
      await persist(draft, draft.setupComplete);
      const models = await fetchModels();
      const updated = await loadConfig();
      setConfig(updated);
      setDraft(updated);
      setConnectionFeedback({
        ok: true,
        message: language === "fa"
          ? `اتصال API برقرار است و ${models.length} مدل دریافت شد.`
          : `Provider API connected. Retrieved ${models.length} models.`,
      });
    } catch (reason) {
      const message = messageOf(reason);
      setConnectionFeedback({ ok: false, message });
      if (wizard) setError(message);
    } finally {
      setBusy("");
    }
  }

  async function handlePingProvider() {
    setBusy("ping");
    setError("");
    try {
      await persist(draft, draft.setupComplete);
      showToast(await pingProvider());
    } catch (reason) { setError(messageOf(reason)); }
    finally { setBusy(""); }
  }

  async function minimizeWindow() {
    if (isTauriRuntime()) await getCurrentWindow().minimize();
  }

  async function maximizeWindow() {
    if (isTauriRuntime()) await getCurrentWindow().toggleMaximize();
  }

  async function keepInTray() {
    setCloseConfirm(false);
    if (isTauriRuntime()) await getCurrentWindow().hide();
  }

  async function quitCompletely() {
    if (isTauriRuntime()) await quitApplication();
    else setCloseConfirm(false);
  }

  function addRoute() {
    const role = routeRoles.find((item) => item.id === newRole);
    const model = newModel.trim();
    if (!role || !model) { setError("Choose a Claude role and enter its upstream model ID."); return; }
    if (draft.routes.some((route) => route.routeId === role.id)) { setError("This Claude role already has a route."); return; }
    const suggestedContext = model === "qwen38" ? 128_000 : model === "qwen38-sglang" ? 200_000 : 0;
    const route: ModelRoute = {
      routeId: role.id,
      displayName: `${role.title} · ${model}`,
      upstreamModel: model,
      tier: role.tier,
      thinkingMode: model === "qwen38" || model === "qwen38-sglang" ? "disabled" : "inherit",
      contextWindow: suggestedContext,
      maxOutputTokens: 0,
      isDefault: draft.routes.length === 0,
    };
    setDraft((current) => ({ ...current, routes: [...current.routes, route] }));
    setNewRole("");
    setNewModel("");
    setError("");
    showToast(`${role.title} route added. Save your settings to apply the route.`);
  }

  function updateRoute(index: number, patch: Partial<ModelRoute>) {
    setDraft((current) => ({ ...current, routes: current.routes.map((route, i) => i === index ? { ...route, ...patch } : route) }));
  }

  function removeRoute(index: number) {
    setDraft((current) => {
      const routes = current.routes.filter((_, i) => i !== index);
      if (routes.length && !routes.some((route) => route.isDefault)) routes[0] = { ...routes[0], isDefault: true };
      return { ...current, routes };
    });
  }

  async function copy(value: string) {
    try { await navigator.clipboard.writeText(value); showToast("Address copied."); }
    catch { showToast("Clipboard access is unavailable.", "error"); }
  }

  function skipWizard() {
    setWizard(false);
    setPage("overview");
    void saveCurrent(true);
  }

  async function wizardNext() {
    if (wizardStep === 0) {
      const saved = await saveCurrent(false);
      if (saved) setWizardStep(1);
    } else if (wizardStep === 1) setWizardStep(2);
    else if (wizardStep === 2) setWizardStep(3);
    else {
      const saved = await saveCurrent(true);
      if (saved) { setWizard(false); setPage("overview"); }
    }
  }

  if (loading) return <div ref={localeRoot} className="loading-screen"><img className="brand-mark brand-image" src={proxyOfflineIcon} alt="" /><span>Loading Claude Proxy…</span></div>;

  return (
    <div ref={localeRoot} className="app-shell" dir={language === "fa" ? "rtl" : "ltr"} data-design="japandi-calm" data-theme={draft.theme}>
      <aside className="sidebar">
        <div className="brand-lockup"><img className="brand-mark brand-image" src={status.running ? proxyActiveIcon : proxyOfflineIcon} alt="" /><span><strong>Claude Proxy</strong><small>Local gateway for Claude</small></span></div>
        <ProxyStatusControl status={status} language={language} busy={busy === "proxy"} onToggle={() => void handleProxyToggle()} />
        <nav className="nav-list" aria-label="Application sections">
          {(["overview", "connection", "models", "clients", "usage", "api-pool", "advanced", "diagnostics"] as Page[]).map((item) => (
            <button key={item} className={`nav-item ${page === item ? "nav-active" : ""}`} onClick={() => { setPage(item); setWizard(false); }}>
              <span className="nav-icon" aria-hidden="true">{{ overview: "⌂", usage: "▥", connection: "⌁", models: "◈", clients: "↗", "api-pool": "◎", advanced: "◇", diagnostics: "≋" }[item]}</span><span>{pageTitles[item]}</span>
            </button>
          ))}
        </nav>
        <div className="sidebar-bottom">
          <button className="side-hint privacy-trigger" type="button" onClick={() => setPrivacyOpen(true)} aria-label="Privacy & usage rights" title="Privacy & usage rights"><span className="small-orb">i</span><span>Privacy &amp; usage rights</span></button>
          <div className="sidebar-version"><span>{language === "fa" ? "نسخه" : "Version"}</span><strong>v{packageMetadata.version}</strong></div>
          <button className="developer-link" onClick={() => void openCreatorLink()}>Created by Salmanzadeh <span>↗</span></button>
        </div>
      </aside>

      <main className="main-area">
        <header className="topbar" dir={language === "fa" ? "rtl" : "ltr"} data-tauri-drag-region onMouseDown={(event) => {
          const target = event.target;
          if (event.button !== 0 || (target instanceof Element && target.closest("button, input, select, a"))) return;
          if (isTauriRuntime()) void getCurrentWindow().startDragging().catch(() => {});
        }}>
          <div className="top-title" data-tauri-drag-region><span className="crumb" data-tauri-drag-region>CLAUDE PROXY</span><span className="crumb-sep" data-tauri-drag-region>/</span><strong data-tauri-drag-region>{wizard ? "Setup" : pageTitles[page]}</strong><span className="version-tag" data-tauri-drag-region>v{packageMetadata.version}</span></div>
          <div className="top-actions" dir="ltr">
            {(() => {
              const windowControls = (language === "fa" ? ["close", "maximize", "minimize"] : ["minimize", "maximize", "close"]).map((kind) => <WindowControl key={kind} kind={kind as WindowControlKind} onClose={() => setCloseConfirm(true)} onMaximize={() => void maximizeWindow()} onMinimize={() => void minimizeWindow()} />);
              const languageControls = <>
                <button className="language-button" aria-label={language === "en" ? "Switch to Persian" : "Switch to English"} title={language === "en" ? "Switch to Persian" : "Switch to English"} onClick={() => setLanguage((current) => current === "en" ? "fa" : "en")}><span aria-hidden="true">{language === "en" ? "فا" : "EN"}</span></button>
                <button className="icon-button" aria-label="Toggle light or dark mode" title="Toggle light or dark mode" onClick={() => void handleTheme()}><span className={`theme-glyph theme-glyph-${draft.theme}`} aria-hidden="true"><span className="theme-symbol">{draft.theme === "light" ? "☼" : "☾"}</span></span></button>
              </>;
              return language === "en"
                ? <>{languageControls}<span className="titlebar-divider" aria-hidden="true" />{windowControls}</>
                : <>{windowControls}<span className="titlebar-divider" aria-hidden="true" />{languageControls}</>;
            })()}
          </div>
        </header>

        <section className="page-content">
          {error && <div className="alert alert-error app-error-notice" role="alert"><span>!</span><p>{error}</p><button onClick={() => setError("")} aria-label="Dismiss">×</button></div>}
          <div key={wizard ? "setup" : page} className="page-view">
          {wizard ? (
            <WizardView
              step={wizardStep} setStep={setWizardStep} draft={draft} setDraft={setDraft} secret={secret} setSecret={setSecret}
              newRole={newRole} setNewRole={setNewRole} newModel={newModel} setNewModel={setNewModel}
              addRoute={addRoute} models={draft.upstreamModels} busy={busy} isTauri={isTauriRuntime()} onFetch={() => void handleFetchModels()}
              onNext={() => void wizardNext()} onSkip={skipWizard} onGoManual={() => { setWizard(false); setPage("connection"); }} routeCount={draft.routes.length}
            />
          ) : page === "overview" ? (
            <Overview config={config} draft={draft} status={status} localUrl={localUrl} ready={ready} onCopy={() => void copy(localUrl)} onGo={(target) => setPage(target)} onWizard={() => { setWizard(true); setWizardStep(0); }} />
          ) : page === "connection" ? (
            <ConnectionPage language={language} draft={draft} setDraft={setDraft} secret={secret} setSecret={setSecret} clearSecret={clearSecret} setClearSecret={setClearSecret} busy={busy} isTauri={isTauriRuntime()} onSave={() => void saveCurrent()} onFetch={() => void handleFetchModels()} feedback={connectionFeedback} />
          ) : page === "models" ? (
            <ModelsPage draft={draft} setDraft={setDraft} newRole={newRole} setNewRole={setNewRole} newModel={newModel} setNewModel={setNewModel} addRoute={addRoute} updateRoute={updateRoute} removeRoute={removeRoute} onSave={() => void saveCurrent()} onFetch={() => void handleFetchModels()} onPing={() => void handlePingProvider()} busy={busy} />
          ) : page === "clients" ? (
            <ClientsPage config={draft} localUrl={localUrl} status={status} busy={busy} onCopy={() => void copy(localUrl)} onDesktop={() => void saveAndApply("desktop")} onCli={() => void saveAndApply("cli")} onMcp={() => void act("mcp", applyClaudeCodeWebMcp, (result) => String(result))} isTauri={isTauriRuntime()} />
          ) : page === "usage" ? (
            <UsagePage isTauri={isTauriRuntime()} language={language} config={draft} />
          ) : page === "api-pool" ? (
            <ApiPoolPage />
          ) : page === "advanced" ? (
            <AdvancedPage language={language} draft={draft} setDraft={setDraft} busy={busy} isTauri={isTauriRuntime()} onSave={() => void saveCurrent()} onApplyOutputLimit={() => void saveAndApply("desktop")} onTest={async () => { await persist(draft, draft.setupComplete); return testWebAccess(); }} />
          ) : (
            <DiagnosticsPage isTauri={isTauriRuntime()} language={language} />
          )}
          </div>
        </section>
      </main>
      {toast && <div className={`toast toast-${toastTone}`} role={toastTone === "error" ? "alert" : "status"}><span className="toast-check" aria-hidden="true">{toastTone === "error" ? "!" : "✓"}</span><span>{toast}</span><button onClick={dismissToast} aria-label="Dismiss">×</button></div>}
      {privacyOpen && <Modal title="Privacy & usage rights" onClose={() => setPrivacyOpen(false)}>
        <h3>Privacy notice</h3>
        <p>Your API credentials and configuration are stored on this computer. Saved provider keys are protected with Windows account encryption. Usage history and diagnostic logs also remain local. When you send a request, the request and provider key are sent to the provider you configured so it can generate a response; they are not sent to the app developer. The local gateway token is written to Claude settings only when you choose to apply them. Prompts, completions, and provider response bodies are not written to diagnostic logs.</p>
        <h3>Purpose and usage rights</h3>
        <p>This software was created for Shia researchers, scholars, and developers seeking to advance the interests of Islam and the Islamic Revolution. It is free to use for personal, academic, educational, research, and other non-commercial purposes. Commercial use, paid distribution, or inclusion in a revenue-generating service requires prior written permission from the developer. Third-party components remain subject to their own licenses.</p>
        <p>This is a custom non-commercial license, not an OSI-approved open-source license. Copyright and all rights not expressly granted remain with the developer.</p>
        <details className="license-details"><summary>Read the full non-commercial license</summary><pre className="license-copy" dir={language === "fa" ? "rtl" : "ltr"}>{licenseTextForLanguage}</pre></details>
        <button className="button button-primary" onClick={() => setPrivacyOpen(false)}>Got it</button>
      </Modal>}
      {closeConfirm && <Modal title="Close Claude Proxy?" onClose={() => setCloseConfirm(false)}><p>Would you like to keep Claude Proxy running in the system tray, or quit the app completely?</p><div className="modal-actions"><button className="button button-secondary" onClick={() => setCloseConfirm(false)}>Cancel</button><button className="button button-secondary" onClick={() => void keepInTray()}>Keep running in the system tray</button><button className="button button-danger" onClick={() => void quitCompletely()}>Quit completely</button></div></Modal>}
    </div>
  );
}

type WindowControlKind = "close" | "maximize" | "minimize";

function WindowControl({ kind, onClose, onMaximize, onMinimize }: {
  kind: WindowControlKind;
  onClose: () => void;
  onMaximize: () => void;
  onMinimize: () => void;
}) {
  const meta: Record<WindowControlKind, { label: string; className: string; iconClass: string; onClick: () => void }> = {
    close: { label: "Close window", className: "window-control-close", iconClass: "window-icon-close", onClick: onClose },
    maximize: { label: "Maximize or restore", className: "", iconClass: "window-icon-maximize", onClick: onMaximize },
    minimize: { label: "Minimize", className: "", iconClass: "window-icon-minimize", onClick: onMinimize },
  };
  const control = meta[kind];
  return <button className={`window-control ${control.className}`} aria-label={control.label} title={control.label} onClick={control.onClick}><span className={`window-icon ${control.iconClass}`} aria-hidden="true" /></button>;
}

function Modal({ title, onClose, children }: { title: string; onClose: () => void; children: ReactNode }) {
  return <div className="modal-backdrop" role="presentation" onMouseDown={(event) => { if (event.target === event.currentTarget) onClose(); }}><section className="app-modal card" role="dialog" aria-modal="true" aria-labelledby="modal-title"><button className="modal-close" type="button" aria-label="Close dialog" onClick={onClose}>×</button><span className="eyebrow">CLAUDE PROXY</span><h2 id="modal-title">{title}</h2><div className="modal-copy">{children}</div></section></div>;
}

function AdvancedPage({ language, draft, setDraft, busy, isTauri, onSave, onApplyOutputLimit, onTest }: {
  language: Language;
  draft: Draft;
  setDraft: Dispatch<SetStateAction<Draft>>;
  busy: string;
  isTauri: boolean;
  onSave: () => void;
  onApplyOutputLimit: () => void;
  onTest: () => Promise<WebProbeResult[]>;
}) {
  const [webProbeResults, setWebProbeResults] = useState<WebProbeResult[] | null>(null);
  const [webProbeBusy, setWebProbeBusy] = useState(false);
  const [webProbeError, setWebProbeError] = useState("");
  async function runWebChecks() {
    setWebProbeBusy(true);
    setWebProbeResults(null);
    setWebProbeError("");
    try {
      setWebProbeResults(await onTest());
    } catch (error) {
      setWebProbeError(error instanceof Error ? error.message : String(error));
    } finally {
      setWebProbeBusy(false);
    }
  }
  const webEnabled = draft.webAccessMode !== "disabled";
  const manualProxy = draft.webProxyMode === "manual";
  const routeSummary = manualProxy
    ? draft.webProxyUrl.trim()
      ? (language === "fa" ? `ترافیک از ${draft.webProxyUrl.trim()} عبور می‌کند.` : `Traffic will use ${draft.webProxyUrl.trim()}.`)
      : translateText("Traffic will use the configured proxy.", language)
    : translateText("Traffic will use a direct connection.", language);
  return <div className="advanced-page">
    <div className="page-heading compact-heading">
      <div><span className="eyebrow">ADVANCED SETTINGS</span><h1>Web access bridge</h1><p>Let Claude use a controlled web tool that runs on this computer, while the model API stays on your configured provider.</p></div>
    </div>
    <div className="advanced-grid">
      <section className="card form-card web-bridge-card">
        <div className="section-title"><span className="title-icon" aria-hidden="true"><svg viewBox="0 0 18 18" focusable="false"><circle cx="9" cy="9" r="6.5" /><path d="M2.8 9h12.4M9 2.5c1.7 1.8 2.6 4 2.6 6.5S10.7 13.7 9 15.5C7.3 13.7 6.4 11.5 6.4 9S7.3 4.3 9 2.5Z" /></svg></span><div><h2>Web access</h2><p>The bridge only runs when you enable it here.</p></div></div>
        <div className="form-grid web-form-grid">
          <label className="field span-two"><span>Web access mode</span><ThemeSelect value={draft.webAccessMode} options={[{ value: "disabled", label: "Disabled" }, { value: "auto", label: "Automatic — model decides" }, { value: "required", label: "Required — search first" }]} onChange={(value) => setDraft((current) => ({ ...current, webAccessMode: value as BridgeConfig["webAccessMode"] }))} /></label>
          <label className="field"><span>Web traffic route</span><ThemeSelect value={draft.webProxyMode} options={[{ value: "direct", label: "Direct connection" }, { value: "manual", label: "Manual proxy" }]} onChange={(value) => setDraft((current) => ({ ...current, webProxyMode: value as BridgeConfig["webProxyMode"] }))} /></label>
          <label className="field"><span>Web proxy address <small>HTTP / SOCKS5</small></span><input dir="ltr" value={draft.webProxyUrl} disabled={!manualProxy} onChange={(event) => setDraft((current) => ({ ...current, webProxyUrl: event.target.value }))} placeholder="127.0.0.1:10808" autoCapitalize="off" spellCheck={false} /><small className="field-hint">For example: 127.0.0.1:10808</small></label>
          <label className="field span-two"><span>Web search endpoint</span><input dir="ltr" value={draft.webSearchUrl} disabled={!webEnabled} onChange={(event) => setDraft((current) => ({ ...current, webSearchUrl: event.target.value }))} placeholder="https://html.duckduckgo.com/html/" autoCapitalize="off" spellCheck={false} /><small className="field-hint">An HTTPS search endpoint returning DuckDuckGo HTML results. Keep the default unless your network requires another endpoint.</small></label>
          <div className="field span-two">
            <label className="toggle-row"><input type="checkbox" checked={draft.clientWebGroundingEnabled} disabled={!webEnabled} onChange={(event) => setDraft((current) => ({ ...current, clientWebGroundingEnabled: event.target.checked }))} /><span>Ground explicit web requests for API-based Cowork/Code sessions (opt-in)</span></label>
            <small className="field-hint">When a client supplies its own tools, source data from allowed public sites can be added to Qwen's context without replacing client tools. This does not enable or override blocked native Cowork web tools. Only enable where your account/device policies allow those destinations.</small>
          </div>
          <label className="field span-two"><span>Approved hostnames for API web grounding (required)</span><textarea dir="ltr" rows={3} disabled={!webEnabled || !draft.clientWebGroundingEnabled} value={draft.clientWebAllowedDomains.join("\n")} onChange={(event) => setDraft((current) => ({ ...current, clientWebAllowedDomains: event.target.value.split(/[\r\n]+/).map((v) => v.trim()).filter(Boolean) }))} placeholder={"docs.example.org\nexample.org"} spellCheck={false} /><small className="field-hint">One exact public hostname per line, with permission to access it. The allowlist applies to redirect hops and search result sources; no automatic authorization of blocked sites. Search queries are sent to your configured engine.</small></label>
          <label className="field span-two"><span>Preferred web domains</span><textarea dir="ltr" value={draft.webPreferredDomains.join("\n")} disabled={!webEnabled} onChange={(event) => setDraft((current) => ({ ...current, webPreferredDomains: event.target.value.split(/[\r\n]+/).map((domain) => domain.trim()).filter(Boolean) }))} placeholder={"github.com\ndocs.example.com"} autoCapitalize="off" spellCheck={false} rows={3} /><small className="field-hint">One hostname per line. These are source preferences for the model, not a network allowlist. A URL you provide is fetched directly without a search first.</small></label>
        </div>
        <div className="web-status-row" aria-live="polite"><span className={`web-status-dot ${webProbeResults?.every((item) => item.ok) ? "web-status-dot-on" : ""}`} aria-hidden="true" /><div><strong>{webEnabled ? (language === "fa" ? "قابلیت وب تنظیم شده؛ نتیجهٔ تست واقعی را ببینید" : "Web configured — verify actual reachability below") : "Web access is disabled"}</strong><small>{routeSummary}</small></div></div>
        <div className="button-row"><button className="button button-primary" disabled={Boolean(busy) || webProbeBusy} onClick={onSave}>{busy === "save" ? "Saving…" : "Save advanced settings"}</button><button className="button button-secondary" disabled={!isTauri || !webEnabled || Boolean(busy) || webProbeBusy} onClick={() => void runWebChecks()}>{webProbeBusy ? (language === "fa" ? "در حال بررسی واقعی…" : "Checking live endpoints…") : (language === "fa" ? "تست واقعی وب و GitHub" : "Test real web and GitHub")}</button></div>
        {webProbeResults && <div className="web-route-list" role="status" aria-live="polite">
          {webProbeResults.map((item) => <div key={item.check}>
            <span className="web-route-number">{item.ok ? "✓" : "×"}</span>
            <div><strong>{item.check === "gateway" ? (language === "fa" ? "درگاه محلی" : "Local gateway") : item.check === "search" ? (language === "fa" ? "موتور جست‌وجو" : "Web search") : (language === "fa" ? "خواندن صفحهٔ GitHub" : "GitHub page fetch")}: {item.ok ? (language === "fa" ? "موفق" : "Passed") : (language === "fa" ? "ناموفق" : "Failed")}</strong><small>{item.detail} · {item.responseMs} ms</small></div>
          </div>)}
        </div>}
        {webProbeError && <p role="alert" className="preview-note">{webProbeError}</p>}
        <p className="preview-note">{language === "fa" ? "این تست فقط اتصال محلی و ابزارهای وب خود Proxy را بررسی می‌کند؛ دسترسی ابزارهای داخلی Cowork و سیاست شبکهٔ سازمان مستقل است." : "These tests check Proxy's own gateway and web tools. Cowork/Code native web permissions remain separate."}</p>
        {!isTauri && <p className="preview-note">Live web checks are available in the installed Windows app.</p>}
      </section>
      <aside className="card web-bridge-info">
        <span className="eyebrow">HOW IT WORKS</span>
        <h2>One setting, two routes</h2>
        <p>Automatic mode does not force web access for ordinary messages. It searches when a GitHub repository is named, reuses fetched context for follow-up opinions, and fetches again when you ask for code changes. Required mode searches each request except a URL is fetched directly. Web requests run through this computer and do not change Cowork's administrator network allowlist.</p>
        <div className="web-route-list"><div><span className="web-route-number">01</span><div><strong>Model API</strong><small>Uses your provider URL and API key.</small></div></div><div><span className="web-route-number">02</span><div><strong>Web bridge</strong><small>Uses Direct or the proxy address above.</small></div></div></div>
        <div className="info-note web-bridge-note"><span>i</span><p>Only public HTTP(S) pages are allowed. Localhost, private IPs, metadata services, and credentials in URLs are blocked before a request is sent.</p></div>
      </aside>
      <section className="card form-card context-advanced-card">
        <div className="section-title"><div><h2>{language === "fa" ? "مدیریت گفت‌وگوهای طولانی" : "Long conversation context"}</h2></div></div>
        <div className="toggle-row"><div><strong>{language === "fa" ? "تنظیم خودکار ظرفیت زمینه" : "Automatically fit long conversations"}</strong>
          <p>{language === "fa" ? "در صورت نیاز، قدیمی‌ترین نوبت‌های کامل از درخواست بعدی کنار گذاشته می‌شوند؛ تاریخچهٔ نمایشی Claude تغییری نمی‌کند." : "When needed, omit the oldest complete turns from future upstream requests without changing Claude's visible history."}</p>
        </div><button role="switch" aria-checked={draft.autoTrimOnContextOverflow} className={`switch ${draft.autoTrimOnContextOverflow ? "switch-on" : ""}`} onClick={() => setDraft((current) => ({ ...current, autoTrimOnContextOverflow: !current.autoTrimOnContextOverflow }))}><span /></button></div>
        <div className="button-row"><button className="button button-secondary" disabled={Boolean(busy)} onClick={onSave}>{language === "fa" ? "ذخیره" : "Save"}</button></div>
      </section>
      <section className="card form-card output-limit-card">
        <div className="section-title"><span className="title-icon output-limit-icon" aria-hidden="true">↥</span><div><h2>Claude output limit</h2><p>Set the maximum output token value for future Claude sessions.</p></div></div>
        <div className="output-limit-layout">
          <label className="field output-limit-field"><span>Maximum output tokens</span><input dir="ltr" type="number" min={1} max={128000} step={1000} value={draft.claudeCodeMaxOutputTokens} onChange={(event) => setDraft((current) => ({ ...current, claudeCodeMaxOutputTokens: Math.min(128000, Math.max(1, Number(event.target.value) || 64000)) }))} /><small className="field-hint">Range: 1–128,000. Current setting: CLAUDE_CODE_MAX_OUTPUT_TOKENS</small></label>
          <div className="info-note output-limit-note"><span>i</span><p>Applying this writes the value to your Windows user environment. Fully quit and reopen Claude Desktop for new sessions to inherit it. This variable is documented for Claude Code; Cowork may not honor it, and provider or model limits still apply.</p></div>
        </div>
        <div className="button-row"><button className="button button-secondary" disabled={Boolean(busy)} onClick={onSave}>{busy === "save" ? "Saving…" : "Save advanced settings"}</button><button className="button button-primary" disabled={!isTauri || Boolean(busy)} onClick={onApplyOutputLimit}>{busy === "desktop" ? "Applying…" : "Save and apply to Claude Desktop"}</button></div>
        {!isTauri && <p className="preview-note">Applying the Windows environment setting is available in the installed desktop app.</p>}
      </section>
    </div>
  </div>;
}

function ApiPoolPage() {
  return <div className="api-pool-page">
    <div className="page-heading compact-heading"><div><span className="eyebrow">ADVANCED ROUTING</span><h1>API pool</h1><p>Bring multiple provider accounts together behind one Claude model route.</p></div><span className="future-badge">Special access · Coming in a future release</span></div>
    <section className="pool-intro card"><span className="pool-icon">◎</span><div><h2>One model name, multiple provider routes</h2><p>The API pool will let you register providers and keys, inspect their available models, then create an internal model backed by several provider models. Requests can be distributed by random selection, ordered failover, weighted routing, or other strategies.</p></div></section>
    <div className="pool-preview-grid">
      <section className="pool-preview-card card" aria-disabled="true"><div className="pool-card-heading"><div><span className="eyebrow">PROVIDERS</span><h2>Provider accounts</h2></div><span className="disabled-chip">Preview only</span></div><div className="mock-provider"><i className="mock-provider-index">01</i><span><strong>OpenAI-compatible gateway</strong><small>2 models · Protected API key</small></span><b>Ready</b></div><div className="mock-provider"><i className="mock-provider-index">02</i><span><strong>Local fallback</strong><small>1 model · Protected API key</small></span><b>Ready</b></div><button className="button button-secondary" disabled>Add provider account</button><p className="pool-card-footnote">Accounts and keys would be managed here in a future release.</p></section>
      <section className="pool-preview-card card" aria-disabled="true"><div className="pool-card-heading"><div><span className="eyebrow">INTERNAL MODEL</span><h2>Route composition</h2></div><span className="disabled-chip">Unavailable</span></div><label className="field"><span>Internal model name</span><input disabled placeholder="For example, balanced-assistant" /></label><ol className="mock-route-list"><li><span>01</span><strong>Qwen3-32B</strong><small>Primary route</small></li><li><span>02</span><strong>Llama-3.3-70B-Instruct</strong><small>Fallback route</small></li><li><span>03</span><strong>DeepSeek-V3</strong><small>Weighted route</small></li><li><span>04</span><strong>Mistral-Large</strong><small>Fallback route</small></li></ol><label className="field"><span>Request strategy</span><ThemeSelect disabled value="ordered" options={[{ value: "ordered", label: "Ordered failover" }, { value: "random", label: "Random" }, { value: "weighted", label: "Weighted" }]} onChange={() => undefined} /></label><button className="button button-primary" disabled>Create internal model</button></section>
    </div>
    <div className="pool-notice"><span>i</span><p>This screen is a visual mockup only. It does not store provider keys, connect to provider APIs, or send requests.</p></div>
  </div>;
}

function Overview({ config, draft, status, localUrl, ready, onCopy, onGo, onWizard }: {
  config: BridgeConfig; draft: Draft; status: ProxyStatus; localUrl: string; ready: boolean; onCopy: () => void; onGo: (page: Page) => void; onWizard: () => void;
}) {
  return <>
    <div className="page-heading"><div><span className="eyebrow">LOCAL AI GATEWAY</span><h1>Claude, with your own models.</h1><p>A simple local route between Claude and your model provider.</p></div><button className="button button-secondary" onClick={onWizard}>Setup guide <span>→</span></button></div>
    {!config.setupComplete && <div className="setup-banner"><div><strong>Setup is not complete yet</strong><span>Add your provider URL, map your models, and connect Claude.</span></div><button className="button button-primary" onClick={onWizard}>Start setup</button></div>}
    <div className="overview-grid">
      <div className="overview-primary card">
        <div className="card-head"><div><span className="eyebrow">PROXY STATUS</span><h2>{status.running ? "Connected" : "Waiting for a connection"}</h2></div><span className={`status-pill ${status.running ? "status-pill-live" : ""}`}><i className={status.running ? "dot dot-live" : "dot"} />{status.running ? "Running" : "Stopped"}</span></div>
        <p className="muted-copy">{status.running ? "Claude requests are routed from the local gateway to your provider." : ready ? "Your settings are ready. Start the proxy or apply them from Claude clients." : "Connect a provider and add at least one model route to get started."}</p>
        <div className="endpoint-box"><span>Local endpoint</span><code dir="ltr">{localUrl}</code><button className="copy-endpoint-button" onClick={onCopy} title="Copy address" aria-label="Copy address"><svg viewBox="0 0 18 18" focusable="false" aria-hidden="true"><rect x="6" y="3" width="8" height="10" rx="1.5" /><path d="M4 6V4.5A1.5 1.5 0 0 1 5.5 3H7" /></svg></button></div>
        <div className="card-foot"><span>Port can be changed in Provider connection</span><button className="text-button" onClick={() => onGo("connection")}>Provider settings <b>→</b></button></div>
      </div>
      <div className="stat-stack">
        <div className="stat-card card"><div className="stat-label-row"><span className="stat-icon">◈</span><span className="stat-label">Model routes</span></div><strong>{draft.routes.length.toLocaleString("en-US")}</strong><button className="text-button" onClick={() => onGo("models")}>Manage routes →</button></div>
        <div className="stat-card card"><div className="stat-label-row"><span className="stat-icon">↗</span><span className="stat-label">Clients</span></div><strong className="stat-word">Desktop and CLI</strong><button className="text-button" onClick={() => onGo("clients")}>Connect a client →</button></div>
      </div>
    </div>
    <div className="section-row"><div><span className="eyebrow">CONFIGURATION</span><h2>Quick overview</h2></div><button className="text-button" onClick={() => onGo("connection")}>Edit settings →</button></div>
    <div className="quick-grid">
      <button className="quick-card card" onClick={() => onGo("connection")}><span className="quick-number">01</span><span><strong>Model provider</strong><small>{draft.baseUrl || "No provider URL added"}</small></span><b>→</b></button>
      <button className="quick-card card" onClick={() => onGo("models")}><span className="quick-number">02</span><span><strong>Default model</strong><small>{draft.routes.find((r) => r.isDefault)?.upstreamModel || "Not selected yet"}</small></span><b>→</b></button>
      <button className="quick-card card" onClick={() => onGo("clients")}><span className="quick-number">03</span><span><strong>Connect Claude</strong><small>Claude Desktop or Claude CLI</small></span><b>→</b></button>
    </div>
  </>;
}

function ConnectionPage({ language, draft, setDraft, secret, setSecret, clearSecret, setClearSecret, busy, isTauri, onSave, onFetch, feedback }: {
  language: Language;
  draft: Draft;
  setDraft: Dispatch<SetStateAction<Draft>>;
  secret: string;
  setSecret: (value: string) => void;
  clearSecret: boolean;
  setClearSecret: (value: boolean) => void;
  busy: string;
  isTauri: boolean;
  onSave: () => void;
  onFetch: () => void;
  feedback: { ok: boolean; message: string } | null;
}) {
  const [showSecret, setShowSecret] = useState(false);
  const fa = language === "fa";
  function toggleClearSecret() {
    if (clearSecret) {
      setClearSecret(false);
      return;
    }
    const message = translateText("Remove the saved provider API key? You will need to enter it again before making provider requests.", language);
    if (window.confirm(message)) {
      setClearSecret(true);
      setSecret("");
    }
  }
  return <div className="provider-settings">
    <div className="page-heading compact-heading">
      <div>
        <span className="eyebrow">PROVIDER CONNECTION</span>
        <h1>{fa ? "اتصال به ارائه‌دهنده" : "Provider connection"}</h1>
        <p>{fa ? "نشانی API و کلید را وارد کنید؛ بررسی اتصال و دریافت مدل‌ها در یک مرحله انجام می‌شود." : "Configure the provider and check connectivity in one step."}</p>
      </div>
    </div>
    <section className="card form-card provider-connection-card">
      <div className="section-title"><div>
        <h2>{fa ? "تنظیمات اصلی" : "Connection settings"}</h2>
        <p>{fa ? "تنظیمات و کلید رمزنگاری‌شده فقط روی دستگاه ذخیره می‌شوند." : "Settings and the encrypted API key stay on this device."}</p>
      </div></div>
      <div className="form-grid">
        <label className="field span-two">
          <span>{fa ? "نام ارائه‌دهنده (اختیاری)" : "Provider name (optional)"}</span>
          <input value={draft.providerName} onChange={(event) => setDraft((current) => ({ ...current, providerName: event.target.value }))} placeholder={fa ? "مثلاً مرکز" : "Example provider"} />
        </label>
        <label className="field span-two">
          <span>{fa ? "نشانی API" : "Base API URL"}</span>
          <input dir="ltr" value={draft.baseUrl} onChange={(event) => setDraft((current) => ({ ...current, baseUrl: event.target.value }))} placeholder="https://gateway.example/api/v1" autoCapitalize="off" spellCheck={false} />
          <small className="field-hint">{fa ? "پسوند /v1 اختیاری است. برنامه آن را فقط در صورت نیاز اضافه می‌کند." : "The /v1 suffix may be omitted."}</small>
        </label>
        <div className="field span-two api-key-field">
          <label htmlFor="provider-api-key"><span>{fa ? "کلید API" : "API key"} {draft.apiKeyConfigured && !clearSecret ? (fa ? "· ذخیره‌شده" : "· protected") : ""}</span></label>
          <div className="secret-row">
            <div className={`secret-control ${draft.apiKeyConfigured && !clearSecret ? "secret-control-saved" : ""}`}>
              <input id="provider-api-key" className="api-key-input" dir="ltr" disabled={draft.apiKeyConfigured && !clearSecret} type={showSecret ? "text" : "password"} value={secret}
                onChange={(event) => { setSecret(event.target.value); setClearSecret(false); }}
                placeholder={draft.apiKeyConfigured && !clearSecret ? (fa ? "کلید به‌صورت امن ذخیره شده است" : "Saved API key is protected") : (fa ? "کلید ارائه‌دهنده" : "Provider API key")} autoComplete="new-password" />
              <button type="button" className="secret-toggle" disabled={draft.apiKeyConfigured && !clearSecret} onClick={() => setShowSecret((value) => !value)}>{showSecret ? (fa ? "پنهان" : "Hide") : (fa ? "نمایش" : "Show")}</button>
            </div>
            {draft.apiKeyConfigured && <button type="button" className="clear-key-icon" onClick={toggleClearSecret} title={fa ? "حذف یا جایگزینی کلید" : "Remove or replace key"} aria-label={fa ? "حذف یا جایگزینی کلید" : "Remove or replace key"}>{clearSecret ? "↶" : "×"}</button>}
          </div>
        </div>
        <label className="field">
          <span>{fa ? "نوع احراز هویت" : "Authentication"}</span>
          <ThemeSelect value={draft.authStyle} options={[{ value: "bearer", label: "Bearer" }, { value: "api_key", label: "api-key" }]} onChange={(value) => setDraft((current) => ({ ...current, authStyle: value as BridgeConfig["authStyle"] }))} />
        </label>
        <label className="field">
          <span>{fa ? "درگاه محلی" : "Local port"}</span>
          <input dir="ltr" type="number" min="1024" max="65535" value={draft.proxyPort} onChange={(event) => setDraft((current) => ({ ...current, proxyPort: Math.min(65535, Math.max(1024, Number(event.target.value) || 18181)) }))} />
        </label>
      </div>
      <details className="provider-network-options">
        <summary>{fa ? "تنظیمات پیشرفتهٔ مسیر اتصال" : "Advanced connection routing"}</summary>
        <label className="field">
          <span>{fa ? "مسیر اتصال API مدل" : "Model API transport"}</span>
          <ThemeSelect value={draft.providerNetworkMode} options={[
            { value: "direct", label: fa ? "مستقیم (پیش‌فرض)" : "Direct (default)" },
            { value: "system", label: fa ? "پراکسی سیستم (با انتخاب شما)" : "System proxy (explicit)" },
          ]} onChange={(value) => setDraft((current) => ({ ...current, providerNetworkMode: value as BridgeConfig["providerNetworkMode"] }))} />
          <small className="field-hint">{fa ? "این گزینه مستقل از پراکسی Web Search است. فقط اگر عمداً می‌خواهید API از پراکسی سیستم عبور کند، حالت دوم را انتخاب کنید." : "Independent from Web Search. System mode uses the operating system proxy only when you explicitly select it."}</small>
        </label>
      </details>
      <div className="provider-check-actions">
        <button className="button button-primary" disabled={!isTauri || Boolean(busy)} onClick={onFetch}>{busy === "fetch" ? (fa ? "در حال بررسی…" : "Checking…") : (fa ? "بررسی اتصال و دریافت مدل‌ها" : "Check connection and load models")}</button>
        <button className="button button-secondary" disabled={Boolean(busy)} onClick={onSave}>{busy === "save" ? (fa ? "در حال ذخیره…" : "Saving…") : (fa ? "ذخیره تنظیمات" : "Save settings")}</button>
      </div>
      {feedback && <div role={feedback.ok ? "status" : "alert"} className={`provider-check-result ${feedback.ok ? "provider-check-ok" : "provider-check-fail"}`}>
        <strong>{feedback.ok ? (fa ? "اتصال برقرار است" : "Connected") : (fa ? "اتصال برقرار نشد" : "Connection failed")}</strong>
        <p dir="auto">{feedback.message}</p>
      </div>}
      {!isTauri && <p className="preview-note">{fa ? "بررسی اتصال فقط در برنامه ویندوز انجام می‌شود." : "Connection checks require the Windows application."}</p>}
    </section>
  </div>;
}

function ModelCombobox({ value, models, onChange, placeholder }: { value: string; models: string[]; onChange: (value: string) => void; placeholder: string }) {
  const [open, setOpen] = useState(false);
  const rootRef = useRef<HTMLDivElement>(null);
  const options = useMemo(() => {
    const query = value.trim().toLocaleLowerCase();
    return [...new Set(models.filter(Boolean))].filter((model) => !query || model.toLocaleLowerCase().includes(query)).slice(0, 12);
  }, [models, value]);

  useEffect(() => {
    function closeOnOutside(event: MouseEvent) {
      if (event.target instanceof Node && !rootRef.current?.contains(event.target)) setOpen(false);
    }
    document.addEventListener("mousedown", closeOnOutside);
    return () => document.removeEventListener("mousedown", closeOnOutside);
  }, []);

  return <div className={`model-combobox${open && options.length > 0 ? " model-combobox-open" : ""}`} ref={rootRef}>
    <input className="model-combobox-input" dir="ltr" role="combobox" aria-expanded={open && options.length > 0} aria-autocomplete="list" value={value} onFocus={() => setOpen(true)} onChange={(event) => { onChange(event.target.value); setOpen(true); }} placeholder={placeholder} autoCapitalize="off" spellCheck={false} />
    <span className="model-combobox-chevron" aria-hidden="true"><svg viewBox="0 0 16 16" focusable="false"><path d="m4 6 4 4 4-4" /></svg></span>
    {open && options.length > 0 && <div className="model-combobox-menu" role="listbox">{options.map((model) => <button key={model} type="button" role="option" className="model-combobox-option" onMouseDown={(event) => event.preventDefault()} onClick={() => { onChange(model); setOpen(false); }}>{model}</button>)}</div>}
  </div>;
}

function ModelsPage({ draft, setDraft, newRole, setNewRole, newModel, setNewModel, addRoute, updateRoute, removeRoute, onSave, onFetch, onPing, busy }: {
  draft: Draft; setDraft: Dispatch<SetStateAction<Draft>>; newRole: string; setNewRole: (value: string) => void; newModel: string; setNewModel: (value: string) => void; addRoute: () => void; updateRoute: (index: number, patch: Partial<ModelRoute>) => void; removeRoute: (index: number) => void; onSave: () => void; onFetch: () => void; onPing: () => void; busy: string;
}) {
  const availableRoles = routeRoles.filter((role) => !draft.routes.some((route) => route.routeId === role.id));
  function modelChanged(index: number, model: string) {
    const context = model === "qwen38" ? 128_000 : model === "qwen38-sglang" ? 200_000 : draft.routes[index].contextWindow;
    updateRoute(index, {
      upstreamModel: model,
      thinkingMode: model === "qwen38" || model === "qwen38-sglang" ? "disabled" : "inherit",
      ...(context ? { contextWindow: context } : {}),
    });
  }
  return <>
    <div className="page-heading compact-heading"><div><span className="eyebrow">MODEL ROUTES</span><h1>Map each Claude role to your model</h1><p>Claude keeps its role names. Map each role to the model ID used by your provider.</p></div><div className="heading-actions"><button className="button button-secondary" disabled={Boolean(busy)} onClick={onPing}>{busy === "ping" ? "Pinging…" : "Ping provider API"}</button><button className="button button-primary" disabled={Boolean(busy)} onClick={onSave}>{busy === "save" ? "Saving…" : "Save model routes"}</button></div></div>
    <div className="mapping-banner"><span className="mapping-glyph">↔</span><p><strong>Claude role → provider model</strong><small>For example, map the Fable role to any model ID available in your gateway.</small></p><button className="button button-secondary" disabled={Boolean(busy)} onClick={onFetch}>{busy === "fetch" ? "Fetching…" : "Fetch model list"}</button></div>
    <div className="mapping-list">
      {draft.routes.map((route, index) => {
        const role = routeRoles.find((item) => item.id === route.routeId);
        return <article className="mapping-card card" key={route.routeId}>
          <div className="mapping-card-head"><div className="model-badge">{(role?.title ?? "C").slice(0, 1)}</div><div className="mapping-name"><strong>{role?.title ?? route.tier}</strong><code dir="ltr">{route.routeId}</code></div><label className="default-check"><input type="radio" name="defaultRoute" checked={route.isDefault} onChange={() => setDraft((c) => ({ ...c, routes: c.routes.map((item, i) => ({ ...item, isDefault: i === index })) }))} /> Default</label><button className="remove-button" onClick={() => removeRoute(index)} aria-label="Remove route">×</button></div>
          <div className="mapping-fields">
            <label className="field"><span>Provider model ID</span><ModelCombobox value={route.upstreamModel} models={draft.upstreamModels} onChange={(value) => modelChanged(index, value)} placeholder="Actual model ID" /></label>
            <label className="field"><span>Display name in Claude</span><input value={route.displayName} onChange={(e) => updateRoute(index, { displayName: e.target.value })} placeholder={route.routeId} /></label>
            <label className="field"><span>Context limit <small>tokens</small></span><input dir="ltr" type="number" min="0" value={route.contextWindow || ""} onChange={(e) => updateRoute(index, { contextWindow: Number(e.target.value) || 0 })} placeholder="For example, 200000" /></label>
            <label className="field"><span>Maximum output <small>0 = no local limit</small></span><input dir="ltr" type="number" min="0" value={route.maxOutputTokens || ""} onChange={(e) => updateRoute(index, { maxOutputTokens: Number(e.target.value) || 0 })} placeholder="Optional" /></label>
            <label className="field"><span>Thinking mode</span>
              <ThemeSelect
                value={route.thinkingMode || (route.upstreamModel === "qwen38" || route.upstreamModel === "qwen38-sglang" ? "disabled" : "inherit")}
                options={[
                  { value: "inherit", label: "Provider / client default" },
                  { value: "disabled", label: "Disabled (Qwen38 recommended)" },
                  { value: "enabled", label: "Enabled" },
                ]}
                onChange={(value) => updateRoute(index, { thinkingMode: value as ModelRoute["thinkingMode"] })}
              />
              <small className="field-hint">Qwen38 defaults to non-thinking as documented by your provider. This setting affects only this model route.</small>
            </label>
          </div>
        </article>;
      })}
      {draft.routes.length === 0 && <div className="empty-card card"><span className="empty-glyph">◈</span><strong>No model routes yet</strong><p>Choose a Claude role and enter the exact model ID from your provider.</p></div>}
    </div>
    {availableRoles.length > 0 && <section className="add-mapping card"><div><span className="eyebrow">NEW ROUTE</span><h2>Add a role</h2></div><div className="add-mapping-fields"><label className="field"><span>Claude role</span><ThemeSelect value={newRole} options={[{ value: "", label: "Choose a role" }, ...availableRoles.map((role) => ({ value: role.id, label: `${role.title} · ${role.id}` }))]} onChange={setNewRole} /></label><label className="field"><span>Provider model</span><ModelCombobox value={newModel} models={draft.upstreamModels} onChange={setNewModel} placeholder="For example, northstar-reasoner" /></label><button className="button button-primary add-route-button" onClick={addRoute}>Add route <span>＋</span></button></div></section>}
    <div className="context-note"><span>i</span><p>Suggested context limits are 128,000 for qwen38 and 200,000 for qwen38-sglang. Adjust these values if your provider differs.</p></div>
  </>;
}

function ClientsPage({ config, localUrl, status, busy, onCopy, onDesktop, onCli, onMcp, isTauri }: {
  config: Draft; localUrl: string; status: ProxyStatus; busy: string; onCopy: () => void; onDesktop: () => void; onCli: () => void; onMcp: () => void; isTauri: boolean;
}) {
  return <>
    <div className="page-heading compact-heading"><div><span className="eyebrow">CLIENT SETUP</span><h1>Connect Claude</h1><p>Claude Proxy writes the required settings directly to each client.</p></div></div>
    <div className="client-endpoint card"><div className="endpoint-icon">⌘</div><div><span className="eyebrow">LOCAL PROXY ADDRESS</span><code dir="ltr">{localUrl}</code><small>{status.running ? "The proxy is running" : "Applying client settings also starts the proxy"}</small></div><button className="button button-secondary" onClick={onCopy}>Copy address</button></div>
    <div className="clients-grid">
      <article className="client-card card"><div className="client-brand"><span className="client-logo claude-logo">✳</span><span className="client-label">CLAUDE DESKTOP</span></div><h2>Claude Desktop</h2><p>Writes the model routes and local gateway to the inference profile. Fully quit and reopen Claude Desktop after applying the settings.</p><div className="client-meta"><span>Configuration method</span><strong>Gateway profile</strong></div><button className="button button-primary button-wide" disabled={!isTauri || !config.routes.length || busy === "desktop"} onClick={onDesktop}>{busy === "desktop" ? "Applying…" : "Configure Claude Desktop"}</button></article>
      <article className="client-card card"><div className="client-brand"><span className="client-logo terminal-logo">›_</span><span className="client-label">CLAUDE CODE</span></div><h2>Claude Code</h2><p>Writes the required ANTHROPIC variables to your user settings. Close and reopen any active Claude Code sessions.</p><div className="client-meta"><span>Configuration method</span><strong>User settings</strong></div><button className="button button-primary button-wide" disabled={!isTauri || !config.routes.length || busy === "cli"} onClick={onCli}>{busy === "cli" ? "Applying…" : "Configure Claude Code"}</button></article>
    </div>
    <section className="card" style={{ padding: 18, marginTop: 16 }}>
      <h2>Web search tools for Claude Code (MCP)</h2>
      <p>Register the local Claude Proxy web_search and web_fetch tools in Claude Code. This modifies only your local Claude Code MCP settings, creates a backup, and leaves other MCP tools untouched. Restart Claude Code and check /mcp after connecting.</p>
      <button className="button button-secondary" disabled={!isTauri || busy.length > 0 || config.webAccessMode === "disabled"} onClick={onMcp}>{busy === "mcp" ? "Connecting MCP…" : "Connect web tools to Claude Code"}</button>
      <p className="muted-copy">Enable and save Web access first. Cowork remote connectors cannot reach this localhost endpoint; its permissions are separate. No organization network policy is changed.</p>
    </section>
    {!config.routes.length && <div className="context-note"><span>!</span><p>Map at least one Claude role to a provider model in Model routes first.</p></div>}
    {!isTauri && <div className="context-note"><span>i</span><p>Applying settings to Claude is available in the installed Windows app.</p></div>}
    <div className="security-note"><span>i</span><p>The provider API key is encrypted by the app. Only the local token Claude needs is written to that client's settings file.</p></div>
  </>;
}

function WizardView({ step, setStep, draft, setDraft, secret, setSecret, newRole, setNewRole, newModel, setNewModel, addRoute, models, busy, isTauri, onFetch, onNext, onSkip, onGoManual, routeCount }: {
  step: number; setStep: (value: number) => void; draft: Draft; setDraft: Dispatch<SetStateAction<Draft>>; secret: string; setSecret: (value: string) => void; newRole: string; setNewRole: (value: string) => void; newModel: string; setNewModel: (value: string) => void; addRoute: () => void; models: string[]; busy: string; isTauri: boolean; onFetch: () => void; onNext: () => void; onSkip: () => void; onGoManual: () => void; routeCount: number;
}) {
  const labels = ["Provider", "Model discovery", "Model routes", "Finish"];
  return <div className="wizard-wrap">
    <div className="wizard-topline"><span className="eyebrow">QUICK SETUP</span><button className="text-button" onClick={onSkip}>Skip for now</button></div>
    <div className="wizard-progress" aria-label={`Step ${step + 1} of 4`}>{labels.map((label, index) => <button key={label} className={`wizard-step ${index === step ? "wizard-step-active" : index < step ? "wizard-step-done" : ""}`} onClick={() => index < step && setStep(index)}><i>{index < step ? "✓" : index + 1}</i><span>{label}</span></button>)}</div>
    <div className="wizard-card card">
      {step === 0 && <>
        <span className="eyebrow">STEP 1</span><h1>Connect to your model provider</h1><p className="wizard-copy">Enter an OpenAI-compatible URL and access key. No provider or key is preconfigured.</p>
        <div className="wizard-fields"><label className="field"><span>Provider label</span><input value={draft.providerName} onChange={(e) => setDraft((c) => ({ ...c, providerName: e.target.value }))} placeholder="For example, company gateway" /></label><label className="field"><span>Base API URL</span><input dir="ltr" value={draft.baseUrl} onChange={(e) => setDraft((c) => ({ ...c, baseUrl: e.target.value }))} placeholder="https://gateway.example/v1" /></label><label className="field"><span>API key</span><input dir="ltr" type="password" value={secret} onChange={(e) => setSecret(e.target.value)} placeholder={draft.apiKeyConfigured ? "Leave blank to keep the saved key" : "Provider API key"} autoComplete="new-password" /></label><div className="wizard-inline"><label className="field"><span>Authentication</span><ThemeSelect value={draft.authStyle} options={[{ value: "bearer", label: "Bearer token" }, { value: "api_key", label: "api-key header" }]} onChange={(value) => setDraft((c) => ({ ...c, authStyle: value as BridgeConfig["authStyle"] }))} /></label><label className="field"><span>Local port</span><input dir="ltr" type="number" min="1024" max="65535" value={draft.proxyPort} onChange={(e) => setDraft((c) => ({ ...c, proxyPort: Number(e.target.value) || 18181 }))} /></label></div></div>
      </>}
      {step === 1 && <>
        <span className="eyebrow">STEP 2</span><h1>Find available models</h1><p className="wizard-copy">Claude Proxy reads models from /v1/models. If your provider does not offer that endpoint, enter model IDs manually later.</p>
        <div className="fetch-box"><span className="fetch-icon">↻</span><div><strong>{models.length ? `${models.length} models retrieved` : "The model list has not been fetched yet"}</strong><small>{models.slice(0, 4).join(", ") || "Fetching models is optional."}</small></div><button className="button button-secondary" disabled={!isTauri || busy === "fetch"} onClick={onFetch}>{busy === "fetch" ? "Checking…" : "Fetch models"}</button></div>
        {!isTauri && <div className="info-note"><span>i</span><p>Live model discovery is available in the desktop app. You can skip this step.</p></div>}
      </>}
      {step === 2 && <>
        <span className="eyebrow">STEP 3</span><h1>Map a Claude role to a model</h1><p className="wizard-copy">Claude clients use role IDs. Choose the provider model for each role here.</p>
        <div className="wizard-route"><label className="field"><span>Claude role</span><ThemeSelect value={newRole} options={[{ value: "", label: "Choose a role" }, ...routeRoles.filter((role) => !draft.routes.some((route) => route.routeId === role.id)).map((role) => ({ value: role.id, label: `${role.title} · ${role.id}` }))]} onChange={setNewRole} /></label><label className="field"><span>Provider model ID</span><ModelCombobox value={newModel} models={models} onChange={setNewModel} placeholder="Provider model ID" /></label><button className="button button-secondary" onClick={addRoute}>Add route</button></div>
        <div className="wizard-route-list">{draft.routes.map((route) => <div key={route.routeId}><span className="route-check">✓</span><strong>{route.routeId}</strong><code dir="ltr">{route.upstreamModel}</code></div>)}</div>
        {!routeCount && <div className="info-note"><span>i</span><p>You can skip this step and add model routes later.</p></div>}
      </>}
      {step === 3 && <>
        <span className="eyebrow">STEP 4</span><h1>You're ready to connect</h1><p className="wizard-copy">Your settings will be saved. Configure Claude Desktop or Claude Code separately from the Claude clients page.</p>
        <div className="review-list"><div><span>Provider</span><strong>{draft.providerName || draft.baseUrl || "Not configured yet"}</strong></div><div><span>Local port</span><code dir="ltr">127.0.0.1:{draft.proxyPort}</code></div><div><span>Model routes</span><strong>{routeCount ? `${routeCount.toLocaleString("en-US")} routes` : "I'll do this later"}</strong></div></div>
        {routeCount > 0 && <div className="context-note"><span>✓</span><p>After setup, configure Claude Desktop or Claude Code from the Claude clients page.</p></div>}
      </>}
      <div className="wizard-actions"><button className="button button-secondary" onClick={onGoManual}>Configure manually</button><div>{step > 0 && <button className="button button-quiet" onClick={() => setStep(step - 1)}>Previous</button>}<button className="button button-primary" disabled={busy === "save" || busy === "fetch"} onClick={onNext}>{step === 3 ? "Save and finish" : "Continue"}<span>→</span></button></div></div>
    </div>
    <p className="wizard-footnote">You can revisit provider, model, and client settings at any time.</p>
  </div>;
}

function messageOf(reason: unknown): string {
  if (typeof reason === "string") return reason;
  if (reason instanceof Error) return reason.message;
  return "The operation could not be completed. Check your settings and connection.";
}

export default App;
