import type { Language } from "./localization";
import type { ProxyStatus } from "./types";
import "./ProxyStatusControl.css";

type Props = {
  status: ProxyStatus;
  language: Language;
  busy: boolean;
  onToggle: () => void;
};

export function ProxyStatusControl({ status, language, busy, onToggle }: Props) {
  const isPersian = language === "fa";
  const statusText = busy
    ? isPersian ? status.running ? "در حال توقف…" : "در حال راه‌اندازی…" : status.running ? "Stopping…" : "Starting…"
    : isPersian ? status.running ? "پروکسی فعال است" : "پروکسی غیرفعال است" : status.running ? "Proxy active" : "Proxy inactive";
  const actionText = isPersian
    ? status.running ? "توقف پروکسی" : "شروع پروکسی"
    : status.running ? "Stop proxy" : "Start proxy";

  return (
    <div className="proxy-control" data-running={status.running}>
      <div className="proxy-control-info">
        <div className="proxy-control-heading" role="status" aria-live="polite">
          <span className="proxy-control-indicator" aria-hidden="true" />
          <span className="proxy-control-label">{statusText}</span>
        </div>
        <span className="proxy-control-address" dir="ltr">127.0.0.1:{status.port}</span>
      </div>
      <button className="proxy-control-button" type="button" onClick={onToggle} disabled={busy} aria-label={actionText} title={actionText} aria-pressed={status.running}>
        {busy ? <span className="proxy-control-spinner" aria-hidden="true" /> : status.running
          ? <svg viewBox="0 0 20 20" fill="none" aria-hidden="true"><rect x="5.5" y="5.5" width="9" height="9" rx="1.5" fill="currentColor" /></svg>
          : <svg viewBox="0 0 20 20" fill="none" aria-hidden="true"><path d="M6.5 4.8a1 1 0 0 1 1.52-.85l8.06 5.2a1 1 0 0 1 0 1.7l-8.06 5.2a1 1 0 0 1-1.52-.85V4.8Z" fill="currentColor" /></svg>}
      </button>
    </div>
  );
}
