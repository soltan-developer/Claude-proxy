import { useCallback, useEffect, useMemo, useRef, useState, type CSSProperties } from "react";
import { getUsageReport } from "./lib/tauri";
import type { BridgeConfig, UsageAggregate, UsageQuery } from "./types";
import { localeFor, type Language } from "./localization";
import { ThemeSelect } from "./ThemeSelect";

type RangePreset = "1h" | "1d" | "7d" | "30d" | "custom";
type Granularity = "daily" | "monthly";
type Grouping = "total" | "model";
type Series = { id: string; label: string; color: string; values: number[]; kind: "input" | "output" | "cache-read" | "cache-write" | "model" };
type ChartPoint = { key: string; dayIndex: number; label: string; input: number; output: number; cacheRead: number; cacheWrite: number; requests: number; models: Record<string, number>; modelLabels: Record<string, string> };

const updateKey = "claude-proxy-usage-refresh-seconds-v2";
const modelColors = ["#657c65", "#a86f4e", "#5e8490", "#9a6871", "#806d9a", "#8c874e", "#4b806c", "#747d91"];

export function UsagePage({ isTauri, language, config }: { isTauri: boolean; language: Language; config: BridgeConfig }) {
  const initialRange = useMemo(() => {
    const end = new Date();
    end.setHours(0, 0, 0, 0);
    const start = new Date(end);
    start.setDate(start.getDate() - 29);
    return { start: dateString(start), end: dateString(end) };
  }, []);
  const [preset, setPreset] = useState<RangePreset>("30d");
  const [startDate, setStartDate] = useState(initialRange.start);
  const [endDate, setEndDate] = useState(initialRange.end);
  const [granularity, setGranularity] = useState<Granularity>("daily");
  const [grouping, setGrouping] = useState<Grouping>("total");
  const [providerFilter, setProviderFilter] = useState("all");
  const [modelFilter, setModelFilter] = useState("all");
  const [refreshSeconds, setRefreshSeconds] = useState(() => {
    const stored = window.localStorage.getItem(updateKey);
    if (stored === null) return 5;
    const saved = Number(stored);
    return [0, 5, 15, 30, 60, 300].includes(saved) ? saved : 5;
  });
  const [rows, setRows] = useState<UsageAggregate[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState("");
  const [lastUpdated, setLastUpdated] = useState<Date | null>(null);
  const inFlight = useRef(false);
  const requestSequence = useRef(0);
  const pendingQuiet = useRef<boolean | null>(null);
  const refreshRef = useRef<((quiet?: boolean) => Promise<void>) | null>(null);

  const refresh = useCallback(async (quiet = false) => {
    if (!isTauri) {
      setRows([]);
      setLoading(false);
      return;
    }
    const requestId = ++requestSequence.current;
    const presetRange = preset === "custom" ? null : presetTimestamps(preset);
    const start = presetRange?.start ?? localDateStart(startDate);
    const end = presetRange?.end ?? localDateStart(addDays(endDate, 1));
    if (!Number.isFinite(start) || !Number.isFinite(end) || start >= end) {
      setError("Choose a valid date range.");
      setLoading(false);
      return;
    }
    if (inFlight.current) {
      pendingQuiet.current = quiet;
      return;
    }
    inFlight.current = true;
    if (!quiet) setLoading(true);
    setError("");
    const query: UsageQuery = {
      startTimestamp: Math.floor(start / 1000),
      endTimestamp: Math.floor(end / 1000),
      utcOffsetMinutes: new Date().getTimezoneOffset(),
    };
    try {
      const result = await getUsageReport(query);
      if (requestId === requestSequence.current) {
        setRows(result);
        setLastUpdated(new Date());
      }
    } catch (reason) {
      if (requestId === requestSequence.current) setError(messageOf(reason));
    } finally {
      inFlight.current = false;
      if (requestId === requestSequence.current && !quiet) setLoading(false);
      if (pendingQuiet.current !== null) {
        const nextQuiet = pendingQuiet.current;
        pendingQuiet.current = null;
        window.setTimeout(() => { void refreshRef.current?.(nextQuiet); }, 0);
      }
    }
  }, [endDate, isTauri, preset, startDate]);
  refreshRef.current = refresh;

  useEffect(() => { void refresh(); }, [refresh]);
  useEffect(() => {
    window.localStorage.setItem(updateKey, String(refreshSeconds));
    if (!refreshSeconds) return;
    const timer = window.setInterval(() => { void refresh(true); }, refreshSeconds * 1000);
    return () => window.clearInterval(timer);
  }, [refresh, refreshSeconds]);

  const chartRange = useMemo(() => {
    if (preset === "custom") return { start: startDate, end: endDate };
    const range = presetTimestamps(preset);
    return { start: dateString(new Date(range.start)), end: dateString(new Date(range.end)) };
  }, [preset, startDate, endDate]);
  const configuredProvider = config.providerName.trim() || config.baseUrl.trim();
  const configuredModels = useMemo(() => [...new Set([...config.upstreamModels, ...config.routes.map((route) => route.upstreamModel)].map((model) => model.trim()).filter(Boolean))].sort((a, b) => a.localeCompare(b)), [config.routes, config.upstreamModels]);
  const providers = useMemo(() => [...new Set([configuredProvider, ...rows.map((row) => row.provider)].filter(Boolean))].sort((a, b) => a.localeCompare(b)), [configuredProvider, rows]);
  const providerRows = useMemo(() => providerFilter === "all" ? rows : rows.filter((row) => row.provider === providerFilter), [rows, providerFilter]);
  const models = useMemo(() => [...new Set([...configuredModels, ...providerRows.map((row) => row.model)].filter(Boolean))].sort((a, b) => a.localeCompare(b)), [configuredModels, providerRows]);
  const visibleRows = useMemo(() => grouping !== "model" ? rows : modelFilter === "all" ? providerRows : providerRows.filter((row) => row.model === modelFilter), [rows, providerRows, modelFilter, grouping]);
  useEffect(() => {
    if (providerFilter !== "all" && !providers.includes(providerFilter)) {
      setProviderFilter("all");
      setModelFilter("all");
    }
  }, [providers, providerFilter]);
  useEffect(() => {
    if (modelFilter !== "all" && !models.includes(modelFilter)) setModelFilter("all");
  }, [models, modelFilter]);
  const points = useMemo(() => makePoints(visibleRows, chartRange.start, chartRange.end, granularity, localeFor(language)), [visibleRows, chartRange, granularity, language]);
  const totals = useMemo(() => visibleRows.reduce((sum, row) => ({
    input: sum.input + uncachedInput(row),
    output: sum.output + row.outputTokens,
    cachedInput: sum.cachedInput + row.cachedInputTokens,
    cacheWriteInput: sum.cacheWriteInput + row.cacheWriteInputTokens,
    cacheStatsInput: sum.cacheStatsInput + row.cacheStatsInputTokens,
    cacheStatsRequests: sum.cacheStatsRequests + row.cacheStatsReportedRequests,
    requests: sum.requests + row.requests,
    estimatedInput: sum.estimatedInput + row.estimatedInputRequests,
    estimatedOutput: sum.estimatedOutput + row.estimatedOutputRequests,
  }), { input: 0, output: 0, cachedInput: 0, cacheWriteInput: 0, cacheStatsInput: 0, cacheStatsRequests: 0, requests: 0, estimatedInput: 0, estimatedOutput: 0 }), [visibleRows]);
  const modelTotals = useMemo(() => {
    const grouped = new Map<string, { provider: string; model: string; requestedModel: string; input: number; output: number; cachedInput: number; cacheWriteInput: number; cacheStatsRequests: number; requests: number; estimated: number }>();
    for (const row of visibleRows) {
      const provider = row.provider || "Unknown provider";
      const key = `${provider}\u0000${row.model}\u0000${row.requestedModel}`;
      const entry = grouped.get(key) ?? { provider, model: row.model, requestedModel: row.requestedModel, input: 0, output: 0, cachedInput: 0, cacheWriteInput: 0, cacheStatsRequests: 0, requests: 0, estimated: 0 };
      entry.input += row.inputTokens;
      entry.output += row.outputTokens;
      entry.cachedInput += row.cachedInputTokens;
      entry.cacheWriteInput += row.cacheWriteInputTokens;
      entry.cacheStatsRequests += row.cacheStatsReportedRequests;
      entry.requests += row.requests;
      entry.estimated += row.estimatedInputRequests + row.estimatedOutputRequests;
      grouped.set(key, entry);
    }
    return [...grouped.values()].sort((a, b) => (b.input + b.output) - (a.input + a.output));
  }, [visibleRows]);
  const estimatedCount = totals.estimatedInput + totals.estimatedOutput;
  const series = useMemo(() => makeSeries(points, grouping), [points, grouping]);

  function selectPreset(next: RangePreset) {
    setPreset(next);
  }

  function changeRefresh(seconds: number) {
    setRefreshSeconds(seconds);
  }

  function resetUsageView() {
    setPreset("30d");
    setProviderFilter("all");
    setModelFilter("all");
    setGrouping("total");
  }

  return <div className="usage-page">
    <div className="page-heading usage-heading"><div><span className="eyebrow">USAGE ANALYTICS</span><h1>Token usage</h1><p>Track provider-reported token use over time, with clear labels when the provider does not return exact counts.</p></div></div>

    <section className="usage-toolbar" aria-label="Usage report controls">
      <div className="usage-period-row">
        <label className="usage-field usage-range-field"><span>Date range</span><ThemeSelect value={preset} ariaLabel="Date range preset" options={[{ value: "1h", label: "Past hour" }, { value: "1d", label: "Past day" }, { value: "7d", label: "7 days" }, { value: "30d", label: "30 days" }, { value: "custom", label: "Custom" }]} onChange={(value) => selectPreset(value as RangePreset)} /></label>
        {preset === "custom" && <>
          <label className="usage-field"><span>From</span><input type="date" value={startDate} max={endDate} onChange={(event) => setStartDate(event.target.value)} /></label>
          <label className="usage-field"><span>Through</span><input type="date" value={endDate} min={startDate} max={todayString()} onChange={(event) => setEndDate(event.target.value)} /></label>
        </>}
      </div>
      <div className="usage-settings-row">
        <label className="usage-field"><span>Group by date</span><ThemeSelect value={granularity} options={[{ value: "daily", label: "Daily" }, { value: "monthly", label: "Monthly" }]} onChange={(value) => setGranularity(value as Granularity)} /></label>
        <div className="usage-toggle-group" role="group" aria-label="Usage breakdown">
          <button type="button" aria-pressed={grouping === "total"} onClick={() => setGrouping("total")}>Total</button>
          <button type="button" aria-pressed={grouping === "model"} onClick={() => setGrouping("model")}>By model</button>
        </div>
        {grouping === "model" && <div className="usage-model-filters" aria-label="Provider and model filters">
          <label className="usage-field"><span>Provider</span><ThemeSelect value={providerFilter} options={[{ value: "all", label: "All providers" }, ...providers.map((provider) => ({ value: provider, label: provider }))]} onChange={(value) => { setProviderFilter(value); setModelFilter("all"); }} /></label>
          <label className="usage-field"><span>Model</span><ThemeSelect value={modelFilter} options={[{ value: "all", label: "All models" }, ...models.map((model) => ({ value: model, label: model }))]} onChange={setModelFilter} /></label>
        </div>}
        <label className="usage-field"><span>Refresh every</span><ThemeSelect value={String(refreshSeconds)} options={[{ value: "0", label: "Off" }, { value: "5", label: "5 seconds" }, { value: "15", label: "15 seconds" }, { value: "30", label: "30 seconds" }, { value: "60", label: "1 minute" }, { value: "300", label: "5 minutes" }]} onChange={(value) => changeRefresh(Number(value))} /></label>
        <button className="usage-refresh" type="button" onClick={() => void refresh()} disabled={loading} aria-label="Refresh token usage">{loading ? "Updating…" : "↻  Update now"}</button>
      </div>
    </section>

    {error && <div className="alert alert-error" role="alert"><span>!</span><p>{error}</p></div>}

    <section className="usage-metric-grid" aria-label="Usage totals">
      <Metric label="Input tokens" value={totals.input} detail={totals.estimatedInput ? `${formatNumber(totals.estimatedInput, language)} request estimates` : "Provider usage"} language={language} />
      <Metric label="Output tokens" value={totals.output} detail={totals.estimatedOutput ? `${formatNumber(totals.estimatedOutput, language)} request estimates` : "Generated response tokens"} language={language} />
      <CacheSummaryMetric read={totals.cachedInput} created={totals.cacheWriteInput} cacheStatsInput={totals.cacheStatsInput} reportedRequests={totals.cacheStatsRequests} requests={totals.requests} language={language} />
      <Metric label="Models used" value={new Set(visibleRows.map((row) => `${row.provider}\u0000${row.model}`)).size} detail={lastUpdated ? `Updated ${lastUpdated.toLocaleTimeString(localeFor(language), { hour: "numeric", minute: "2-digit" })}` : "Waiting for usage"} language={language} />
    </section>
    <p className="usage-cache-definition">Input excludes cache tokens when the provider reports them; unknown cache stays in input. Cache combines reads and creation. Output is generated separately.</p>

    <section className="usage-chart-card card" aria-labelledby="usage-chart-title">
      <div className="usage-section-heading"><div><h2 id="usage-chart-title">{granularity === "daily" ? "Daily usage" : "Monthly usage"}</h2><p>{grouping === "total" ? "Input, cache, and output token totals" : "Token totals for each provider model"}</p></div><span className="status-pill">{loading ? "Updating" : `${formatNumber(totals.requests, language)} requests`}</span></div>
      {points.length && totals.requests ? <>
        <UsageChart points={points} series={series} grouping={grouping} language={language} />
        <div className="usage-legend" aria-label="Chart legend">{series.map((item) => <span className="usage-legend-item" key={item.id}><i className={item.kind === "model" ? "usage-legend-mark-line" : "usage-legend-mark"} style={{ "--legend-color": item.color } as CSSProperties} />{item.label}</span>)}</div>
        <p className="usage-estimate-note"><span aria-hidden="true">i</span>{estimatedCount ? `Estimated usage: input for ${formatNumber(totals.estimatedInput, language)} requests and output for ${formatNumber(totals.estimatedOutput, language)} requests. ` : "Token counts were supplied by the provider. "}Hover or focus chart marks for details; the table labels totals that include estimates.</p>
      </> : <div className="usage-empty"><strong>{isTauri ? "No usage in this date range" : "Usage is available in the Windows app"}</strong><p>{isTauri ? "Successful requests through the local proxy will appear here. Usage history is stored on this computer." : "The browser preview does not have access to your local proxy history. Open the installed desktop app to view real usage."}</p></div>}
    </section>

    <section className="usage-breakdown card" aria-labelledby="usage-breakdown-title">
      <div className="usage-section-heading"><div><h2 id="usage-breakdown-title">Model breakdown</h2><p>Input and output by provider model; cache read and creation are separate when reported.</p></div></div>
      {modelTotals.length ? <div className="usage-table-wrap"><table className="usage-table"><thead><tr><th scope="col">Provider</th><th scope="col">Provider model</th><th scope="col">Claude route</th><th scope="col" className="usage-number">Requests</th><th scope="col" className="usage-number">Input tokens</th><th scope="col" className="usage-number">Output tokens</th><th scope="col" className="usage-number">Cache read</th><th scope="col" className="usage-number">Cache creation</th><th scope="col">Source</th></tr></thead><tbody>
        {modelTotals.map((item) => <tr key={`${item.provider}-${item.model}-${item.requestedModel}`}><td><span className="technical-value" dir="auto">{item.provider}</span></td><td><span className="technical-value" dir="auto">{item.model}</span></td><td><span className="technical-value" dir="auto">{item.requestedModel}</span></td><td className="usage-number">{formatNumber(item.requests, language)}</td><td className="usage-number">{formatNumber(Math.max(0, item.input - item.cachedInput - item.cacheWriteInput), language)}</td><td className="usage-number">{formatNumber(item.output, language)}</td><td className="usage-number" title={`${formatNumber(item.cacheStatsRequests, language)} / ${formatNumber(item.requests, language)} requests reported cache details`}>{item.cacheStatsRequests ? formatNumber(item.cachedInput, language) : "—"}</td><td className="usage-number">{item.cacheStatsRequests ? formatNumber(item.cacheWriteInput, language) : "—"}</td><td><span className="usage-source-badge">{item.estimated ? "Includes estimates" : "Provider reported"}</span></td></tr>)}
      </tbody></table></div> : <div className="usage-table-empty"><span className="empty-glyph" aria-hidden="true">⌁</span><strong>No successful requests recorded for this range.</strong><p>Try a wider date range or reset the provider and model filters.</p><button className="button button-secondary" type="button" onClick={resetUsageView}>Show the last 30 days</button></div>}
    </section>
    <p className="diagnostics-note">The local history stores timestamps, provider names, model IDs, token totals, provider-reported cache counts, and estimate flags. It never stores prompts, completions, API keys, or provider response bodies.</p>
  </div>;
}

function Metric({ label, value, detail, language }: { label: string; value: number; detail: string; language: Language }) {
  return <article className="usage-metric"><span className="usage-metric-label">{label}</span><strong>{formatNumber(value, language)}</strong><small>{detail}</small></article>;
}

function CacheSummaryMetric({ read, created, cacheStatsInput, reportedRequests, requests, language }: { read: number; created: number; cacheStatsInput: number; reportedRequests: number; requests: number; language: Language }) {
  const hasCacheData = reportedRequests > 0;
  const cacheHitRate = cacheStatsInput > 0 ? Math.min(100, (read / cacheStatsInput) * 100) : null;
  return <article className="usage-metric usage-cache-summary">
    <span className="usage-metric-label">Cache tokens</span>
    <strong>{hasCacheData ? formatNumber(read + created, language) : "—"}</strong>
    {hasCacheData ? <div className="usage-cache-parts"><small><span>Cache read</span> {formatNumber(read, language)}</small><small><span>Cache creation</span> {formatNumber(created, language)}</small><small><span>Hit rate</span> {cacheHitRate === null ? "—" : `${cacheHitRate.toLocaleString(localeFor(language), { maximumFractionDigits: 1 })}%`}</small></div> : <small>Not reported by provider</small>}
    {hasCacheData && reportedRequests < requests && <small>{formatNumber(reportedRequests, language)} / {formatNumber(requests, language)} requests reported cache usage</small>}
  </article>;
}

function UsageChart({ points, series, grouping, language }: { points: ChartPoint[]; series: Series[]; grouping: Grouping; language: Language }) {
  const width = 920;
  const height = 300;
  const margin = { top: 16, right: 18, bottom: 42, left: 70 };
  const plotWidth = width - margin.left - margin.right;
  const plotHeight = height - margin.top - margin.bottom;
  const number = (value: number) => formatNumber(value, language);
  const maximum = Math.max(1, ...points.map((point) => grouping === "total" ? point.input + point.cacheRead + point.cacheWrite + point.output : Math.max(0, ...series.map((item) => item.values[points.indexOf(point)] ?? 0))));
  const step = niceStep(maximum / 4);
  const axisMax = step * Math.ceil(maximum / step);
  const y = (value: number) => margin.top + plotHeight - (value / axisMax) * plotHeight;
  const x = (index: number) => margin.left + (points.length <= 1 ? plotWidth / 2 : (index / (points.length - 1)) * plotWidth);
  const slot = plotWidth / Math.max(1, points.length);
  const barWidth = Math.min(30, Math.max(4, slot * .55));
  const labelEvery = Math.max(1, Math.ceil(points.length / 8));

  return <div className="usage-chart-wrap"><svg className="usage-chart-svg" viewBox={`0 0 ${width} ${height}`} role="img" aria-label={`${grouping === "total" ? "Input, cache, and output tokens" : "Tokens by provider model"} over ${points.length} ${points.length === 1 ? "period" : "periods"}`}>
    {[0, 1, 2, 3, 4].map((index) => {
      const value = axisMax * (index / 4);
      const lineY = y(value);
      return <g key={index}><line className="usage-gridline" x1={margin.left} x2={width - margin.right} y1={lineY} y2={lineY} /><text className="usage-axis-label" x={margin.left - 10} y={lineY + 4} textAnchor="end">{formatCompact(value, language)}</text></g>;
    })}
    {grouping === "total" ? points.map((point, index) => {
      const barX = x(index) - barWidth / 2;
      const inputHeight = (point.input / axisMax) * plotHeight;
      const cacheReadHeight = (point.cacheRead / axisMax) * plotHeight;
      const cacheWriteHeight = (point.cacheWrite / axisMax) * plotHeight;
      const outputHeight = (point.output / axisMax) * plotHeight;
      const cacheReadTop = point.input + point.cacheRead;
      const cacheWriteTop = cacheReadTop + point.cacheWrite;
      const outputTop = cacheWriteTop + point.output;
      return <g key={point.key}>
        <rect className="usage-bar-input" x={barX} y={y(point.input)} width={barWidth} height={Math.max(0, inputHeight)} rx="3" tabIndex={0} role="img" aria-label={`${point.label}: ${number(point.input)} input tokens`}><title>{point.label} · Input: {number(point.input)} tokens</title></rect>
        <rect className="usage-bar-cache-read" x={barX} y={y(cacheReadTop)} width={barWidth} height={Math.max(0, cacheReadHeight)} rx="3" tabIndex={0} role="img" aria-label={`${point.label}: ${number(point.cacheRead)} cache read tokens`}><title>{point.label} · Cache read: {number(point.cacheRead)} tokens</title></rect>
        <rect className="usage-bar-cache-write" x={barX} y={y(cacheWriteTop)} width={barWidth} height={Math.max(0, cacheWriteHeight)} rx="3" tabIndex={0} role="img" aria-label={`${point.label}: ${number(point.cacheWrite)} cache creation tokens`}><title>{point.label} · Cache creation: {number(point.cacheWrite)} tokens</title></rect>
        <rect className="usage-bar-output" x={barX} y={y(outputTop)} width={barWidth} height={Math.max(0, outputHeight)} rx="3" tabIndex={0} role="img" aria-label={`${point.label}: ${number(point.output)} output tokens`}><title>{point.label} · Output: {number(point.output)} tokens</title></rect>
      </g>;
    }) : series.map((item) => {
      const d = item.values.map((value, index) => `${index === 0 ? "M" : "L"} ${x(index)} ${y(value)}`).join(" ");
      return <g key={item.id}><path className="usage-model-line" d={d} stroke={item.color} />{item.values.map((value, index) => <circle key={`${item.id}-${points[index].key}`} className="usage-point" cx={x(index)} cy={y(value)} r="4" fill={item.color} tabIndex={0} role="img" aria-label={`${points[index].label}: ${item.label}, ${number(value)} tokens`}><title>{points[index].label} · {item.label}: {number(value)} tokens</title></circle>)}</g>;
    })}
    {points.map((point, index) => index % labelEvery === 0 || index === points.length - 1 ? <text className="usage-axis-label" key={point.key} x={x(index)} y={height - 12} textAnchor="middle">{point.label}</text> : null)}
  </svg></div>;
}

function makePoints(rows: UsageAggregate[], startDate: string, endDate: string, granularity: Granularity, locale: string): ChartPoint[] {
  const start = parseDate(startDate);
  const end = parseDate(endDate);
  if (!start || !end || start > end) return [];
  const points = new Map<string, ChartPoint>();
  const makeDayIndex = (date: Date) => Math.floor(Date.UTC(date.getFullYear(), date.getMonth(), date.getDate()) / 86_400_000);
  if (granularity === "daily") {
    const current = new Date(start);
    while (current <= end) {
      const index = makeDayIndex(current);
      const key = String(index);
      points.set(key, { key, dayIndex: index, label: current.toLocaleDateString(locale, { month: "short", day: "numeric" }), input: 0, output: 0, cacheRead: 0, cacheWrite: 0, requests: 0, models: {}, modelLabels: {} });
      current.setDate(current.getDate() + 1);
    }
  } else {
    const current = new Date(start.getFullYear(), start.getMonth(), 1);
    while (current <= end) {
      const index = Math.floor(Date.UTC(current.getFullYear(), current.getMonth(), 1) / 86_400_000);
      const key = `${current.getFullYear()}-${String(current.getMonth() + 1).padStart(2, "0")}`;
      points.set(key, { key, dayIndex: index, label: current.toLocaleDateString(locale, { month: "short", year: "2-digit" }), input: 0, output: 0, cacheRead: 0, cacheWrite: 0, requests: 0, models: {}, modelLabels: {} });
      current.setMonth(current.getMonth() + 1);
    }
  }
  for (const row of rows) {
    const date = new Date(row.dayIndex * 86_400_000);
    const key = granularity === "daily" ? String(row.dayIndex) : `${date.getUTCFullYear()}-${String(date.getUTCMonth() + 1).padStart(2, "0")}`;
    const point = points.get(key);
    if (!point) continue;
    const input = uncachedInput(row);
    point.input += input;
    point.output += row.outputTokens;
    point.cacheRead += row.cachedInputTokens;
    point.cacheWrite += row.cacheWriteInputTokens;
    point.requests += row.requests;
    const modelKey = `${row.provider}\u0000${row.model}`;
    point.models[modelKey] = (point.models[modelKey] ?? 0) + input + row.cachedInputTokens + row.cacheWriteInputTokens + row.outputTokens;
    point.modelLabels[modelKey] = `${row.provider} · ${row.model}`;
  }
  return [...points.values()];
}

function makeSeries(points: ChartPoint[], grouping: Grouping): Series[] {
  if (grouping === "total") return [
    { id: "input", label: "Input tokens", color: "#81967a", kind: "input", values: points.map((point) => point.input) },
    { id: "cache-read", label: "Cache read", color: "#806d9a", kind: "cache-read", values: points.map((point) => point.cacheRead) },
    { id: "cache-write", label: "Cache creation", color: "#a86f4e", kind: "cache-write", values: points.map((point) => point.cacheWrite) },
    { id: "output", label: "Output tokens", color: "#c4b997", kind: "output", values: points.map((point) => point.output) },
  ];
  const totals = new Map<string, number>();
  for (const point of points) for (const [model, value] of Object.entries(point.models)) totals.set(model, (totals.get(model) ?? 0) + value);
  const ranked = [...totals.entries()].sort((a, b) => b[1] - a[1]);
  const labels = new Map(points.flatMap((point) => Object.entries(point.modelLabels)));
  const top = ranked.slice(0, 7).map(([model], index) => ({ id: model, label: labels.get(model) ?? model, color: modelColors[index], kind: "model" as const, values: points.map((point) => point.models[model] ?? 0) }));
  if (ranked.length > 7) top.push({ id: "other-models", label: "Other models", color: modelColors[7], kind: "model", values: points.map((point) => Object.entries(point.models).filter(([model]) => !ranked.slice(0, 7).some(([topModel]) => topModel === model)).reduce((sum, [, value]) => sum + value, 0)) });
  return top;
}

function presetTimestamps(preset: Exclude<RangePreset, "custom">): { start: number; end: number } {
  const end = Date.now();
  const hours = preset === "1h" ? 1 : preset === "1d" ? 24 : preset === "7d" ? 24 * 7 : 24 * 30;
  return { start: end - hours * 60 * 60 * 1000, end };
}

function todayString(): string { return dateString(new Date()); }
function dateString(date: Date): string { return `${date.getFullYear()}-${String(date.getMonth() + 1).padStart(2, "0")}-${String(date.getDate()).padStart(2, "0")}`; }
function parseDate(value: string): Date | null {
  const [year, month, day] = value.split("-").map(Number);
  if (!year || !month || !day) return null;
  const date = new Date(year, month - 1, day);
  return Number.isNaN(date.getTime()) ? null : date;
}
function localDateStart(value: string): number { const date = parseDate(value); return date ? date.getTime() : Number.NaN; }
function addDays(value: string, count: number): string { const date = parseDate(value); if (!date) return ""; date.setDate(date.getDate() + count); return dateString(date); }
function niceStep(value: number): number {
  if (value <= 0) return 1;
  const power = Math.pow(10, Math.floor(Math.log10(value)));
  const normalized = value / power;
  return (normalized <= 1 ? 1 : normalized <= 3 ? 2 : normalized <= 7 ? 5 : 10) * power;
}
function formatCompact(value: number, language: Language): string { return new Intl.NumberFormat(localeFor(language), { notation: value >= 10_000 ? "compact" : "standard", maximumFractionDigits: 1 }).format(value); }
function formatNumber(value: number, language: Language): string { return new Intl.NumberFormat(localeFor(language), { maximumFractionDigits: 0 }).format(value); }
function uncachedInput(row: UsageAggregate): number { return Math.max(0, row.inputTokens - row.cachedInputTokens - row.cacheWriteInputTokens); }
function messageOf(reason: unknown): string { return reason instanceof Error ? reason.message : String(reason); }
