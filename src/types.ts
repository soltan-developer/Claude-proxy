export type AuthStyle = "bearer" | "api_key";
export type Theme = "light" | "dark";
export type WebAccessMode = "disabled" | "auto" | "required";
export type WebProxyMode = "direct" | "manual";

export type ModelRoute = {
  routeId: string;
  displayName: string;
  upstreamModel: string;
  tier: string;
  contextWindow: number;
  maxOutputTokens: number;
  isDefault: boolean;
};

export type BridgeConfig = {
  providerName: string;
  baseUrl: string;
  apiKeyConfigured: boolean;
  authStyle: AuthStyle;
  upstreamModels: string[];
  routes: ModelRoute[];
  proxyPort: number;
  theme: Theme;
  setupComplete: boolean;
  autoTrimOnContextOverflow: boolean;
  claudeCodeMaxOutputTokens: number;
  webAccessMode: WebAccessMode;
  webProxyMode: WebProxyMode;
  webProxyUrl: string;
  webSearchUrl: string;
  webPreferredDomains: string[];
};

export type ConfigInput = Omit<BridgeConfig, "apiKeyConfigured"> & {
  apiKey?: string;
  clearApiKey?: boolean;
};

export type ProxyStatus = {
  running: boolean;
  host: string;
  port: number;
  configured: boolean;
  mappedRoutes: number;
};

export type UsageQuery = {
  startTimestamp: number;
  endTimestamp: number;
  utcOffsetMinutes: number;
};

export type UsageAggregate = {
  dayIndex: number;
  provider: string;
  model: string;
  requestedModel: string;
  inputTokens: number;
  outputTokens: number;
    cachedInputTokens: number;
    cacheWriteInputTokens: number;
    cacheStatsInputTokens: number;
    cacheStatsReportedRequests: number;
  requests: number;
  estimatedInputRequests: number;
  estimatedOutputRequests: number;
};

export type DiagnosticEntry = {
  timestamp: number;
  level: "info" | "warning" | "error";
  category: string;
  message: string;
};

export const emptyConfig: BridgeConfig = {
  providerName: "",
  baseUrl: "",
  apiKeyConfigured: false,
  authStyle: "bearer",
  upstreamModels: [],
  routes: [],
  proxyPort: 18181,
  theme: "light",
  setupComplete: false,
  autoTrimOnContextOverflow: true,
  claudeCodeMaxOutputTokens: 64000,
  webAccessMode: "disabled",
  webProxyMode: "direct",
  webProxyUrl: "",
  webSearchUrl: "https://html.duckduckgo.com/html/",
  webPreferredDomains: ["github.com", "raw.githubusercontent.com", "api.github.com"],
};

export const routeRoles = [
  { id: "claude-fable-5-1", title: "Fable 5.1", tier: "fable" },
  { id: "claude-opus-5-5", title: "Opus 5.5", tier: "opus" },
  { id: "claude-sonnet-5-5", title: "Sonnet 5.5", tier: "sonnet" },
  { id: "claude-haiku-4-5-20251001", title: "Haiku 4.5", tier: "haiku" },
] as const;
