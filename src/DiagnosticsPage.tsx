import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { createPortal } from "react-dom";
import packageMetadata from "../package.json";
import { clearDiagnostics, getDiagnostics } from "./lib/tauri";
import type { DiagnosticEntry } from "./types";
import { localeFor, translateText, type Language } from "./localization";

type LevelFilter = "all" | "problems";

export function DiagnosticsPage({ isTauri, language }: { isTauri: boolean; language: Language }) {
  const [entries, setEntries] = useState<DiagnosticEntry[]>([]);
  const [filter, setFilter] = useState<LevelFilter>("all");
  const [loading, setLoading] = useState(true);
  const [message, setMessage] = useState("");
  const [error, setError] = useState("");
  const [clearDialogOpen, setClearDialogOpen] = useState(false);
  const [clearingLogs, setClearingLogs] = useState(false);
  const clearTriggerRef = useRef<HTMLButtonElement>(null);
  const clearCancelRef = useRef<HTMLButtonElement>(null);
  const clearDialogRef = useRef<HTMLElement>(null);
  const headingRef = useRef<HTMLHeadingElement>(null);
  const clearingLogsRef = useRef(false);

  const refresh = useCallback(async () => {
    if (!isTauri) {
      setEntries([]);
      setLoading(false);
      return;
    }
    setLoading(true);
    setError("");
    try {
      setEntries(await getDiagnostics(500));
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason));
    } finally {
      setLoading(false);
    }
  }, [isTauri]);

  useEffect(() => { void refresh(); }, [refresh]);
  useEffect(() => {
    if (!message) return;
    const timer = window.setTimeout(() => setMessage(""), 2800);
    return () => window.clearTimeout(timer);
  }, [message]);

  useEffect(() => {
    if (!clearDialogOpen) return;
    const previousFocus = document.activeElement instanceof HTMLElement ? document.activeElement : null;
    const focusFrame = window.requestAnimationFrame(() => clearCancelRef.current?.focus());
    const handleDialogKeys = (event: KeyboardEvent) => {
      if (event.key === "Escape" && !clearingLogsRef.current) {
        event.preventDefault();
        setClearDialogOpen(false);
        return;
      }
      if (event.key !== "Tab") return;
      const focusable = clearDialogRef.current?.querySelectorAll<HTMLElement>("button:not(:disabled)");
      if (!focusable?.length) return;
      const first = focusable[0];
      const last = focusable[focusable.length - 1];
      if (event.shiftKey && document.activeElement === first) {
        event.preventDefault();
        last.focus();
      } else if (!event.shiftKey && document.activeElement === last) {
        event.preventDefault();
        first.focus();
      }
    };
    document.addEventListener("keydown", handleDialogKeys);
    return () => {
      window.cancelAnimationFrame(focusFrame);
      document.removeEventListener("keydown", handleDialogKeys);
      window.requestAnimationFrame(() => {
        if (clearTriggerRef.current && !clearTriggerRef.current.disabled) clearTriggerRef.current.focus();
        else if (previousFocus?.isConnected && !(previousFocus instanceof HTMLButtonElement && previousFocus.disabled)) previousFocus.focus();
        else headingRef.current?.focus();
      });
    };
  }, [clearDialogOpen]);

  const visible = useMemo(() => filter === "all" ? entries : entries.filter((entry) => entry.level !== "info"), [entries, filter]);
  const report = useMemo(() => [
    "Claude Proxy " + packageMetadata.version + " diagnostic report",
    "Created: " + new Date().toISOString(),
    "Log data is stored locally. Prompts, completions, provider response bodies, and API keys are excluded.",
    "",
    ...visible.map((entry) => new Date(entry.timestamp * 1000).toISOString() + " [" + entry.level.toUpperCase() + "] " + entry.category + ": " + entry.message),
  ].join("\n"), [visible]);

  async function copyReport() {
    try {
      await navigator.clipboard.writeText(report);
      setMessage("Diagnostic report copied.");
    } catch {
      setError(translateText("Clipboard access is unavailable. Download the report instead.", language));
    }
  }

  function downloadReport() {
    const file = new Blob([report], { type: "text/plain;charset=utf-8" });
    const url = URL.createObjectURL(file);
    const link = document.createElement("a");
    link.href = url;
    link.download = "claude-proxy-diagnostics-" + new Date().toISOString().slice(0, 10) + ".txt";
    link.click();
    window.setTimeout(() => URL.revokeObjectURL(url), 1000);
    setMessage("Diagnostic report downloaded.");
  }

  async function confirmClearLogs() {
    if (!isTauri || !entries.length || clearingLogsRef.current) return;
    clearingLogsRef.current = true;
    setClearingLogs(true);
    setError("");
    try {
      await clearDiagnostics();
      setEntries([]);
      setMessage("Diagnostic history cleared.");
      setClearDialogOpen(false);
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason));
      setClearDialogOpen(false);
    } finally {
      clearingLogsRef.current = false;
      setClearingLogs(false);
    }
  }

  const problemCount = entries.filter((entry) => entry.level !== "info").length;

  return <div className="diagnostics-page">
    <div className="page-heading diagnostics-heading"><div><span className="eyebrow">LOCAL TROUBLESHOOTING</span><h1 ref={headingRef} tabIndex={-1}>Diagnostics</h1><p>Review proxy and provider issues, then copy or download a small report to share with the developer.</p></div></div>
    {error && <div className="alert alert-error" role="alert"><span>!</span><p>{error}</p><button type="button" onClick={() => setError("")} aria-label="Dismiss error">×</button></div>}
    <section className="diagnostics-toolbar" aria-label="Diagnostic log controls">
      <div className="diagnostics-summary"><span>{loading ? "Loading local diagnostics…" : language === "fa" ? `${entries.length.toLocaleString(localeFor(language))} مورد · ${problemCount.toLocaleString(localeFor(language))} هشدار و خطا` : `${entries.length.toLocaleString(localeFor(language))} entries · ${problemCount.toLocaleString(localeFor(language))} warnings and errors`}</span><span className="usage-toggle-group" role="group" aria-label="Filter log entries"><button type="button" aria-pressed={filter === "all"} onClick={() => setFilter("all")}>All</button><button type="button" aria-pressed={filter === "problems"} onClick={() => setFilter("problems")}>Problems</button></span></div>
      <div className="diagnostics-actions">
        <button className="button button-secondary" type="button" onClick={() => void refresh()} disabled={loading}>↻  Refresh</button>
        <button className="button button-secondary" type="button" onClick={() => void copyReport()} disabled={!visible.length}>Copy report</button>
        <button className="button button-secondary" type="button" onClick={downloadReport} disabled={!visible.length}>Download report</button>
        <button ref={clearTriggerRef} className="button button-danger" type="button" onClick={() => setClearDialogOpen(true)} disabled={!isTauri || !entries.length}>
          <svg aria-hidden="true" viewBox="0 0 16 16" width="15" height="15" fill="none" stroke="currentColor" strokeWidth="1.6" strokeLinecap="round" strokeLinejoin="round">
            <path d="M2.5 4.5h11M6 4.5V2.7h4v1.8m2.2 0-.6 9H4.4l-.6-9m3.2 2.2v4.5m2.1-4.5v4.5" />
          </svg>
          <span>Clear logs</span>
        </button>
      </div>
    </section>
    {message && <p className="context-note" role="status"><span>✓</span>{message}</p>}
    {clearDialogOpen && createPortal(
      <div className="diagnostics-confirm-backdrop" dir={language === "fa" ? "rtl" : "ltr"} onMouseDown={(event) => {
        if (!clearingLogsRef.current && event.target === event.currentTarget) setClearDialogOpen(false);
      }}>
        <section ref={clearDialogRef} className="diagnostics-confirm-card" role="dialog" aria-modal="true" aria-labelledby="clear-logs-title" aria-describedby="clear-logs-description">
          <button className="diagnostics-confirm-close" type="button" aria-label={translateText("Close dialog", language)} onClick={() => setClearDialogOpen(false)} disabled={clearingLogs}>×</button>
          <div className="diagnostics-confirm-heading">
            <span className="diagnostics-confirm-icon" aria-hidden="true">
              <svg viewBox="0 0 24 24" width="24" height="24" fill="none" stroke="currentColor" strokeWidth="1.7" strokeLinecap="round" strokeLinejoin="round">
                <path d="M4.5 7h15M9 7V4.5h6V7m2.5 0-.8 12H7.3L6.5 7m3.4 3v5.5m4.2-5.5v5.5" />
              </svg>
            </span>
            <div className="diagnostics-confirm-title-wrap">
              <span className="diagnostics-confirm-kicker">{translateText("LOCAL DIAGNOSTICS", language)}</span>
              <h2 id="clear-logs-title">{translateText("Delete diagnostic logs?", language)}</h2>
            </div>
          </div>
          <p id="clear-logs-description" className="diagnostics-confirm-description">{translateText("This removes the diagnostic logs stored on this computer. You cannot undo this action.", language)}</p>
          <div className="diagnostics-confirm-impact"><span aria-hidden="true">i</span><p>{translateText("Your token usage history will remain untouched.", language)}</p></div>
          <div className="diagnostics-confirm-actions">
            <button ref={clearCancelRef} className="button button-secondary" type="button" onClick={() => setClearDialogOpen(false)} disabled={clearingLogs}>{translateText("Cancel", language)}</button>
            <button className="button button-danger diagnostics-confirm-delete" type="button" onClick={() => void confirmClearLogs()} disabled={clearingLogs}>
              {clearingLogs ? translateText("Deleting…", language) : translateText("Clear logs", language)}
            </button>
          </div>
        </section>
      </div>,
      document.body,
    )}
    {!isTauri ? <div className="diagnostics-empty"><strong>Open the Windows app to view logs</strong><p>The browser preview cannot read or change the local diagnostic history.</p></div> : visible.length ? <section className="diagnostics-list" aria-label="Recent diagnostic entries">
      {visible.map((entry, index) => <article className="diagnostic-row" key={String(entry.timestamp) + "-" + index} style={{ animationDelay: String(Math.min(index, 8) * 24) + "ms" }}>
        <time className="diagnostic-time" dateTime={new Date(entry.timestamp * 1000).toISOString()}>{new Date(entry.timestamp * 1000).toLocaleString(localeFor(language), { month: "short", day: "numeric", hour: "numeric", minute: "2-digit", second: "2-digit" })}</time>
        <span className={"diagnostic-level diagnostic-level-" + entry.level}>{entry.level}</span>
        <div className="diagnostic-copy"><strong>{entry.category}</strong><p>{entry.message}</p></div>
      </article>)}
    </section> : <div className="diagnostics-empty"><strong>No diagnostics yet</strong><p>Proxy starts, configuration changes, model discovery problems, provider errors, and token-history issues will appear here.</p></div>}
    <p className="diagnostics-note">Reports stay on this computer until you copy or download them. Token history and diagnostics exclude conversation content and credentials.</p>
  </div>;
}
