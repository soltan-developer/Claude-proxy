import { invoke } from "@tauri-apps/api/core";
import type { BridgeConfig, ConfigInput, DiagnosticEntry, ProxyStatus, UsageAggregate, UsageQuery } from "../types";
import { emptyConfig } from "../types";

const storageKey = "claude-proxy-config";

export function isTauriRuntime(): boolean {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

export async function loadConfig(): Promise<BridgeConfig> {
  if (isTauriRuntime()) return invoke<BridgeConfig>("get_config");
  const raw = window.localStorage.getItem(storageKey);
  return raw ? { ...emptyConfig, ...(JSON.parse(raw) as Partial<BridgeConfig>) } : emptyConfig;
}

export async function saveConfig(input: ConfigInput): Promise<BridgeConfig> {
  if (isTauriRuntime()) return invoke<BridgeConfig>("save_config", { input });
  const { apiKey: _ignored, clearApiKey, ...publicConfig } = input;
  const current = await loadConfig();
  const next = {
    ...current,
    ...publicConfig,
    apiKeyConfigured: clearApiKey ? false : Boolean(input.apiKey?.trim()) || current.apiKeyConfigured,
  };
  window.localStorage.setItem(storageKey, JSON.stringify(next));
  return next;
}

export async function getProxyStatus(): Promise<ProxyStatus> {
  if (isTauriRuntime()) return invoke<ProxyStatus>("get_proxy_status");
  const config = await loadConfig();
  return { running: false, host: "127.0.0.1", port: config.proxyPort, configured: false, mappedRoutes: config.routes.length };
}

export async function getUsageReport(query: UsageQuery): Promise<UsageAggregate[]> {
  if (!isTauriRuntime()) return [];
  return invoke<UsageAggregate[]>("get_usage_report", { query });
}

export async function getDiagnostics(limit = 500): Promise<DiagnosticEntry[]> {
  if (!isTauriRuntime()) return [];
  return invoke<DiagnosticEntry[]>("get_diagnostics", { limit });
}

export async function clearDiagnostics(): Promise<void> {
  if (isTauriRuntime()) await invoke("clear_diagnostics");
}

export async function fetchModels(): Promise<string[]> {
  if (!isTauriRuntime()) throw new Error("Model discovery is available in the desktop app.");
  return invoke<string[]>("fetch_models");
}

export async function pingProvider(): Promise<string> {
  if (!isTauriRuntime()) throw new Error("Provider API checks are available in the desktop app.");
  return invoke<string>("ping_provider");
}

export async function quitApplication(): Promise<void> {
  if (isTauriRuntime()) await invoke("quit_application");
}

export async function startProxy(): Promise<ProxyStatus> {
  if (!isTauriRuntime()) throw new Error("The proxy is unavailable in the browser preview.");
  return invoke<ProxyStatus>("start_proxy");
}

export async function stopProxy(): Promise<ProxyStatus> {
  if (!isTauriRuntime()) throw new Error("The proxy is unavailable in the browser preview.");
  return invoke<ProxyStatus>("stop_proxy");
}

export async function testLocalRoute(): Promise<string> {
  if (!isTauriRuntime()) throw new Error("The local route check is available in the desktop app.");
  return invoke<string>("test_local_route");
}

export async function testWebAccess(): Promise<string> {
  if (!isTauriRuntime()) throw new Error("The web bridge check is available in the desktop app.");
  return invoke<string>("test_web_access");
}

export async function applyClaudeDesktop(): Promise<string> {
  if (!isTauriRuntime()) throw new Error("Claude Desktop setup is available in the desktop app.");
  return invoke<string>("apply_claude_desktop");
}

export async function applyClaudeCli(): Promise<string> {
  if (!isTauriRuntime()) throw new Error("Claude Code setup is available in the desktop app.");
  return invoke<string>("apply_claude_cli");
}

export async function openCreatorLink(): Promise<void> {
  if (isTauriRuntime()) await invoke("open_creator_link");
  else window.open("https://eitaa.com/salmanzadeh_ir", "_blank", "noopener,noreferrer");
}

// Fixed, synthetic prompts only. No user conversation content or Code workspace
// is sent; results distinguish a direct provider response from proxy routing.
export type ProviderProbeResult = {
  language: string;
  marker: string;
  passed: boolean;
  model: string;
  responseMs: number;
  toolCalls: number;
  reply: string;
};

export async function probeProviderFidelity(): Promise<ProviderProbeResult[]> {
  if (!isTauriRuntime()) throw new Error("The provider fidelity probe is available in the desktop app.");
  return invoke<ProviderProbeResult[]>("probe_provider_fidelity");
}
