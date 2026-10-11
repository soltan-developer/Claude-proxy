use std::{
    collections::{BTreeMap, HashSet},
    ffi::c_void,
    fs, io,
    io::Read,
    net::SocketAddr,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use crate::observability::{DiagnosticEntry, ObservabilityStore, UsageAggregate, UsageQuery};
use crate::v2::capabilities::{CapabilityMode, CapabilitySettings};
use crate::v2::client_output::ClientOutputGuard;
use crate::v2::context_shape::ContextShape;
use crate::v2::preflight::{check_chat_completions, stream_enabled, ContractMode};
use crate::v2::provider_response::validate_completion;
use crate::v2::stream_machine::{StreamContractError, StreamMachine};
use axum::{
    body::{Body, Bytes},
    extract::{DefaultBodyLimit, State},
    http::{header, HeaderMap, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use futures_util::StreamExt;
use reqwest::{redirect::Policy, Client, Proxy, Url};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tauri::{
    image::Image,
    menu::{Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    AppHandle, Manager, State as TauriState,
};
use tokio::sync::{oneshot, Mutex, OwnedSemaphorePermit, RwLock, Semaphore};

const LOCAL_HOST: &str = "127.0.0.1";
const DEFAULT_PORT: u16 = 18_181;
const PROFILE_ID: &str = "00000000-0000-4000-8000-000000181810";
const PROFILE_NAME: &str = "Claude Proxy";
const CLAUDE_DESKTOP_PREFIX: &str = "/claude-desktop";
const STATUS_TRAY_ID: &str = "proxy-status";
const UPSTREAM_CONCURRENCY: usize = 4;
const WEB_CONCURRENCY: usize = 4;
const DEFAULT_WEB_SEARCH_URL: &str = "https://html.duckduckgo.com/html/";
const WEB_TOOL_MAX_ROUNDS: usize = 4;
const WEB_FETCH_MAX_CHARS: usize = 20_000;
const WEB_RESPONSE_MAX_BYTES: usize = 2 * 1024 * 1024;
const DEFAULT_SHELL_ACCESS_MODE: &str = "disabled";
const SHELL_OUTPUT_MAX_CHARS: usize = 20_000;
const SHELL_COMMAND_MAX_CHARS: usize = 4_000;
const SHELL_DEFAULT_TIMEOUT_SECS: u64 = 60;
const SHELL_MAX_TIMEOUT_SECS: u64 = 300;
const SHELL_CONCURRENCY: usize = 2;
const DEFAULT_WEB_PREFERRED_DOMAINS: &[&str] =
    &["github.com", "raw.githubusercontent.com", "api.github.com"];
const WEB_BRIDGE_SYSTEM_MARKER: &str = "Claude Proxy local web bridge instructions:";
const SHELL_BRIDGE_SYSTEM_MARKER: &str = "Claude Proxy local shell bridge instructions:";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelRoute {
    pub route_id: String,
    pub display_name: String,
    pub upstream_model: String,
    pub tier: String,
    /// Optional provider-specific thinking switch. Legacy route records use
    /// an empty value, resolved according to the known upstream model.
    #[serde(default)]
    pub thinking_mode: String,
    #[serde(default)]
    pub context_window: u64,
    #[serde(default)]
    pub max_output_tokens: u64,
    #[serde(default)]
    pub is_default: bool,
}

#[derive(Debug, Clone)]
struct AppConfig {
    provider_name: String,
    base_url: String,
    api_key: String,
    auth_style: String,
    provider_network_mode: String,
    upstream_models: Vec<String>,
    routes: Vec<ModelRoute>,
    proxy_port: u16,
    theme: String,
    setup_complete: bool,
    auto_trim_on_context_overflow: bool,
    claude_code_max_output_tokens: u64,
    gateway_token: String,
    web_access_mode: String,
    web_proxy_mode: String,
    web_proxy_url: String,
    web_search_url: String,
    web_preferred_domains: Vec<String>,
    client_web_grounding_enabled: bool,
    client_web_allowed_domains: Vec<String>,
    shell_access_mode: String,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            provider_name: String::new(),
            base_url: String::new(),
            api_key: String::new(),
            auth_style: "bearer".into(),
            provider_network_mode: default_provider_network_mode(),
            upstream_models: Vec::new(),
            routes: Vec::new(),
            proxy_port: DEFAULT_PORT,
            theme: "light".into(),
            setup_complete: false,
            auto_trim_on_context_overflow: true,
            claude_code_max_output_tokens: 64_000,
            gateway_token: String::new(),
            web_access_mode: "disabled".into(),
            web_proxy_mode: "direct".into(),
            web_proxy_url: String::new(),
            web_search_url: DEFAULT_WEB_SEARCH_URL.into(),
            web_preferred_domains: DEFAULT_WEB_PREFERRED_DOMAINS
                .iter()
                .map(|domain| (*domain).to_string())
                .collect(),
            client_web_grounding_enabled: false,
            client_web_allowed_domains: Vec::new(),
            shell_access_mode: DEFAULT_SHELL_ACCESS_MODE.into(),
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct DiskConfig {
    #[serde(default)]
    provider_name: String,
    #[serde(default)]
    base_url: String,
    #[serde(default)]
    api_key_protected: String,
    #[serde(default = "default_auth_style")]
    auth_style: String,
    #[serde(default = "default_provider_network_mode")]
    provider_network_mode: String,
    #[serde(default)]
    upstream_models: Vec<String>,
    #[serde(default)]
    routes: Vec<ModelRoute>,
    #[serde(default = "default_port")]
    proxy_port: u16,
    #[serde(default = "default_theme")]
    theme: String,
    #[serde(default)]
    setup_complete: bool,
    #[serde(default = "default_auto_trim")]
    auto_trim_on_context_overflow: bool,
    #[serde(default = "default_claude_code_max_output_tokens")]
    claude_code_max_output_tokens: u64,
    #[serde(default)]
    gateway_token_protected: String,
    #[serde(default = "default_web_access_mode")]
    web_access_mode: String,
    #[serde(default = "default_web_proxy_mode")]
    web_proxy_mode: String,
    #[serde(default)]
    web_proxy_url: String,
    #[serde(default = "default_web_search_url")]
    web_search_url: String,
    #[serde(default = "default_web_preferred_domains")]
    web_preferred_domains: Vec<String>,
    #[serde(default)]
    client_web_grounding_enabled: bool,
    #[serde(default)]
    client_web_allowed_domains: Vec<String>,
    #[serde(default = "default_shell_access_mode")]
    shell_access_mode: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ConfigInput {
    provider_name: String,
    base_url: String,
    #[serde(default)]
    api_key: Option<String>,
    #[serde(default)]
    clear_api_key: bool,
    auth_style: String,
    #[serde(default = "default_provider_network_mode")]
    provider_network_mode: String,
    #[serde(default)]
    upstream_models: Vec<String>,
    #[serde(default)]
    routes: Vec<ModelRoute>,
    proxy_port: u16,
    theme: String,
    setup_complete: bool,
    #[serde(default = "default_auto_trim")]
    auto_trim_on_context_overflow: bool,
    #[serde(default = "default_claude_code_max_output_tokens")]
    claude_code_max_output_tokens: u64,
    #[serde(default = "default_web_access_mode")]
    web_access_mode: String,
    #[serde(default = "default_web_proxy_mode")]
    web_proxy_mode: String,
    #[serde(default)]
    web_proxy_url: String,
    #[serde(default = "default_web_search_url")]
    web_search_url: String,
    #[serde(default = "default_web_preferred_domains")]
    web_preferred_domains: Vec<String>,
    #[serde(default)]
    client_web_grounding_enabled: bool,
    #[serde(default)]
    client_web_allowed_domains: Vec<String>,
    #[serde(default = "default_shell_access_mode")]
    shell_access_mode: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct PublicConfig {
    provider_name: String,
    base_url: String,
    api_key_configured: bool,
    auth_style: String,
    provider_network_mode: String,
    upstream_models: Vec<String>,
    routes: Vec<ModelRoute>,
    proxy_port: u16,
    theme: String,
    setup_complete: bool,
    auto_trim_on_context_overflow: bool,
    claude_code_max_output_tokens: u64,
    web_access_mode: String,
    web_proxy_mode: String,
    web_proxy_url: String,
    web_search_url: String,
    web_preferred_domains: Vec<String>,
    client_web_grounding_enabled: bool,
    client_web_allowed_domains: Vec<String>,
    shell_access_mode: String,
}

impl From<&AppConfig> for PublicConfig {
    fn from(config: &AppConfig) -> Self {
        Self {
            provider_name: config.provider_name.clone(),
            base_url: config.base_url.clone(),
            api_key_configured: !config.api_key.is_empty(),
            auth_style: config.auth_style.clone(),
            provider_network_mode: config.provider_network_mode.clone(),
            upstream_models: config.upstream_models.clone(),
            routes: config.routes.clone(),
            proxy_port: config.proxy_port,
            theme: config.theme.clone(),
            setup_complete: config.setup_complete,
            auto_trim_on_context_overflow: config.auto_trim_on_context_overflow,
            claude_code_max_output_tokens: config.claude_code_max_output_tokens,
            web_access_mode: config.web_access_mode.clone(),
            web_proxy_mode: config.web_proxy_mode.clone(),
            web_proxy_url: config.web_proxy_url.clone(),
            web_search_url: config.web_search_url.clone(),
            web_preferred_domains: config.web_preferred_domains.clone(),
            client_web_grounding_enabled: config.client_web_grounding_enabled,
            client_web_allowed_domains: config.client_web_allowed_domains.clone(),
            shell_access_mode: config.shell_access_mode.clone(),
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ProxyStatus {
    running: bool,
    host: String,
    port: u16,
    configured: bool,
    mapped_routes: usize,
}

struct RuntimeHandle {
    id: uuid::Uuid,
    stop: oneshot::Sender<()>,
}

#[derive(Clone)]
struct BridgeState {
    app: AppHandle,
    config: Arc<RwLock<AppConfig>>,
    runtime: Arc<Mutex<Option<RuntimeHandle>>>,
    client: Client,
    direct_provider_client: Client,
    upstream_clients: Arc<Vec<Client>>,
    upstream_cursor: Arc<AtomicUsize>,
    upstream_slots: Arc<Semaphore>,
    web_slots: Arc<Semaphore>,
    shell_slots: Arc<Semaphore>,
    config_path: PathBuf,
    observability: ObservabilityStore,
    active_requests: Arc<AtomicUsize>,
}

#[derive(Clone)]
struct ServerState {
    config: Arc<RwLock<AppConfig>>,
    direct_provider_client: Client,
    upstream_clients: Arc<Vec<Client>>,
    upstream_cursor: Arc<AtomicUsize>,
    upstream_slots: Arc<Semaphore>,
    web_slots: Arc<Semaphore>,
    shell_slots: Arc<Semaphore>,
    observability: ObservabilityStore,
    active_requests: Arc<AtomicUsize>,
    v2_contract_mode: ContractMode,
    v2_stream_enabled: bool,
    v2_output_guard_enabled: bool,
    v2_capabilities: CapabilitySettings,
}

impl ServerState {
    fn next_upstream_client(&self) -> Client {
        let index =
            self.upstream_cursor.fetch_add(1, Ordering::Relaxed) % self.upstream_clients.len();
        self.upstream_clients[index].clone()
    }
}

fn default_auth_style() -> String {
    "bearer".into()
}
fn default_theme() -> String {
    "light".into()
}
fn default_provider_network_mode() -> String {
    "direct".into()
}

fn normalize_provider_network_mode(value: &str) -> &'static str {
    if value.trim().eq_ignore_ascii_case("system") {
        "system"
    } else {
        "direct"
    }
}

fn default_port() -> u16 {
    DEFAULT_PORT
}
fn default_auto_trim() -> bool {
    true
}
fn default_web_access_mode() -> String {
    "disabled".into()
}
fn default_web_proxy_mode() -> String {
    "direct".into()
}
fn default_web_search_url() -> String {
    DEFAULT_WEB_SEARCH_URL.into()
}
fn default_web_preferred_domains() -> Vec<String> {
    DEFAULT_WEB_PREFERRED_DOMAINS
        .iter()
        .map(|domain| (*domain).to_string())
        .collect()
}

fn default_shell_access_mode() -> String {
    DEFAULT_SHELL_ACCESS_MODE.into()
}

fn default_claude_code_max_output_tokens() -> u64 {
    64_000
}

fn normalize_claude_code_max_output_tokens(value: u64) -> u64 {
    value.clamp(1, 128_000)
}

fn normalize_shell_access_mode(value: &str) -> &'static str {
    match value.trim().to_ascii_lowercase().as_str() {
        "auto" => "auto",
        _ => "disabled",
    }
}

fn normalize_web_access_mode(value: &str) -> &'static str {
    match value.trim().to_ascii_lowercase().as_str() {
        "auto" => "auto",
        "required" => "required",
        _ => "disabled",
    }
}

fn normalize_web_proxy_mode(value: &str) -> &'static str {
    if value.trim().eq_ignore_ascii_case("manual") {
        "manual"
    } else {
        "direct"
    }
}

fn normalize_web_proxy_url(value: &str) -> Result<String, String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Ok(String::new());
    }
    let candidate = if trimmed.contains("://") {
        trimmed.to_string()
    } else {
        format!("http://{trimmed}")
    };
    let parsed = Url::parse(&candidate)
        .map_err(|error| format!("The web proxy address is invalid: {error}"))?;
    if !matches!(parsed.scheme(), "http" | "https" | "socks5" | "socks5h")
        || parsed.host_str().is_none()
        || parsed.port_or_known_default().is_none()
    {
        return Err(
            "The web proxy must be an HTTP, HTTPS, SOCKS5, or SOCKS5H address such as 127.0.0.1:10808."
                .into(),
        );
    }
    Ok(candidate)
}

fn normalize_web_search_url(value: &str) -> Result<String, String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Ok(DEFAULT_WEB_SEARCH_URL.into());
    }
    let parsed = Url::parse(trimmed)
        .map_err(|error| format!("The web search address is invalid: {error}"))?;
    if parsed.scheme() != "https"
        || parsed.host_str().is_none()
        || !parsed.username().is_empty()
        || parsed.password().is_some()
    {
        return Err("The web search address must be an HTTPS URL.".into());
    }
    Ok(trimmed
        .trim_end_matches('?')
        .trim_end_matches('&')
        .to_string())
}

fn normalize_web_preferred_domains(domains: Vec<String>) -> Vec<String> {
    let mut normalized = Vec::new();
    let mut seen = HashSet::new();
    for raw in domains.into_iter().take(64) {
        let domain = raw.trim().trim_end_matches('.').to_ascii_lowercase();
        if domain.is_empty()
            || domain.len() > 253
            || domain
                .chars()
                .any(|character| matches!(character, '/' | ':' | '?' | '#' | '@' | '*'))
            || domain.chars().any(char::is_whitespace)
        {
            continue;
        }
        let Ok(url) = Url::parse(&format!("https://{domain}/")) else {
            continue;
        };
        let Some(host) = url.host_str().map(str::to_ascii_lowercase) else {
            continue;
        };
        if url.path() != "/"
            || url.port().is_some()
            || host.parse::<std::net::IpAddr>().is_ok()
            || host == "localhost"
            || host.ends_with(".localhost")
            || host.ends_with(".local")
            || host.ends_with(".internal")
            || !host.contains('.')
            || !seen.insert(host.clone())
        {
            continue;
        }
        normalized.push(host);
        if normalized.len() == 32 {
            break;
        }
    }
    normalized
}

fn has_direct_public_url(text: &str) -> bool {
    ["https://", "http://"].iter().any(|scheme| {
        text.match_indices(scheme).any(|(start, _)| {
            let candidate = text[start..]
                .split_whitespace()
                .next()
                .unwrap_or_default()
                .trim_matches(|character: char| {
                    matches!(character, '<' | '>' | '"' | '\'' | '(' | '[' | '{')
                })
                .trim_end_matches(|character: char| {
                    matches!(
                        character,
                        '.' | ',' | ';' | ':' | '!' | '?' | ')' | ']' | '}'
                    )
                });
            validate_web_url(candidate).is_ok()
        })
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum WebIntent {
    Automatic,
    Search,
    Fetch,
    ReuseContext,
    NoWeb,
}

fn mentions_github_or_repository(text: &str) -> bool {
    let text = text.to_ascii_lowercase();
    [
        "github",
        "repo",
        "repository",
        "codebase",
        "گیت‌هاب",
        "گیت هاب",
        "گیتهاب",
        "ریپو",
        "مخزن",
    ]
    .iter()
    .any(|term| text.contains(term))
}

fn asks_to_revisit_repository(text: &str) -> bool {
    let text = text.to_ascii_lowercase();
    [
        "implement",
        "apply",
        "modify",
        "edit",
        "update",
        "patch",
        "fix",
        "change",
        "refactor",
        "review",
        "inspect",
        "analyze",
        "analyse",
        "check the repo",
        "open the repo",
        "read the repo",
        "make the changes",
        "look at the repository",
        "پیاده‌سازی",
        "پیاده سازی",
        "اعمال کن",
        "اصلاح کن",
        "درست کن",
        "ویرایش کن",
        "تغییر بده",
        "به‌روزرسانی",
        "بروزرسانی",
        "بررسی کن",
        "نگاه کن",
        "باز کن",
        "بخون",
    ]
    .iter()
    .any(|term| text.contains(term))
}

fn asks_for_new_search(text: &str) -> bool {
    let text = text.to_ascii_lowercase();
    [
        "search",
        "find",
        "look up",
        "latest",
        "release",
        "جستجو",
        "جست‌وجو",
        "پیدا کن",
        "آخرین",
        "نسخهٔ جدید",
        "نسخه جدید",
    ]
    .iter()
    .any(|term| text.contains(term))
}

fn asks_for_contextual_opinion(text: &str) -> bool {
    let text = text.to_ascii_lowercase();
    [
        "what do you think",
        "your opinion",
        "why",
        "explain",
        "summarize",
        "summary",
        "نظرت",
        "نظر چیه",
        "تحلیل",
        "توضیح بده",
        "خلاصه",
    ]
    .iter()
    .any(|term| text.contains(term))
}

fn asks_for_fresh_information(text: &str) -> bool {
    let text = text.to_lowercase();
    [
        "what's the",
        "what is the",
        "who is",
        "who's",
        "where is",
        "when is",
        "latest",
        "last ",
        "news",
        "today",
        "right now",
        "current",
        "up to date",
        "recent",
        "price",
        "score",
        "weather",
        "release date",
        "release notes",
        "releases",
        "updated",
        "update on",
        "just happened",
        "امروز",
        "اکنون",
        "فعلا",
        "فعلاً",
        "اخیر",
        "تازه",
        "آخرین",
        "به‌روز",
        "به روز",
        "بروز",
        "قیمت",
        "خبر",
        "نتیجهٔ بازی",
        "نتیجه بازی",
        "نسخهٔ جدید",
        "نسخه جدید",
    ]
    .iter()
    .any(|term| text.contains(term))
}

// The client always owns its declared tools, regardless of the experimental
// V2 contract flag. Inserting a provider-side search tool (or forced choice)
// alongside Claude Code/Cowork tools steals control of the user's request.
fn client_tools_take_precedence(request: &Value) -> bool {
    request
        .get("tools")
        .and_then(Value::as_array)
        .is_some_and(|tools| !tools.is_empty())
}

// Cowork/Claude Code own their tool definitions and their initial session
// bootstrap messages. A local heuristic must not silently remove any of that
// conversation to fit a configured context-window approximation. On overflow
// we must return a visible capacity error instead.
fn may_auto_trim_context(config: &AppConfig, client_tools_owned: bool) -> bool {
    config.auto_trim_on_context_overflow && !client_tools_owned
}

// When a Cowork/Code tool schema fills the configured context window, a
// one-token response cap masquerades as successful generation and may start
// pointless follow-up tool loops. Reject that estimate rather than sending
// a silently crippled agent request.
fn agent_output_budget_collapsed(request: &Value, payload: &Value, context_window: u64) -> bool {
    context_window > 0
        && request
            .get("max_tokens")
            .and_then(Value::as_u64)
            .is_some_and(|requested| requested > 1)
        && payload.get("max_tokens").and_then(Value::as_u64) == Some(1)
}

// Control-plane text sometimes travels inside a user content block (for
// example Claude Code's <system-reminder> about the working directory).
// It must be forwarded unmodified to the provider, but must NEVER authorize
// a local web search. Strip only for optional intent classification.
fn user_text_for_web_intent(raw: &str) -> String {
    let mut cleaned = raw.to_owned();
    for tag in ["system-reminder", "environment_context"] {
        let opening = format!("<{tag}>");
        let closing = format!("</{tag}>");
        while let Some(start) = cleaned.find(&opening) {
            if let Some(end) = cleaned[start + opening.len()..].find(&closing) {
                let end = start + opening.len() + end + closing.len();
                cleaned.replace_range(start..end, "");
            } else {
                // Unclosed control metadata is never evidence of user intent.
                cleaned.truncate(start);
                break;
            }
        }
    }
    cleaned.trim().to_owned()
}

fn is_simple_greeting_request(text: &str) -> bool {
    matches!(
        user_text_for_web_intent(text).trim(),
        "سلام" | "سلام!" | "سلام؟" | "hello" | "hi" | "Hi" | "Hello"
    )
}

fn classify_web_intent(request: &Value) -> WebIntent {
    let Some(messages) = request.get("messages").and_then(Value::as_array) else {
        return WebIntent::NoWeb;
    };
    let latest_user_text = messages
        .iter()
        .rev()
        .find(|message| message.get("role").and_then(Value::as_str) == Some("user"))
        .map(|message| {
            user_text_for_web_intent(&anthropic_text(
                message.get("content").unwrap_or(&Value::Null),
            ))
        })
        .unwrap_or_default();
    if latest_user_text.is_empty() {
        return WebIntent::NoWeb;
    }
    if has_direct_public_url(&latest_user_text) {
        return WebIntent::Fetch;
    }
    let github_was_in_chat = messages.iter().any(|message| {
        message.get("role").and_then(Value::as_str) == Some("user")
            && mentions_github_or_repository(&user_text_for_web_intent(&anthropic_text(
                message.get("content").unwrap_or(&Value::Null),
            )))
    });
    // Each new turn determines tool intent. An old GitHub mention is context,
    // not permission to inject web tools into a new greeting or a language-only
    // instruction (which can otherwise trigger a fresh unrelated search).
    // Preserve explicit follow-ups about an earlier repository.
    let current_repo_intent = mentions_github_or_repository(&latest_user_text);
    let current_fresh_intent = asks_for_fresh_information(&latest_user_text);
    let explicit_repo_followup = github_was_in_chat
        && (asks_to_revisit_repository(&latest_user_text)
            || asks_for_contextual_opinion(&latest_user_text)
            || asks_for_new_search(&latest_user_text));
    // Mentioning a repository in environment metadata or conversational
    // context is not an instruction to search it. Require a current action,
    // a public URL, or a fresh-information question.
    let has_repository_action = (github_was_in_chat || current_repo_intent)
        && (asks_to_revisit_repository(&latest_user_text)
            || asks_for_new_search(&latest_user_text));
    let asks_fresh_question = current_fresh_intent
        && (latest_user_text.contains('?')
            || latest_user_text.contains('؟')
            || asks_for_new_search(&latest_user_text)
            || latest_user_text.to_ascii_lowercase().contains("tell me")
            || latest_user_text.contains("بگو"));
    if !(asks_fresh_question || has_repository_action || explicit_repo_followup) {
        return WebIntent::NoWeb;
    }
    if github_was_in_chat && asks_to_revisit_repository(&latest_user_text) {
        return WebIntent::Fetch;
    }
    if github_was_in_chat && asks_for_contextual_opinion(&latest_user_text) {
        return WebIntent::ReuseContext;
    }
    if mentions_github_or_repository(&latest_user_text)
        || (github_was_in_chat && asks_for_new_search(&latest_user_text))
    {
        return WebIntent::Search;
    }
    WebIntent::Automatic
}

fn load_config(path: &Path) -> AppConfig {
    let Ok(raw) = fs::read_to_string(path) else {
        return AppConfig::default();
    };
    let Ok(disk) = serde_json::from_str::<DiskConfig>(&raw) else {
        return AppConfig::default();
    };
    AppConfig {
        provider_name: disk.provider_name,
        base_url: disk.base_url,
        api_key: unprotect_secret(&disk.api_key_protected).unwrap_or_default(),
        auth_style: disk.auth_style,
        provider_network_mode: normalize_provider_network_mode(&disk.provider_network_mode).into(),
        upstream_models: disk.upstream_models,
        routes: normalize_routes(disk.routes),
        proxy_port: if disk.proxy_port == 0 {
            DEFAULT_PORT
        } else {
            disk.proxy_port
        },
        theme: if disk.theme == "dark" {
            "dark"
        } else {
            "light"
        }
        .into(),
        setup_complete: disk.setup_complete,
        auto_trim_on_context_overflow: disk.auto_trim_on_context_overflow,
        claude_code_max_output_tokens: normalize_claude_code_max_output_tokens(
            disk.claude_code_max_output_tokens,
        ),
        gateway_token: unprotect_secret(&disk.gateway_token_protected).unwrap_or_default(),
        web_access_mode: normalize_web_access_mode(&disk.web_access_mode).into(),
        web_proxy_mode: normalize_web_proxy_mode(&disk.web_proxy_mode).into(),
        web_proxy_url: normalize_web_proxy_url(&disk.web_proxy_url).unwrap_or_default(),
        web_search_url: normalize_web_search_url(&disk.web_search_url)
            .unwrap_or_else(|_| DEFAULT_WEB_SEARCH_URL.into()),
        web_preferred_domains: normalize_web_preferred_domains(disk.web_preferred_domains),
        client_web_grounding_enabled: disk.client_web_grounding_enabled,
        client_web_allowed_domains: normalize_web_preferred_domains(disk.client_web_allowed_domains),
        shell_access_mode: normalize_shell_access_mode(&disk.shell_access_mode).into(),
    }
}

fn persist_config(path: &Path, config: &AppConfig) -> Result<(), String> {
    let disk = DiskConfig {
        provider_name: config.provider_name.clone(),
        base_url: config.base_url.clone(),
        api_key_protected: protect_secret(&config.api_key)?,
        auth_style: config.auth_style.clone(),
        provider_network_mode: config.provider_network_mode.clone(),
        upstream_models: config.upstream_models.clone(),
        routes: config.routes.clone(),
        proxy_port: config.proxy_port,
        theme: config.theme.clone(),
        setup_complete: config.setup_complete,
        auto_trim_on_context_overflow: config.auto_trim_on_context_overflow,
        claude_code_max_output_tokens: config.claude_code_max_output_tokens,
        gateway_token_protected: protect_secret(&config.gateway_token)?,
        web_access_mode: config.web_access_mode.clone(),
        web_proxy_mode: config.web_proxy_mode.clone(),
        web_proxy_url: config.web_proxy_url.clone(),
        web_search_url: config.web_search_url.clone(),
        web_preferred_domains: config.web_preferred_domains.clone(),
        client_web_grounding_enabled: config.client_web_grounding_enabled,
        client_web_allowed_domains: config.client_web_allowed_domains.clone(),
        shell_access_mode: config.shell_access_mode.clone(),
    };
    let parent = path
        .parent()
        .ok_or_else(|| "The configuration storage path is invalid.".to_string())?;
    fs::create_dir_all(parent)
        .map_err(|error| format!("Could not create the settings folder: {error}"))?;
    let body = serde_json::to_vec_pretty(&disk).map_err(|error| error.to_string())?;
    write_atomic(path, &body)
}

fn write_atomic(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let mut temp = path.as_os_str().to_os_string();
    temp.push(format!(".{}.tmp", uuid::Uuid::new_v4().simple()));
    let temp_path = PathBuf::from(temp);
    fs::write(&temp_path, bytes)
        .map_err(|error| format!("Could not write the temporary file: {error}"))?;
    if let Err(error) = fs::rename(&temp_path, path) {
        let _ = fs::remove_file(&temp_path);
        return Err(format!(
            "Could not safely replace the settings file: {error}"
        ));
    }
    Ok(())
}

const PUBLISHED_ROUTE_IDS: &[(&str, &str, &str)] = &[
    ("claude-fable-5-1", "Fable 5.1", "fable"),
    ("claude-opus-5-5", "Opus 5.5", "opus"),
    ("claude-sonnet-5-5", "Sonnet 5.5", "sonnet"),
    ("claude-haiku-4-5-20251001", "Haiku 4.5", "haiku"),
];

fn canonical_route_id(route: &str) -> Option<&'static str> {
    let route = route.trim().to_ascii_lowercase();
    if let Some((id, _, _)) = PUBLISHED_ROUTE_IDS.iter().find(|(id, _, _)| *id == route) {
        return Some(id);
    }

    // Migrate older saved role aliases to the current four published roles. The
    // provider model stays untouched, so an existing mapping is not lost on upgrade.
    if route
        .strip_prefix("claude-fable-")
        .is_some_and(|tail| !tail.is_empty())
    {
        Some("claude-fable-5-1")
    } else if route
        .strip_prefix("claude-opus-")
        .is_some_and(|tail| !tail.is_empty())
    {
        Some("claude-opus-5-5")
    } else if route
        .strip_prefix("claude-sonnet-")
        .is_some_and(|tail| !tail.is_empty())
        || route
            .strip_prefix("claude-3-7-sonnet-")
            .is_some_and(|tail| !tail.is_empty())
    {
        Some("claude-sonnet-5-5")
    } else if route
        .strip_prefix("claude-haiku-")
        .is_some_and(|tail| !tail.is_empty())
        || route
            .strip_prefix("claude-3-5-haiku-")
            .is_some_and(|tail| !tail.is_empty())
        || route
            .strip_prefix("claude-3-haiku-")
            .is_some_and(|tail| !tail.is_empty())
    {
        Some("claude-haiku-4-5-20251001")
    } else {
        None
    }
}

fn route_metadata(route_id: &str) -> Option<(&'static str, &'static str)> {
    let canonical = canonical_route_id(route_id)?;
    PUBLISHED_ROUTE_IDS
        .iter()
        .find(|(id, _, _)| *id == canonical)
        .map(|(_, title, tier)| (*title, *tier))
}

fn known_qwen38_model(model: &str) -> bool {
    let name = model.trim().to_ascii_lowercase();
    name == "qwen38" || name == "qwen38-sglang"
}

fn effective_thinking_mode(route: &ModelRoute) -> &'static str {
    match route.thinking_mode.trim().to_ascii_lowercase().as_str() {
        "disabled" => "disabled",
        "enabled" => "enabled",
        "inherit" => "inherit",
        // Qwen38 provider's documented default is non-thinking for our route
        // unless the operator explicitly opts in. Other models are unaffected.
        _ if known_qwen38_model(&route.upstream_model) => "disabled",
        _ => "inherit",
    }
}

fn normalize_routes(routes: Vec<ModelRoute>) -> Vec<ModelRoute> {
    let mut normalized = Vec::new();
    let mut seen_ids = HashSet::new();
    for mut route in routes {
        if !is_claude_safe_route(&route.route_id) {
            continue;
        }
        let Some(canonical_id) = canonical_route_id(&route.route_id) else {
            continue;
        };
        if route.upstream_model.trim().is_empty() || !seen_ids.insert(canonical_id) {
            continue;
        }
        route.route_id = canonical_id.to_string();
        route.display_name = route.display_name.trim().to_string();
        route.upstream_model = route.upstream_model.trim().to_string();
        route.thinking_mode = effective_thinking_mode(&route).into();
        if let Some((_, tier)) = route_metadata(canonical_id) {
            route.tier = tier.to_string();
        }
        normalized.push(route);
    }

    let mut used_default = false;
    for route in &mut normalized {
        if used_default {
            route.is_default = false;
        }
        if route.is_default {
            used_default = true;
        }
    }
    if !normalized.is_empty() && !normalized.iter().any(|route| route.is_default) {
        normalized[0].is_default = true;
    }
    normalized
}

fn is_claude_safe_route(route: &str) -> bool {
    canonical_route_id(route).is_some()
}

fn route_labels(routes: &[ModelRoute]) -> Vec<String> {
    let mut used = HashSet::new();
    routes
        .iter()
        .map(|route| {
            let base = if route.display_name.is_empty() {
                route_metadata(&route.route_id)
                    .map(|(title, _)| format!("{title} · {}", route.upstream_model))
                    .unwrap_or_else(|| route.route_id.clone())
            } else {
                route.display_name.clone()
            };
            let key = base.to_ascii_lowercase();
            if used.insert(key) {
                base
            } else {
                let mut candidate = format!("{base} · {}", route.route_id);
                let mut suffix = 2;
                while !used.insert(candidate.to_ascii_lowercase()) {
                    candidate = format!("{base} · {}-{suffix}", route.route_id);
                    suffix += 1;
                }
                candidate
            }
        })
        .collect()
}

fn configured(config: &AppConfig) -> bool {
    Url::parse(config.base_url.trim()).is_ok_and(|url| matches!(url.scheme(), "http" | "https"))
        && !config.api_key.is_empty()
        && !normalize_routes(config.routes.clone()).is_empty()
}

fn status_from(config: &AppConfig, running: bool) -> ProxyStatus {
    ProxyStatus {
        running,
        host: LOCAL_HOST.into(),
        port: config.proxy_port,
        configured: configured(config),
        mapped_routes: config.routes.len(),
    }
}

fn status_icon(active: bool) -> Image<'static> {
    let rgba = if active {
        include_bytes!("../icons/proxy-active.rgba")
    } else {
        include_bytes!("../icons/proxy-offline.rgba")
    };
    Image::new_owned(rgba.to_vec(), 128, 128)
}

fn update_status_icon(app: &AppHandle, active: bool) {
    let icon = status_icon(active);
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.set_icon(icon.clone());
    }
    if let Some(tray) = app.tray_by_id(STATUS_TRAY_ID) {
        let _ = tray.set_icon(Some(icon));
    }
}

fn ensure_gateway_token(config: &mut AppConfig) -> String {
    if config.gateway_token.is_empty() {
        config.gateway_token = format!("qbs2-{}", uuid::Uuid::new_v4().simple());
    }
    config.gateway_token.clone()
}

#[tauri::command]
async fn get_config(state: TauriState<'_, BridgeState>) -> Result<PublicConfig, String> {
    Ok(PublicConfig::from(&*state.config.read().await))
}

#[tauri::command]
async fn get_usage_report(
    state: TauriState<'_, BridgeState>,
    query: UsageQuery,
) -> Result<Vec<UsageAggregate>, String> {
    state.observability.usage(query)
}

#[tauri::command]
async fn get_diagnostics(
    state: TauriState<'_, BridgeState>,
    limit: usize,
) -> Result<Vec<DiagnosticEntry>, String> {
    state.observability.logs(limit)
}

#[tauri::command]
async fn clear_diagnostics(state: TauriState<'_, BridgeState>) -> Result<(), String> {
    state.observability.clear_logs()
}

#[tauri::command]
async fn save_config(
    app: AppHandle,
    state: TauriState<'_, BridgeState>,
    input: ConfigInput,
) -> Result<PublicConfig, String> {
    let previous = state.config.read().await.clone();
    let was_running = state.runtime.lock().await.is_some();
    let mut config = previous.clone();
    config.provider_name = input.provider_name.trim().to_string();
    config.base_url = input.base_url.trim().trim_end_matches('/').to_string();
    if input.clear_api_key {
        config.api_key.clear();
    } else if let Some(key) = input.api_key.filter(|key| !key.trim().is_empty()) {
        config.api_key = key.trim().to_string();
    }
    config.auth_style = if input.auth_style == "api_key" {
        "api_key"
    } else {
        "bearer"
    }
    .into();
    config.provider_network_mode =
        normalize_provider_network_mode(&input.provider_network_mode).into();
    config.upstream_models = input
        .upstream_models
        .into_iter()
        .map(|model| model.trim().to_string())
        .filter(|model| !model.is_empty())
        .collect();
    config.routes = normalize_routes(input.routes);
    config.proxy_port = if input.proxy_port == 0 {
        DEFAULT_PORT
    } else {
        input.proxy_port
    };
    config.theme = if input.theme == "dark" {
        "dark"
    } else {
        "light"
    }
    .into();
    config.setup_complete = input.setup_complete;
    config.auto_trim_on_context_overflow = input.auto_trim_on_context_overflow;
    config.claude_code_max_output_tokens =
        normalize_claude_code_max_output_tokens(input.claude_code_max_output_tokens);
    config.web_access_mode = normalize_web_access_mode(&input.web_access_mode).into();
    config.web_proxy_mode = normalize_web_proxy_mode(&input.web_proxy_mode).into();
    config.web_proxy_url = normalize_web_proxy_url(&input.web_proxy_url)?;
    config.web_search_url = normalize_web_search_url(&input.web_search_url)?;
    config.web_preferred_domains = normalize_web_preferred_domains(input.web_preferred_domains);
    config.client_web_allowed_domains =
        normalize_web_preferred_domains(input.client_web_allowed_domains);
    config.client_web_grounding_enabled = input.client_web_grounding_enabled;
    if config.client_web_grounding_enabled
        && (config.web_access_mode == "disabled" || config.client_web_allowed_domains.is_empty())
    {
        return Err("Enable Web access and enter at least one authorized public hostname before enabling client web grounding.".into());
    }
    config.shell_access_mode = normalize_shell_access_mode(&input.shell_access_mode).into();
    if config.web_access_mode != "disabled"
        && config.web_proxy_mode == "manual"
        && config.web_proxy_url.is_empty()
    {
        return Err("Enter a web proxy address or choose Direct connection.".into());
    }
    if let Err(error) = persist_config(&state.config_path, &config) {
        state.observability.log(
            "error",
            "settings",
            "Could not save provider settings to local storage.",
        );
        return Err(error);
    }
    *state.config.write().await = config.clone();
    if was_running && (previous.proxy_port != config.proxy_port || !configured(&config)) {
        stop_proxy_inner(state.inner()).await;
        if configured(&config) {
            if let Err(error) = start_proxy_inner(state.inner()).await {
                persist_config(&state.config_path, &previous)?;
                *state.config.write().await = previous.clone();
                let _ = start_proxy_inner(state.inner()).await;
                state.observability.log("error", "settings", "A settings change could not restart the proxy; the previous settings were restored.");
                return Err(format!(
                    "The new port was unavailable. The previous settings and proxy have been restored. Details: {error}"
                ));
            }
        }
    }
    update_status_icon(&app, state.runtime.lock().await.is_some());
    state
        .observability
        .log("info", "settings", "Provider and model settings saved.");
    Ok(PublicConfig::from(&config))
}

#[tauri::command]
async fn get_proxy_status(state: TauriState<'_, BridgeState>) -> Result<ProxyStatus, String> {
    let config = state.config.read().await.clone();
    Ok(status_from(&config, state.runtime.lock().await.is_some()))
}

#[tauri::command]
async fn fetch_models(state: TauriState<'_, BridgeState>) -> Result<Vec<String>, String> {
    let requested_config = state.config.read().await.clone();
    let models = match fetch_upstream_models(
        &state.client,
        &state.direct_provider_client,
        &requested_config,
        &state.observability,
    )
    .await
    {
        Ok(models) => models,
        Err(error) => {
            state.observability.log("error", "model discovery", "Model discovery failed. Review the provider URL, authentication, and model discovery endpoint.");
            return Err(error);
        }
    };
    let mut config = state.config.write().await;
    if config.base_url != requested_config.base_url
        || config.api_key != requested_config.api_key
        || config.auth_style != requested_config.auth_style
        || config.provider_network_mode != requested_config.provider_network_mode
    {
        return Err(
            "Provider settings changed while models were being fetched. Please try again.".into(),
        );
    }
    let mut updated = config.clone();
    updated.upstream_models = models.clone();
    persist_config(&state.config_path, &updated)?;
    *config = updated;
    Ok(models)
}

// TCP connect measures DNS + TCP handshake, not the provider's model-list
// processing time and not ICMP latency. Never goes through the Web Search VPN.
async fn measure_direct_provider_tcp_ms(provider_url: &str) -> Option<u128> {
    let url = Url::parse(provider_url).ok()?;
    let host = url.host_str()?.to_owned();
    let port = url.port_or_known_default()?;
    let started = Instant::now();
    match tokio::time::timeout(
        Duration::from_secs(8),
        tokio::net::TcpStream::connect((host.as_str(), port)),
    )
    .await
    {
        Ok(Ok(stream)) => {
            drop(stream);
            Some(started.elapsed().as_millis())
        }
        _ => None,
    }
}

#[tauri::command]
async fn ping_provider(state: TauriState<'_, BridgeState>) -> Result<String, String> {
    let config = state.config.read().await.clone();
    if config.base_url.trim().is_empty() {
        return Err("Enter a provider URL before checking its API.".into());
    }
    let url = upstream_url(&config.base_url, "models")?;
    let mut request = state.client.get(&url).timeout(Duration::from_secs(15));
    request = add_upstream_auth(request, &config)?;
    let started = Instant::now();
    // A VPN-only operator may explicitly request system proxy mode. In that
    // case do not leak a separate direct TCP probe to the provider.
    let direct_mode =
        ProviderProxyMode::from_setting(&config.provider_network_mode) == ProviderProxyMode::Direct;
    let (tcp_ms, response) = tokio::join!(
        async {
            if direct_mode {
                measure_direct_provider_tcp_ms(&url).await
            } else {
                None
            }
        },
        send_provider_with_fallback(
            request,
            &state.direct_provider_client,
            &state.observability,
            &config,
        )
    );
    let response = match response {
        Ok(response) => response,
        Err(error) => {
            state.observability.log(
                "error",
                "provider ping",
                "Provider API check failed before receiving an HTTP response.",
            );
            return Err(diagnose_provider_transport(&url, &config, &error).await);
        }
    };
    let status = response.status();
    let elapsed = started.elapsed().as_millis();
    if !status.is_success() {
        state.observability.log(
            "warning",
            "provider ping",
            &format!("Provider API ping returned HTTP {}.", status.as_u16()),
        );
        return Err(format!(
            "Provider API responded with HTTP {} in {elapsed} ms.",
            status.as_u16()
        ));
    }
    let tcp_label = tcp_ms
        .map(|ms| format!("{ms} ms (DNS + TCP connection)"))
        .unwrap_or_else(|| "unavailable".to_string());
    state.observability.log(
        "info",
        "provider ping",
        &format!(
            "Direct provider TCP connect: {tcp_label}; authenticated /v1/models response: {elapsed} ms."
        ),
    );
    Ok(format!(
        "Direct network TCP connect: {tcp_label}; API /v1/models response: {elapsed} ms (HTTP {}). These are not ICMP ping measurements.",
        status.as_u16()
    ))
}

// This is a conservative, script-based smoke check, not a linguistic
// correctness assessment. A bare marker or an English-only reply is not a pass.
fn isolated_probe_checks(
    content: &str,
    marker: &str,
    language: &str,
    tool_count: usize,
) -> (bool, bool, bool) {
    let marker_at_end = content.trim_end().ends_with(marker);
    let script_characters = content
        .chars()
        .filter(|&character| match language {
            "Persian" => matches!(
                character,
                '\u{0600}'..='\u{06FF}' | '\u{0750}'..='\u{077F}' | '\u{08A0}'..='\u{08FF}'
            ),
            "Chinese" => matches!(
                character,
                '\u{3400}'..='\u{4DBF}' | '\u{4E00}'..='\u{9FFF}' | '\u{F900}'..='\u{FAFF}'
            ),
            _ => false,
        })
        .take(2)
        .count();
    let script_detected = script_characters >= 2;
    (
        marker_at_end && script_detected && tool_count == 0,
        marker_at_end,
        script_detected,
    )
}

fn isolated_provider_probe_payload(model: &str, prompt: &str) -> Value {
    json!({
        "model": model,
        "messages": [{"role": "user", "content": prompt}],
        "max_tokens": 512,
        "stream": false
    })
}

// An opt-in, minimal provider-only probe. Each question is independent:
// no Claude Desktop history, tools, web search, system prompts or host actions.
// Only fixed synthetic markers go to the selected default upstream model.
#[tauri::command]
async fn probe_provider_fidelity(state: TauriState<'_, BridgeState>) -> Result<Vec<Value>, String> {
    let config = state.config.read().await.clone();
    let route = config
        .routes
        .iter()
        .find(|route| route.is_default)
        .or_else(|| config.routes.first())
        .ok_or_else(|| "Map at least one upstream model before running the probe.".to_string())?;
    let endpoint = upstream_url(&config.base_url, "chat/completions")?;
    let mut results = Vec::new();
    for (language, marker, prompt) in [
        (
            "Persian",
            "FA-527",
            "فقط به فارسی پاسخ بده و در پایان FA-527 بنویس. هیچ جست‌وجویی انجام نده.",
        ),
        (
            "Chinese",
            "ZH-884",
            "请只用中文回答，并在结尾写上 ZH-884。不要搜索。",
        ),
    ] {
        let request = state
            .client
            .post(&endpoint)
            .timeout(Duration::from_secs(45))
            .json(&isolated_provider_probe_payload(
                &route.upstream_model,
                prompt,
            ));
        let request = add_upstream_auth(request, &config)?;
        let started = Instant::now();
        let response = send_provider_with_fallback(
            request,
            &state.direct_provider_client,
            &state.observability,
            &config,
        )
        .await
        .map_err(|err| format!("Isolated model probe could not connect: {err}"))?;
        let status = response.status();
        if !status.is_success() {
            return Err(format!(
                "Isolated model probe returned HTTP {}. Check the model route and provider API.",
                status.as_u16()
            ));
        }
        let payload: Value = response
            .json()
            .await
            .map_err(|_| "The provider probe response is not valid JSON.".to_string())?;
        let content = payload
            .pointer("/choices/0/message/content")
            .and_then(Value::as_str)
            .unwrap_or("");
        let tool_count = payload
            .pointer("/choices/0/message/tool_calls")
            .and_then(Value::as_array)
            .map_or(0, Vec::len);
        let (success, marker_at_end, script_detected) =
            isolated_probe_checks(content, marker, language, tool_count);
        let reply_preview: String = content.chars().take(240).collect();
        state.observability.log(
            if success { "info" } else { "warning" },
            "provider fidelity",
            &format!(
                "Isolated {language} model probe: passed={success}, marker_at_end={marker_at_end}, script_detected={script_detected}, tool_calls={tool_count}, response_ms={}.",
                started.elapsed().as_millis()
            ),
        );
        results.push(json!({
            "language": language,
            "marker": marker,
            "passed": success,
            "markerAtEnd": marker_at_end,
            "scriptDetected": script_detected,
            "model": payload.get("model").and_then(Value::as_str).unwrap_or("unreported"),
            "requestedModel": route.upstream_model,
            "responseId": payload.get("id").and_then(Value::as_str).unwrap_or("unreported"),
            "responseMs": started.elapsed().as_millis(),
            "toolCalls": tool_count,
            "reply": reply_preview
        }));
    }
    Ok(results)
}

// A failed HTTP request is not an HTTP status. Diagnose DNS and TCP locally
// and separately; never guess TLS failure if only TCP was tested. The optional
// system route must not trigger an unrelated direct DNS/TCP probe.
async fn diagnose_provider_transport(
    endpoint: &str,
    config: &AppConfig,
    error: &reqwest::Error,
) -> String {
    if normalize_provider_network_mode(&config.provider_network_mode) == "system" {
        return "PROVIDER_SYSTEM_ROUTE: اتصال از مسیر پراکسی سیستم برقرار نشد. وضعیت پراکسی ویندوز و پورت آن را بررسی کنید یا مسیر مستقیم را انتخاب کنید.".into();
    }
    let Some((host, port)) = Url::parse(endpoint)
        .ok()
        .and_then(|url| Some((url.host_str()?.to_owned(), url.port_or_known_default()?)))
    else {
        return "PROVIDER_URL: نشانی API ارائه‌دهنده معتبر نیست.".into();
    };
    let resolved = tokio::time::timeout(
        Duration::from_secs(5),
        tokio::net::lookup_host((host.as_str(), port)),
    )
    .await;
    let addresses: Vec<std::net::SocketAddr> = match resolved {
        Ok(Ok(result)) => result.take(8).collect(),
        _ => {
            return "PROVIDER_DNS: تبدیل نام دامنه به IP در ویندوز ناموفق بود. DNS سیستم و اتصال شبکه را بررسی کنید.".into();
        }
    };
    if addresses.is_empty() {
        return "PROVIDER_DNS: هیچ آدرس IP برای دامنه پیدا نشد. تنظیمات DNS را بررسی کنید.".into();
    }
    let tcp = tokio::time::timeout(
        Duration::from_secs(5),
        tokio::net::TcpStream::connect(addresses.as_slice()),
    )
    .await;
    if !matches!(tcp, Ok(Ok(_))) {
        return "PROVIDER_TCP: دامنه به IP تبدیل شد اما اتصال TCP مستقیم برقرار نشد. مسیر اینترنت، IPv4/IPv6 و فایروال را بررسی کنید. پراکسی وب به صورت خودکار به API اعمال نمی‌شود.".into();
    }
    if error.is_timeout() {
        "PROVIDER_TIMEOUT: DNS و TCP مستقیم کار می‌کنند، ولی درخواست HTTPS به‌موقع کامل نشد. TLS، سرعت پاسخ سرور یا مسیر شبکه را بررسی کنید.".into()
    } else {
        "PROVIDER_TLS_OR_HTTP: DNS و TCP مستقیم برقرار است، اما تبادل HTTPS شکست خورد. گواهی TLS، محدودیت شبکه یا ارتباط سرور را بررسی کنید؛ خطای TLS هنوز قطعی نیست.".into()
    }
}

async fn fetch_upstream_models(
    client: &Client,
    direct: &Client,
    config: &AppConfig,
    observability: &ObservabilityStore,
) -> Result<Vec<String>, String> {
    let url = upstream_url(&config.base_url, "models")?;
    let mut request = client.get(&url).timeout(Duration::from_secs(30));
    request = add_upstream_auth(request, config)?;
    let response = match send_provider_with_fallback(request, direct, observability, config).await {
        Ok(response) => response,
        Err(error) => return Err(diagnose_provider_transport(&url, config, &error).await),
    };
    let status = response.status();
    let body = response
        .text()
        .await
        .map_err(|error| format!("Could not read the model list response: {error}"))?;
    if !status.is_success() {
        return Err(provider_http_error(status, "/v1/models", &body));
    }
    let parsed: Value = serde_json::from_str(&body)
        .map_err(|_| "The provider's /models response was not valid JSON.".to_string())?;
    let list = parsed
        .get("data")
        .or_else(|| parsed.get("models"))
        .and_then(Value::as_array)
        .ok_or_else(|| {
            "The provider response did not contain a data or models array.".to_string()
        })?;
    let mut models: Vec<String> = list
        .iter()
        .filter_map(|item| {
            item.as_str()
                .or_else(|| item.get("id").and_then(Value::as_str))
                .or_else(|| item.get("name").and_then(Value::as_str))
        })
        .map(str::trim)
        .filter(|model| !model.is_empty())
        .map(str::to_string)
        .collect();
    models.sort();
    models.dedup();
    if models.is_empty() {
        return Err("The provider returned no models.".into());
    }
    Ok(models)
}

fn add_upstream_auth(
    request: reqwest::RequestBuilder,
    config: &AppConfig,
) -> Result<reqwest::RequestBuilder, String> {
    if config.api_key.is_empty() {
        return Ok(request);
    }
    if config.auth_style == "api_key" {
        Ok(request.header("api-key", &config.api_key))
    } else {
        Ok(request.bearer_auth(&config.api_key))
    }
}

fn provider_http_error(status: reqwest::StatusCode, endpoint: &str, body: &str) -> String {
    let code = status.as_u16();
    match code {
        401 | 403 => format!(
            "The provider returned HTTP {code} for {endpoint}. Check the API key and authentication style."
        ),
        404 => format!(
            "The provider returned HTTP 404 for {endpoint}. Check that the base URL includes the provider's compatible API path."
        ),
        502 | 503 | 504 => format!(
            "The provider gateway returned HTTP {code} (Bad Gateway). The server was reached, but its upstream service is unavailable. Verify the API URL and provider availability, then try again."
        ),
        _ => {
            let detail = compact_response_body(body);
            if detail.is_empty() {
                format!("The provider rejected the model list request (HTTP {code}).")
            } else {
                format!("The provider rejected the model list request (HTTP {code}): {detail}")
            }
        }
    }
}

fn compact_response_body(body: &str) -> String {
    let mut text = String::new();
    let mut in_tag = false;
    let mut pending_space = false;
    for character in body.chars() {
        match character {
            '<' => in_tag = true,
            '>' if in_tag => in_tag = false,
            _ if in_tag => {}
            character if character.is_whitespace() => pending_space = true,
            character => {
                if pending_space && !text.is_empty() {
                    text.push(' ');
                }
                pending_space = false;
                text.push(character);
            }
        }
    }
    truncate(text.trim(), 180)
}

fn upstream_url(base_url: &str, suffix: &str) -> Result<String, String> {
    let base = base_url.trim().trim_end_matches('/');
    let url = Url::parse(base).map_err(|error| format!("The provider URL is invalid: {error}"))?;
    if !matches!(url.scheme(), "http" | "https") {
        return Err("The provider URL must start with http:// or https://.".into());
    }
    let path = if url.path().trim_end_matches('/').ends_with("/v1") {
        format!("{}/{}", base, suffix)
    } else {
        format!("{}/v1/{}", base, suffix)
    };
    Ok(path)
}

#[tauri::command]
async fn start_proxy(
    app: AppHandle,
    state: TauriState<'_, BridgeState>,
) -> Result<ProxyStatus, String> {
    if let Err(error) = start_proxy_inner(state.inner()).await {
        state.observability.log(
            "error",
            "proxy",
            &format!("Could not start the local proxy: {error}"),
        );
        return Err(error);
    }
    update_status_icon(&app, true);
    let config = state.config.read().await.clone();
    state
        .observability
        .log("info", "proxy", "Local proxy started.");
    Ok(status_from(&config, true))
}

async fn start_proxy_inner(state: &BridgeState) -> Result<(), String> {
    let mut runtime = state.runtime.lock().await;
    if runtime.is_some() {
        return Ok(());
    }
    let mut stored_config = state.config.write().await;
    let config = &mut *stored_config;
    if !configured(config) {
        return Err(
            "Add a provider URL, API key, and at least one model route before starting the proxy."
                .into(),
        );
    }
    ensure_gateway_token(config);
    persist_config(&state.config_path, config)?;
    let config = config.clone();
    drop(stored_config);
    let address: SocketAddr = format!("{LOCAL_HOST}:{}", config.proxy_port)
        .parse()
        .map_err(|error: std::net::AddrParseError| error.to_string())?;
    let listener = tokio::net::TcpListener::bind(address)
        .await
        .map_err(|error| format!("Local port {} is unavailable: {error}", config.proxy_port))?;
    let (stop_tx, stop_rx) = oneshot::channel();
    let app = Router::new()
        .route("/healthz", get(healthz))
        .route("/mcp", post(mcp_post).get(mcp_get))
        .route("/v1/models", get(list_models).options(preflight))
        .route("/v1/messages", post(anthropic_messages).options(preflight))
        .route(
            "/claude-desktop/v1/models",
            get(list_models).options(preflight),
        )
        .route(
            "/claude-desktop/v1/messages",
            post(anthropic_messages).options(preflight),
        )
        .with_state(ServerState {
            config: state.config.clone(),
            upstream_clients: state.upstream_clients.clone(),
            direct_provider_client: state.direct_provider_client.clone(),
            upstream_cursor: state.upstream_cursor.clone(),
            upstream_slots: state.upstream_slots.clone(),
            web_slots: state.web_slots.clone(),
            shell_slots: state.shell_slots.clone(),
            observability: state.observability.clone(),
            active_requests: state.active_requests.clone(),
            v2_contract_mode: ContractMode::from_setting(
                &std::env::var("CLAUDE_PROXY_V2_CONTRACTS").unwrap_or_default(),
            ),
            v2_stream_enabled: stream_enabled(
                ContractMode::from_setting(
                    &std::env::var("CLAUDE_PROXY_V2_CONTRACTS").unwrap_or_default(),
                ),
                &std::env::var("CLAUDE_PROXY_V2_STREAM").unwrap_or_default(),
            ),
            v2_capabilities: CapabilitySettings::from_strings(
                &std::env::var("CLAUDE_PROXY_V2_CAPABILITIES").unwrap_or_default(),
                &std::env::var("CLAUDE_PROXY_V2_MODEL_CAPABILITIES").unwrap_or_default(),
            ),
            v2_output_guard_enabled: ClientOutputGuard::enabled(
                ContractMode::from_setting(
                    &std::env::var("CLAUDE_PROXY_V2_CONTRACTS").unwrap_or_default(),
                ) == ContractMode::Enforce,
                &std::env::var("CLAUDE_PROXY_V2_DESKTOP_OUTPUT_CAP").unwrap_or_default(),
            ),
        })
        .layer(DefaultBodyLimit::max(25 * 1024 * 1024));
    let runtime_id = uuid::Uuid::new_v4();
    let runtime_state = state.runtime.clone();
    let observability = state.observability.clone();
    let app_handle = state.app.clone();
    tauri::async_runtime::spawn(async move {
        let server = axum::serve(listener, app).with_graceful_shutdown(async {
            let _ = stop_rx.await;
        });
        if let Err(error) = server.await {
            observability.log(
                "error",
                "proxy",
                &format!("Local proxy server stopped unexpectedly: {error}"),
            );
        }
        let mut runtime = runtime_state.lock().await;
        let was_active = if runtime
            .as_ref()
            .is_some_and(|active| active.id == runtime_id)
        {
            runtime.take();
            true
        } else {
            false
        };
        drop(runtime);
        if was_active {
            update_status_icon(&app_handle, false);
        }
    });
    *runtime = Some(RuntimeHandle {
        id: runtime_id,
        stop: stop_tx,
    });
    Ok(())
}

async fn stop_proxy_inner(state: &BridgeState) {
    if let Some(runtime) = state.runtime.lock().await.take() {
        let _ = runtime.stop.send(());
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

#[tauri::command]
async fn stop_proxy(
    app: AppHandle,
    state: TauriState<'_, BridgeState>,
) -> Result<ProxyStatus, String> {
    stop_proxy_inner(state.inner()).await;
    update_status_icon(&app, false);
    let config = state.config.read().await.clone();
    state
        .observability
        .log("info", "proxy", "Local proxy stopped.");
    Ok(status_from(&config, false))
}

#[tauri::command]
async fn test_local_route(state: TauriState<'_, BridgeState>) -> Result<String, String> {
    if state.runtime.lock().await.is_none() {
        return Err("The local proxy is stopped.".into());
    }
    let config = state.config.read().await.clone();
    let response = state
        .client
        .get(format!(
            "http://{}:{}/healthz",
            LOCAL_HOST, config.proxy_port
        ))
        .send()
        .await
        .map_err(|error| {
            state.observability.log(
                "error",
                "proxy check",
                "Could not reach the local proxy health endpoint.",
            );
            error.to_string()
        })?;
    if !response.status().is_success() {
        state.observability.log(
            "error",
            "proxy check",
            &format!(
                "The local proxy health endpoint returned HTTP {}.",
                response.status().as_u16()
            ),
        );
        return Err(format!(
            "The local proxy returned HTTP {}.",
            response.status().as_u16()
        ));
    }
    Ok("The local proxy is ready.".into())
}

// This is an opt-in diagnostic, not a tool-availability or admin-policy
// bypass. Each check reports its own result. Do not log HTTP content,
// credentials, proxy address, search text, or response bodies.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct WebProbeResult {
    check: &'static str,
    ok: bool,
    detail: &'static str,
    response_ms: u128,
}

fn web_probe_failure(error: &str) -> &'static str {
    let text = error.to_ascii_lowercase();
    if text.contains("403") || text.contains("forbidden") || text.contains("blocked") {
        "Access denied or blocked"
    } else if text.contains("429") {
        "Remote service rate-limited this request"
    } else if text.contains("timed out") || text.contains("timeout") {
        "Connection timed out"
    } else if text.contains("resolve") || text.contains("dns") {
        "DNS resolution failed (may differ from proxy DNS)"
    } else if text.contains("proxy") {
        "Proxy connection failed"
    } else {
        "Network, HTTP, or parsing error"
    }
}

fn evaluate_web_probe(check: &'static str, payload: &str, response_ms: u128) -> WebProbeResult {
    let parsed: Value = match serde_json::from_str(payload) {
        Ok(value) => value,
        Err(_) => {
            return WebProbeResult {
                check,
                ok: false,
                detail: "Tool response was not valid JSON",
                response_ms,
            };
        }
    };
    if let Some(error) = parsed.get("error").and_then(Value::as_str) {
        return WebProbeResult {
            check,
            ok: false,
            detail: web_probe_failure(error),
            response_ms,
        };
    }
    match check {
        "search" => {
            let result_count = parsed
                .get("results")
                .and_then(Value::as_array)
                .map_or(0, Vec::len);
            WebProbeResult {
                check,
                ok: result_count > 0,
                detail: if result_count > 0 {
                    "Search returned parseable public results"
                } else {
                    "Search returned no parseable results (possibly blocked or changed HTML)"
                },
                response_ms,
            }
        }
        "github" => {
            let readable = parsed
                .get("content")
                .and_then(Value::as_str)
                .is_some_and(|content| content.to_ascii_lowercase().contains("claude-mem"));
            WebProbeResult {
                check,
                ok: readable,
                detail: if readable {
                    "Public GitHub repository page was retrieved"
                } else {
                    "GitHub response did not contain the expected repository name"
                },
                response_ms,
            }
        }
        _ => WebProbeResult {
            check,
            ok: false,
            detail: "Unknown diagnostic type",
            response_ms,
        },
    }
}

#[tauri::command]
async fn test_web_access(
    state: TauriState<'_, BridgeState>,
) -> Result<Vec<WebProbeResult>, String> {
    let config = state.config.read().await.clone();
    if !web_tools_enabled(&config) {
        return Err(
            "Enable Automatic or Required web access before running capability checks.".into(),
        );
    }
    let mut checks = Vec::with_capacity(3);

    // The gateway is local. Never send its authenticated endpoint or token
    // through the user's web proxy, provider, or a system-wide proxy.
    let started = Instant::now();
    let local = Client::builder()
        .no_proxy()
        .timeout(Duration::from_secs(5))
        .build()
        .map_err(|_| "Could not initialize local gateway check.")?;
    let gateway_ok = match local
        .get(format!("http://127.0.0.1:{}/healthz", config.proxy_port))
        .send()
        .await
    {
        Ok(response) if response.status().is_success() => {
            response
                .json::<Value>()
                .await
                .ok()
                .and_then(|value| value.get("ok").and_then(Value::as_bool))
                == Some(true)
        }
        _ => false,
    };
    checks.push(WebProbeResult {
        check: "gateway",
        ok: gateway_ok,
        detail: if gateway_ok {
            "Local gateway responded to /healthz"
        } else {
            "Local gateway was unavailable or returned an invalid health response"
        },
        response_ms: started.elapsed().as_millis(),
    });

    // Use the exact same search/fetch functions, proxy route and public-host
    // validation that are used for actual Proxy tools. No fabricated success.
    let started = Instant::now();
    let search = web_search(&config, &json!({"query":"claude-mem thedotmack GitHub"})).await;
    checks.push(evaluate_web_probe(
        "search",
        &search,
        started.elapsed().as_millis(),
    ));

    let started = Instant::now();
    let github = web_fetch(
        &config,
        &json!({"url":"https://github.com/thedotmack/claude-mem","max_chars":15000}),
    )
    .await;
    checks.push(evaluate_web_probe(
        "github",
        &github,
        started.elapsed().as_millis(),
    ));

    for check in &checks {
        state.observability.log(
            if check.ok { "info" } else { "warning" },
            "web capability",
            &format!(
                "{} probe: {} ({} ms).",
                check.check,
                if check.ok { "passed" } else { check.detail },
                check.response_ms
            ),
        );
    }
    Ok(checks)
}

// Local-only, authenticated MCP Streamable HTTP endpoint for Claude Code.
// The gateway's chat CORS rules are deliberately NOT applied to this route.
// A local MCP is not a Cowork remote connector and does not change client policy.
const MCP_SERVER_NAME: &str = "claude-proxy-web";
const MCP_PROTOCOL_VERSION: &str = "2025-11-25";

fn mcp_access_allowed(headers: &HeaderMap, config: &AppConfig) -> Result<(), StatusCode> {
    let expected_ip = format!("127.0.0.1:{}", config.proxy_port);
    let expected_localhost = format!("localhost:{}", config.proxy_port);
    let host = headers
        .get(header::HOST)
        .and_then(|value| value.to_str().ok())
        .ok_or(StatusCode::FORBIDDEN)?;
    if host != expected_ip && host != expected_localhost {
        return Err(StatusCode::FORBIDDEN);
    }
    if let Some(origin) = headers.get(header::ORIGIN) {
        let origin = origin.to_str().map_err(|_| StatusCode::FORBIDDEN)?;
        if origin != format!("http://{expected_ip}")
            && origin != format!("http://{expected_localhost}")
        {
            return Err(StatusCode::FORBIDDEN);
        }
    }
    if !authorized(headers, config) {
        return Err(StatusCode::UNAUTHORIZED);
    }
    if !web_tools_enabled(config) {
        return Err(StatusCode::FORBIDDEN);
    }
    Ok(())
}

fn mcp_response(id: &Value, result: Value) -> Response {
    Json(json!({"jsonrpc":"2.0","id":id,"result":result})).into_response()
}

fn mcp_error(id: Value, code: i32, message: &'static str) -> Response {
    Json(json!({"jsonrpc":"2.0","id":id,"error":{"code":code,"message":message}})).into_response()
}

fn mcp_tool_list() -> Vec<Value> {
    web_tool_definitions()
        .iter()
        .filter_map(|item| item.get("function"))
        .map(|tool| {
            json!({
                "name":tool["name"],
                "description":tool["description"],
                "inputSchema":tool["parameters"],
                "annotations": {
                    "readOnlyHint": true,
                    "destructiveHint": false,
                    "openWorldHint": true
                }
            })
        })
        .collect()
}

async fn mcp_get(State(state): State<ServerState>, headers: HeaderMap) -> Response {
    let config = state.config.read().await.clone();
    if let Err(status) = mcp_access_allowed(&headers, &config) {
        return status.into_response();
    }
    StatusCode::METHOD_NOT_ALLOWED.into_response()
}

async fn mcp_post(
    State(state): State<ServerState>,
    headers: HeaderMap,
    Json(request): Json<Value>,
) -> Response {
    let config = state.config.read().await.clone();
    if let Err(status) = mcp_access_allowed(&headers, &config) {
        return status.into_response();
    }
    if request.get("jsonrpc").and_then(Value::as_str) != Some("2.0") {
        return mcp_error(Value::Null, -32600, "Invalid JSON-RPC request.");
    }
    let id = request.get("id").cloned().unwrap_or(Value::Null);
    let Some(method) = request.get("method").and_then(Value::as_str) else {
        return mcp_error(id, -32600, "Missing JSON-RPC method.");
    };
    if id.is_null() {
        return if method == "notifications/initialized" {
            StatusCode::ACCEPTED.into_response()
        } else {
            mcp_error(Value::Null, -32600, "Unsupported MCP notification.")
        };
    }
    match method {
        "initialize" => mcp_response(
            &id,
            json!({
                "protocolVersion":MCP_PROTOCOL_VERSION,
                "capabilities":{"tools":{"listChanged":false}},
                "serverInfo":{"name":MCP_SERVER_NAME,"version":env!("CARGO_PKG_VERSION")},
                "instructions":"Public web search and page reading use Claude Proxy's configured web route. These tools cannot bypass client or organization network restrictions."
            }),
        ),
        "ping" => mcp_response(&id, json!({})),
        "tools/list" => mcp_response(&id, json!({"tools":mcp_tool_list()})),
        "tools/call" => {
            let Some(name) = request.pointer("/params/name").and_then(Value::as_str) else {
                return mcp_error(id, -32602, "Missing MCP tool name.");
            };
            if name != "claude_proxy_web_search" && name != "claude_proxy_web_fetch" {
                return mcp_error(id, -32602, "Unknown MCP web tool.");
            }
            let args = request
                .pointer("/params/arguments")
                .cloned()
                .unwrap_or(json!({}));
            if !args.is_object() {
                return mcp_error(id, -32602, "Tool arguments must be an object.");
            }
            let Ok(_permit) = state.web_slots.clone().acquire_owned().await else {
                return mcp_error(id, -32603, "Web tool capacity unavailable.");
            };
            let started = Instant::now();
            let text = if name == "claude_proxy_web_search" {
                web_search(&config, &args).await
            } else {
                web_fetch(&config, &args).await
            };
            let is_error = serde_json::from_str::<Value>(&text)
                .ok()
                .is_none_or(|value| value.get("error").is_some());
            state.observability.log(
                if is_error { "warning" } else { "info" },
                "mcp web tool",
                &format!(
                    "{}: {} ({} ms).",
                    if name == "claude_proxy_web_search" {
                        "search"
                    } else {
                        "fetch"
                    },
                    if is_error { "failed" } else { "completed" },
                    started.elapsed().as_millis()
                ),
            );
            mcp_response(
                &id,
                json!({
                    "content":[{"type":"text","text":text}],
                    "isError":is_error
                }),
            )
        }
        _ => mcp_error(id, -32601, "MCP method not found."),
    }
}

fn register_claude_code_web_mcp(
    settings: &mut Value,
    port: u16,
    token: &str,
) -> Result<(), String> {
    if !settings.is_object() {
        return Err("Claude Code configuration must be a JSON object.".into());
    }
    if settings.get("mcpServers").is_some()
        && !settings.get("mcpServers").is_some_and(Value::is_object)
    {
        return Err("Existing Claude Code mcpServers is not a JSON object.".into());
    }
    let url = format!("http://127.0.0.1:{port}/mcp");
    if settings.get("mcpServers").is_none() {
        settings["mcpServers"] = json!({});
    }
    if let Some(existing) = settings["mcpServers"].get(MCP_SERVER_NAME) {
        if existing.get("url").and_then(Value::as_str) != Some(url.as_str()) {
            return Err("A different Claude Code MCP server already uses the claude-proxy-web name. It was not overwritten.".into());
        }
    }
    settings["mcpServers"][MCP_SERVER_NAME] = json!({
        "type":"http",
        "url":url,
        "headers":{"Authorization":format!("Bearer {token}")}
    });
    Ok(())
}

#[tauri::command]
async fn apply_claude_code_web_mcp(
    app: AppHandle,
    state: TauriState<'_, BridgeState>,
) -> Result<String, String> {
    let config = state.config.read().await.clone();
    if !web_tools_enabled(&config) {
        return Err(
            "Enable and save Web access in Advanced settings before connecting tools.".into(),
        );
    }
    start_proxy_inner(state.inner()).await?;
    update_status_icon(&app, true);
    let config = state.config.read().await.clone();
    let root = std::env::var_os("USERPROFILE")
        .map(PathBuf::from)
        .ok_or_else(|| "The Windows user profile was not found.".to_string())?;
    let path = root.join(".claude.json");
    let mut settings = read_json_object(&path)?;
    register_claude_code_web_mcp(&mut settings, config.proxy_port, &config.gateway_token)?;
    transactional_writes(vec![(
        path,
        serde_json::to_vec_pretty(&settings).map_err(|error| error.to_string())?,
    )])?;
    state.observability.log(
        "info",
        "mcp configuration",
        "Claude Code local web MCP registration was updated after a user action.",
    );
    Ok("Claude Code Web MCP was configured. Fully restart Claude Code; then use /mcp to inspect its connection. Cowork remote connectors remain separate.".into())
}

async fn healthz(State(state): State<ServerState>) -> impl IntoResponse {
    let config = state.config.read().await;
    with_cors(Json(
        json!({ "ok": true, "configured": configured(&config), "routes": config.routes.len() }),
    ))
}

async fn list_models(State(state): State<ServerState>, headers: HeaderMap) -> Response {
    let config = state.config.read().await.clone();
    if !authorized(&headers, &config) {
        state.observability.log(
            "warning",
            "gateway",
            "Rejected a request with an invalid local gateway token.",
        );
        return api_error(
            StatusCode::UNAUTHORIZED,
            "authentication_error",
            "The local gateway token is invalid.",
        );
    }
    let routes = normalize_routes(config.routes);
    let labels = route_labels(&routes);
    let data: Vec<Value> = routes
        .iter()
        .zip(labels)
        .map(|(route, label)| {
            json!({
                "type": "model",
                "id": route.route_id,
                "display_name": label,
                "created_at": "2026-01-01T00:00:00Z"
            })
        })
        .collect();
    let first_id = data
        .first()
        .and_then(|model| model.get("id"))
        .cloned()
        .unwrap_or(Value::Null);
    let last_id = data
        .last()
        .and_then(|model| model.get("id"))
        .cloned()
        .unwrap_or(Value::Null);
    cors_response(
        Json(json!({ "data": data, "has_more": false, "first_id": first_id, "last_id": last_id }))
            .into_response(),
    )
}

async fn preflight() -> Response {
    let mut response = StatusCode::NO_CONTENT.into_response();
    response.headers_mut().insert(
        header::ACCESS_CONTROL_ALLOW_ORIGIN,
        HeaderValue::from_static("*"),
    );
    response.headers_mut().insert(
        header::ACCESS_CONTROL_ALLOW_METHODS,
        HeaderValue::from_static("GET, POST, OPTIONS"),
    );
    response.headers_mut().insert(
        header::ACCESS_CONTROL_ALLOW_HEADERS,
        HeaderValue::from_static(
            "authorization, content-type, anthropic-version, anthropic-beta, x-api-key",
        ),
    );
    response
}

fn authorized(headers: &HeaderMap, config: &AppConfig) -> bool {
    if config.gateway_token.is_empty() {
        return false;
    }
    let supplied = headers
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "))
        .or_else(|| {
            headers
                .get("x-api-key")
                .and_then(|value| value.to_str().ok())
        });
    supplied
        .is_some_and(|value| constant_time_eq(value.as_bytes(), config.gateway_token.as_bytes()))
}

fn constant_time_eq(left: &[u8], right: &[u8]) -> bool {
    if left.len() != right.len() {
        return false;
    }
    left.iter()
        .zip(right)
        .fold(0u8, |diff, (a, b)| diff | (a ^ b))
        == 0
}

fn api_error(status: StatusCode, kind: &str, message: &str) -> Response {
    cors_response(
        (
            status,
            Json(json!({ "type": "error", "error": { "type": kind, "message": message } })),
        )
            .into_response(),
    )
}

fn cors_response(mut response: Response) -> Response {
    response.headers_mut().insert(
        header::ACCESS_CONTROL_ALLOW_ORIGIN,
        HeaderValue::from_static("*"),
    );
    response.headers_mut().insert(
        header::ACCESS_CONTROL_ALLOW_HEADERS,
        HeaderValue::from_static(
            "authorization, content-type, anthropic-version, anthropic-beta, x-api-key",
        ),
    );
    response.headers_mut().insert(
        header::ACCESS_CONTROL_EXPOSE_HEADERS,
        HeaderValue::from_static("x-qbs-context-trimmed-turns"),
    );
    response
}

fn with_cors<T: Serialize>(value: Json<T>) -> Response {
    cors_response(value.into_response())
}

fn truncate(value: &str, max_chars: usize) -> String {
    let mut result: String = value.chars().take(max_chars).collect();
    if value.chars().count() > max_chars {
        result.push('…');
    }
    result
}

fn provider_label(config: &AppConfig) -> String {
    let name = config.provider_name.trim();
    let host = Url::parse(config.base_url.trim())
        .ok()
        .and_then(|url| url.host_str().map(str::to_owned));
    match (name.is_empty(), host) {
        (false, Some(host)) => truncate(&format!("{name} · {host}"), 160),
        (false, None) => truncate(name, 160),
        (true, Some(host)) => truncate(&host, 160),
        (true, None) => "Unknown provider".into(),
    }
}

async fn anthropic_messages(
    State(state): State<ServerState>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    let should_keep_stream_alive = serde_json::from_slice::<Value>(&body)
        .ok()
        .and_then(|request| request.get("stream").and_then(Value::as_bool))
        == Some(true)
        && web_tools_enabled(&*state.config.read().await);
    let mut processing = Box::pin(process_anthropic_messages(state, headers, body));
    if !should_keep_stream_alive {
        return processing.await;
    }
    if let Ok(response) = tokio::time::timeout(Duration::from_secs(10), &mut processing).await {
        return response;
    }

    // An SSE comment is a protocol-valid heartbeat. It keeps intermediate web
    // tool work from looking like a stalled response without exposing partial
    // model text or changing the normal Anthropic message event sequence.
    let stream = async_stream::stream! {
        let mut keepalive = tokio::time::interval(Duration::from_secs(10));
        keepalive.tick().await;
        yield Ok::<Bytes, axum::Error>(Bytes::from_static(b": keep-alive\n\n"));
        loop {
            tokio::select! {
                response = &mut processing => {
                    if response.status().is_success() {
                        let mut body = response.into_body().into_data_stream();
                        while let Some(chunk) = body.next().await {
                            yield chunk;
                        }
                    } else {
                        let mut body = response.into_body().into_data_stream();
                        let mut bytes = Vec::new();
                        while let Some(Ok(chunk)) = body.next().await {
                            bytes.extend_from_slice(&chunk);
                        }
                        let error_body = serde_json::from_slice::<Value>(&bytes).unwrap_or_else(|_| {
                            json!({ "error": { "type": "api_error", "message": "The web-enabled request failed. Review local diagnostics." } })
                        });
                        let error = json!({
                            "type": "error",
                            "error": error_body.get("error").cloned().unwrap_or_else(|| json!({
                                "type": "api_error",
                                "message": "The web-enabled request failed. Review local diagnostics."
                            }))
                        });
                        yield Ok::<Bytes, axum::Error>(Bytes::from(encode_sse("error", error)));
                    }
                    break;
                }
                _ = keepalive.tick() => {
                    yield Ok::<Bytes, axum::Error>(Bytes::from_static(b": keep-alive\n\n"));
                }
            }
        }
    };
    let mut response = Response::new(Body::from_stream(stream));
    *response.status_mut() = StatusCode::OK;
    response.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("text/event-stream; charset=utf-8"),
    );
    response.headers_mut().insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("no-cache, no-transform"),
    );
    response
        .headers_mut()
        .insert(header::CONNECTION, HeaderValue::from_static("keep-alive"));
    cors_response(response)
}

async fn process_anthropic_messages(
    state: ServerState,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    let request_id = uuid::Uuid::new_v4().simple().to_string();
    let config = state.config.read().await.clone();
    if !authorized(&headers, &config) {
        return api_error(
            StatusCode::UNAUTHORIZED,
            "authentication_error",
            "The local gateway token is invalid.",
        );
    }
    let request: Value = match serde_json::from_slice(&body) {
        Ok(request) => request,
        Err(_) => {
            state.observability.log(
                "warning",
                "gateway",
                "Rejected a request with an invalid JSON body.",
            );
            return api_error(
                StatusCode::BAD_REQUEST,
                "invalid_request_error",
                "The request body is not valid JSON.",
            );
        }
    };
    // OFF by default. Shadow observes request shape without storing prompts.
    // ENFORCE rejects unsupported Anthropic blocks before legacy translation,
    // so Code/Cowork cannot silently lose client tool or document context.
    if let Some(assessment) = check_chat_completions(state.v2_contract_mode, &request) {
        state.observability.log(
            "info",
            "v2 preflight",
            &format!("Request {request_id}: {}", assessment.sanitized_summary(),),
        );
        // Byte-only context accounting explains the contribution of system,
        // tools, and history without claiming verified tokenizer accuracy.
        let context_shape = ContextShape::inspect(&request);
        state.observability.log(
            "info",
            "v2 context shape",
            &format!(
                "Request {request_id}: {}",
                context_shape.sanitized_summary()
            ),
        );
        if assessment.failed() {
            state.observability.log(
                "warning",
                "v2 compatibility",
                &format!("Request {request_id}: unsupported client content detected."),
            );
            if state.v2_contract_mode == ContractMode::Enforce {
                return api_error(
                    StatusCode::BAD_REQUEST,
                    "invalid_request_error",
                    "V2 compatibility check: this request contains blocks not safely supported by the Chat Completions adapter. Select an eligible native Anthropic provider or disable this experimental adapter mode.",
                );
            }
        }
    }
    let requested_model = request
        .get("model")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let Some(route) = resolve_route(&config.routes, requested_model) else {
        state.observability.log(
            "warning",
            "gateway",
            "A request referenced a model with no configured route.",
        );
        return api_error(
            StatusCode::BAD_REQUEST,
            "invalid_request_error",
            "The requested model is not mapped to a provider model.",
        );
    };
    let mut payload = match anthropic_to_openai(&request, route) {
        Ok(payload) => payload,
        Err(error) => return api_error(StatusCode::BAD_REQUEST, "invalid_request_error", &error),
    };
    let client_wants_stream = request.get("stream").and_then(Value::as_bool) == Some(true);
    // V2 capability checks run on the fully translated payload, including
    // client tools, historical tool results, and optional local gateway tools.
    // They never guess from model names or silently degrade Code into chat.
    let client_tools_owned = client_tools_take_precedence(&request);
    let may_trim_context = may_auto_trim_context(&config, client_tools_owned);
    if client_tools_owned && agent_output_budget_collapsed(&request, &payload, route.context_window)
    {
        state.observability.log(
            "warning",
            "context",
            &format!(
                "Request {request_id}: native tool definitions exceed the configured agent response budget; refused an artificial one-token completion."
            ),
        );
        return api_error(
            StatusCode::BAD_REQUEST,
            "invalid_request_error",
            "COWORK_CONTEXT_TOO_LARGE: the configured model context cannot safely fit the Cowork/Claude Code tool schema and leave room for a response. Set the model route's verified context window to its actual provider limit or use an appropriately sized model. No client history was deleted.",
        );
    }
    let web_intent = if client_tools_owned {
        WebIntent::NoWeb
    } else {
        classify_web_intent(&request)
    };
    let web_enabled = add_web_tools(&mut payload, &config, web_intent);
    let shell_enabled = if client_tools_owned {
        false
    } else {
        add_shell_tool(&mut payload, &config)
    };
    let local_tools_enabled = web_enabled || shell_enabled;
    let inbound_last_user = request
        .get("messages")
        .and_then(Value::as_array)
        .and_then(|messages| {
            messages
                .iter()
                .rev()
                .find(|message| message.get("role").and_then(Value::as_str) == Some("user"))
        })
        .map(|message| anthropic_text(message.get("content").unwrap_or(&Value::Null)))
        .unwrap_or_default();
    let outbound_last_user = payload
        .get("messages")
        .and_then(Value::as_array)
        .and_then(|messages| {
            messages
                .iter()
                .rev()
                .find(|message| message.get("role").and_then(Value::as_str) == Some("user"))
        })
        .map(|message| anthropic_text(message.get("content").unwrap_or(&Value::Null)))
        .unwrap_or_default();
    // A greeting-category flag is deliberately not a prompt hash: hashing
    // a one-word message like "سلام" is trivially reversible.
    let inbound_greeting = is_simple_greeting_request(&inbound_last_user);
    let outbound_greeting = is_simple_greeting_request(&outbound_last_user);
    let last_user_text_preserved = inbound_last_user == outbound_last_user;
    let ordered_user_text_preserved = user_text_order_preserved(&request, &payload);
    state.observability.log(
        "info",
        "request provenance",
        &format!(
            "Request {request_id}: inbound_greeting={inbound_greeting}, outbound_greeting={outbound_greeting}, last_user_text_preserved={last_user_text_preserved}, ordered_user_text_preserved={ordered_user_text_preserved}, inbound_message_count={}, upstream_message_count={}, streaming={client_wants_stream}, client_tool_context_preserved={client_tools_owned}.",
            request.get("messages").and_then(Value::as_array).map_or(0, Vec::len),
            payload.get("messages").and_then(Value::as_array).map_or(0, Vec::len),
        ),
    );
    // A changed sequence of human-text messages is not a safe best-effort
    // translation. Report it visibly before forwarding anything upstream.
    if !ordered_user_text_preserved {
        state.observability.log(
            "error",
            "request fidelity",
            &format!(
                "Request {request_id}: human-text message sequence changed during translation; request rejected before provider execution."
            ),
        );
        return api_error(
            StatusCode::BAD_REQUEST,
            "invalid_request_error",
            "COWORK_MESSAGE_FIDELITY_ERROR: the adapter could not preserve the text of the client messages. No request was sent to the provider.",
        );
    }
    // An audit of the request SHAPE, not the content: safe to keep enabled
    // even when the V2 contract flag is off. The diagnostic page can prove
    // whether the gateway itself added web tools to a specific request.
    state.observability.log(
        "info",
        "tool routing",
        &format!(
            "Request {request_id}: client_tools={}, client_messages={}, web_intent={web_intent:?}, client_tools_owned={client_tools_owned}, local_web={web_enabled}, local_shell={shell_enabled}, forced_tool_choice={}.",
            request.get("tools").and_then(Value::as_array).map_or(0, Vec::len),
            request.get("messages").and_then(Value::as_array).map_or(0, Vec::len),
            payload.get("tool_choice").is_some(),
        ),
    );
    if let Some(result) = state
        .v2_capabilities
        .assess(&route.upstream_model, &payload)
    {
        if let Err(failure) = result {
            state.observability.log(
                "warning",
                "v2 capabilities",
                &format!("Request {request_id}: unsupported provider capability {failure:?}."),
            );
            if state.v2_capabilities.mode == CapabilityMode::Enforce {
                return api_error(
                    StatusCode::BAD_REQUEST,
                    "invalid_request_error",
                    failure.user_message(),
                );
            }
        }
    }
    if client_wants_stream && !local_tools_enabled {
        payload["stream_options"] = json!({ "include_usage": true });
    } else if local_tools_enabled {
        // The local tool loop needs a complete JSON message from the provider.
        // A final answer is converted back to Anthropic SSE below when the
        // originating Claude request asked for streaming.
        payload["stream"] = json!(false);
        if let Some(object) = payload.as_object_mut() {
            object.remove("stream_options");
        }
    }
    let target = match upstream_url(&config.base_url, "chat/completions") {
        Ok(url) => url,
        Err(error) => return api_error(StatusCode::BAD_GATEWAY, "api_error", &error),
    };
    let accepts = if client_wants_stream && !local_tools_enabled {
        "text/event-stream"
    } else {
        "application/json"
    };
    let mut trimmed_turns = 0usize;
    let mut proactive_trimmed_turns = 0usize;
    if may_trim_context && route.context_window > 0 {
        // Reserve output room for the budget the client actually asked for,
        // not the conversion-time cap: in a long conversation that cap is
        // already squeezed against the pre-trim input, which would make the
        // proactive trim reserve almost no output room at all.
        let mut output_budget = request
            .get("max_tokens")
            .and_then(Value::as_u64)
            .filter(|value| *value > 0)
            .unwrap_or(4_096);
        if route.max_output_tokens > 0 {
            output_budget = output_budget.min(route.max_output_tokens);
        }
        proactive_trimmed_turns =
            trim_to_context_budget(&mut payload, route.context_window, output_budget);
        trimmed_turns = proactive_trimmed_turns;
        if proactive_trimmed_turns > 0 {
            state.observability.log(
                "warning",
                "context",
                &format!(
                    "Proactive context budgeting removed {trimmed_turns} older conversation turn(s) before sending request {request_id}."
                ),
            );
            // The `max_tokens` cap set during conversion was sized against the
            // pre-trim input, so a long conversation can leave the model a
            // near-zero output budget. Now that turns are gone, restore the
            // freed output room (still bounded by the client's requested
            // `max_tokens` and the route's per-round cap).
            let room = route
                .context_window
                .saturating_sub(estimate_tokens(&payload))
                .saturating_sub(512)
                .max(1);
            let mut cap = request
                .get("max_tokens")
                .and_then(Value::as_u64)
                .unwrap_or(4_096);
            if route.max_output_tokens > 0 {
                cap = cap.min(route.max_output_tokens);
            }
            cap = cap.min(room).max(1);
            if let Some(current) = payload.get("max_tokens").and_then(Value::as_u64) {
                if cap > current {
                    payload["max_tokens"] = json!(cap);
                }
            }
        }
    }
    // The optional 32k Desktop/Cowork cap is negotiated after proactive
    // context adjustment, then enforced before *every* provider round.
    // Streaming needs V2 SSE because the legacy continuation path can
    // rewrite provider accounting and its stop reason.
    if state.v2_output_guard_enabled
        && client_wants_stream
        && !local_tools_enabled
        && !state.v2_stream_enabled
    {
        return api_error(
            StatusCode::BAD_REQUEST,
            "invalid_request_error",
            "Enable CLAUDE_PROXY_V2_STREAM=on before using the strict Desktop output cap with streaming.",
        );
    }
    let v2_output_guard = if state.v2_output_guard_enabled {
        match ClientOutputGuard::negotiate(&request, &payload, route.max_output_tokens) {
            Ok(guard) => {
                state.observability.log(
                    "info",
                    "v2 output budget",
                    &format!(
                        "Request {request_id}: strict upstream output limit {} tokens.",
                        guard.limit
                    ),
                );
                Some(guard)
            }
            Err(failure) => {
                return api_error(
                    StatusCode::BAD_REQUEST,
                    "invalid_request_error",
                    failure.user_message(),
                )
            }
        }
    } else {
        None
    };
    let upstream_client = state.next_upstream_client();
    let mut web_round = 0usize;
    let mut web_tool_count = 0usize;
    let request_started = Instant::now();
    let completion: Value = loop {
        if let Some(guard) = v2_output_guard {
            guard.apply(&mut payload);
        }
        let queue_started = Instant::now();
        let upstream_permit = match state.upstream_slots.clone().acquire_owned().await {
            Ok(permit) => permit,
            Err(_) => {
                return api_error(
                    StatusCode::SERVICE_UNAVAILABLE,
                    "api_error",
                    "The proxy is shutting down and cannot accept another model request.",
                )
            }
        };
        let queue_wait_ms = queue_started.elapsed().as_millis();
        if queue_wait_ms >= 25 {
            state.observability.log(
                "info",
                "concurrency",
                &format!("Request {request_id} upstream round {} waited {queue_wait_ms} ms for a model slot.", web_round + 1),
            );
        }
        let upstream_started = Instant::now();
        let upstream = loop {
            let mut outgoing = upstream_client
                .post(&target)
                .header(header::CONTENT_TYPE, "application/json")
                .header(header::ACCEPT, accepts);
            outgoing = match add_upstream_auth(outgoing, &config) {
                Ok(outgoing) => outgoing,
                Err(error) => {
                    return api_error(StatusCode::BAD_GATEWAY, "authentication_error", &error)
                }
            };
            let response = match send_provider_with_fallback(
                outgoing.json(&payload),
                &state.direct_provider_client,
                &state.observability,
                &config,
            )
            .await
            {
                Ok(response) => response,
                Err(error) => {
                    let failure_kind = provider_transport_error_kind(&error);
                    state.observability.log(
                        "error",
                        "provider",
                        &format!(
                            "Provider request failed for model route {requested_model} during upstream round {} ({failure_kind}, after {} ms).",
                            web_round + 1,
                            upstream_started.elapsed().as_millis()
                        ),
                    );
                    return api_error(
                        StatusCode::BAD_GATEWAY,
                        "api_error",
                        &format!("Could not send the request to the provider: {error}"),
                    );
                }
            };
            if response.status().is_success() {
                break response;
            }
            let status = response.status();
            let message = response
                .text()
                .await
                .unwrap_or_else(|_| "The error response could not be read.".into());
            if payload.get("stream_options").is_some() && is_stream_usage_unsupported(&message) {
                if let Some(object) = payload.as_object_mut() {
                    object.remove("stream_options");
                }
                state.observability.log(
                    "warning",
                    "usage",
                    "The provider does not support streamed usage metadata; token totals will be estimated when the stream omits usage.",
                );
                continue;
            }
            // Keep trimming the oldest complete turns until the provider
            // accepts the request or the payload has no older complete turn
            // left to drop (only the current turn remains). There is no fixed
            // trim budget here: a long conversation can hold far more turns
            // than a small cap, and stopping early while more turns were
            // still droppable turned long chats into dead ends.
            if may_trim_context && is_context_overflow(&message) && trim_oldest_turn(&mut payload) {
                trimmed_turns += 1;
                // Each dropped turn frees input room; hand the corresponding
                // output budget back to the model (bounded by the client's
                // requested `max_tokens` and the route's per-round cap).
                let room = route
                    .context_window
                    .saturating_sub(estimate_tokens(&payload))
                    .saturating_sub(512)
                    .max(1);
                let mut cap = request
                    .get("max_tokens")
                    .and_then(Value::as_u64)
                    .unwrap_or(4_096);
                if route.max_output_tokens > 0 {
                    cap = cap.min(route.max_output_tokens);
                }
                cap = cap.min(room).max(1);
                if let Some(current) = payload.get("max_tokens").and_then(Value::as_u64) {
                    if cap > current {
                        payload["max_tokens"] = json!(cap);
                    }
                }
                continue;
            }
            let provider_error = provider_error_identifier(&message)
                .map(|identifier| format!(" (provider error code/type: {identifier})"))
                .unwrap_or_default();
            state.observability.log(
                "error",
                "provider",
                &format!(
                    "Provider request failed with HTTP {} for model route {requested_model}{provider_error}.",
                    status.as_u16(),
                ),
            );
            let message = if is_context_overflow(&message) {
                if client_tools_owned {
                    format!(
                        "COWORK_CONTEXT_TOO_LARGE: Cowork/Claude Code tool and session context exceeds the selected provider capacity. The proxy preserved the client messages and did not silently delete its setup or tool history. Select a model with sufficient context, or reduce the agent task scope. Provider response: {}",
                        truncate(&message, 600)
                    )
                } else {
                    format!(
                        "The context limit was reached. Claude's visible history has not changed. Summarize an older message or enable automatic trimming of older turns. Gateway response: {}",
                        truncate(&message, 900)
                    )
                }
            } else {
                truncate(&message, 1200)
            };
            return api_error(
                StatusCode::from_u16(status.as_u16()).unwrap_or(StatusCode::BAD_GATEWAY),
                "api_error",
                &message,
            );
        };

        if client_wants_stream && !local_tools_enabled {
            let continuation_payload = payload.clone();
            // The client's own output budget, expressed in Anthropic
            // `max_tokens` on the original request. When it is present we keep
            // every merged response within this budget so the client's
            // output-token invariant holds (see `stream_openai_response`).
            // If the client omitted `max_tokens` we fall back to the route's
            // per-round provider budget so we still avoid merging more than
            // one provider response's worth into a single message.
            let client_output_budget = request
                .get("max_tokens")
                .and_then(Value::as_u64)
                .filter(|value| *value > 0)
                .or_else(|| (route.max_output_tokens > 0).then_some(route.max_output_tokens));
            let client_output_budget = v2_output_guard
                .map(|guard| guard.limit)
                .or(client_output_budget);
            return stream_openai_response(
                upstream,
                requested_model.to_string(),
                route.upstream_model.clone(),
                provider_label(&config),
                estimate_tokens(&payload),
                trimmed_turns,
                state.observability.clone(),
                upstream_permit,
                state.next_upstream_client(),
                state.direct_provider_client.clone(),
                config.clone(),
                target.to_string(),
                continuation_payload,
                route.context_window,
                route.max_output_tokens,
                client_output_budget,
                state.v2_stream_enabled,
                v2_output_guard.is_some(),
            );
        }

        let value: Value = match upstream.json().await {
            Ok(value) => value,
            Err(error) => {
                state.observability.log(
                    "error",
                    "provider",
                    "The provider returned a response that was not valid JSON.",
                );
                return api_error(
                    StatusCode::BAD_GATEWAY,
                    "api_error",
                    &format!("The model response was not valid JSON: {error}"),
                );
            }
        };
        // Record response shape without content or provider-supplied strings.
        // If local_web=false and provider_tool_calls=0, a claim about a
        // failing GitHub search is model-generated text, not a proxy search.
        state.observability.log(
            "info",
            "provider response shape",
            &format!(
                "Request {request_id}: provider_tool_calls={}, has_separate_reasoning={}, has_answer_content={}, local_web_enabled={web_enabled}, local_shell_enabled={shell_enabled}, local_tool_rounds={web_round}.",
                value
                    .pointer("/choices/0/message/tool_calls")
                    .and_then(Value::as_array)
                    .map_or(0, Vec::len),
                value
                    .pointer("/choices/0/message/reasoning_content")
                    .and_then(Value::as_str)
                    .is_some_and(|text| !text.is_empty()),
                value
                    .pointer("/choices/0/message/content")
                    .and_then(Value::as_str)
                    .is_some_and(|text| !text.is_empty()),
            ),
        );
        // Never forward an over-limit or unmeasured strict-mode response.
        // This also covers the nonstream local tool loop before execution.
        if let Some(guard) = v2_output_guard {
            let measured =
                parse_provider_usage(value.get("usage").unwrap_or(&Value::Null)).output_tokens;
            if let Err(failure) = guard.verify_reported_output(measured) {
                state.observability.log(
                    "error",
                    "v2 output budget",
                    &format!(
                        "Request {request_id}: provider output validation failed ({failure:?})."
                    ),
                );
                return api_error(StatusCode::BAD_GATEWAY, "api_error", failure.user_message());
            }
        }
        // Check the complete provider response before the legacy mapper can
        // invent tool IDs/names or swallow malformed JSON arguments.
        if state.v2_contract_mode == ContractMode::Enforce {
            if let Err(failure) = validate_completion(&value) {
                state.observability.log(
                    "error",
                    "v2 provider response",
                    &format!("Request {request_id}: invalid provider tool response ({failure:?})."),
                );
                return api_error(StatusCode::BAD_GATEWAY, "api_error", failure.user_message());
            }
        }
        drop(upstream_permit);
        let upstream_elapsed_ms = upstream_started.elapsed().as_millis();
        state.observability.log(
            "info",
            "concurrency",
            &format!(
                "Request {request_id} upstream round {} completed in {upstream_elapsed_ms} ms.",
                web_round + 1
            ),
        );

        // Transparent auto-continuation for the JSON path (used by the local
        // web tool loop and non-streaming clients): when the provider cut the
        // answer at its output budget, resume it here so the client never
        // sees a truncated `max_tokens` response.
        let mut value = value;
        {
            let client_output_budget = request
                .get("max_tokens")
                .and_then(Value::as_u64)
                .filter(|value| *value > 0)
                .or_else(|| (route.max_output_tokens > 0).then_some(route.max_output_tokens));
            let message = value
                .pointer("/choices/0/message")
                .cloned()
                .unwrap_or_else(|| json!({}));
            let finish = value
                .pointer("/choices/0/finish_reason")
                .and_then(Value::as_str)
                .unwrap_or("stop");
            let text = match message.get("content") {
                Some(Value::String(text)) => text.clone(),
                Some(Value::Array(parts)) => parts
                    .iter()
                    .filter_map(|part| part.get("text").and_then(Value::as_str))
                    .collect::<Vec<_>>()
                    .join(""),
                _ => String::new(),
            };
            if finish == "length"
                && state.v2_contract_mode != ContractMode::Enforce
                && !message.get("tool_calls").is_some_and(Value::is_array)
                && continuation_safe(&text)
            {
                let mut combined = text;
                let mut continuation_used = false;
                let mut rounds = 0usize;
                let target_str = target.to_string();
                while rounds < MAX_AUTO_CONTINUATIONS {
                    rounds += 1;
                    // Same client-budget guard as the streaming path: keep the
                    // combined response within the client's `max_tokens`
                    // budget so the client's output-token invariant holds,
                    // and cap this round to the remaining budget so the
                    // combined answer can never overshoot it.
                    let (ascii, non_ascii) = {
                        let (mut ascii, mut non_ascii) = (0u64, 0u64);
                        for ch in combined.chars() {
                            if ch.is_ascii() {
                                ascii += 1;
                            } else {
                                non_ascii += 1;
                            }
                        }
                        (ascii, non_ascii)
                    };
                    if let Some(budget) = client_output_budget {
                        let cumulative = estimate_tokens_from_counts(ascii, non_ascii);
                        if cumulative >= budget {
                            state.observability.log(
                                "info",
                                "continuation",
                                &format!(
                                    "Reached the client's {}-token output budget; keeping the partial response and letting the client resume it.",
                                    budget
                                ),
                            );
                            break;
                        }
                    }
                    let next = build_continuation_payload(&payload, &combined);
                    let cumulative = estimate_tokens_from_counts(ascii, non_ascii);
                    let remaining = budget_remaining(client_output_budget, cumulative);
                    let next = cap_continuation_max_tokens(next, client_output_budget, remaining);
                    let output_ceiling =
                        continuation_output_ceiling(&next, route.max_output_tokens, remaining);
                    state.observability.log(
                        "info",
                        "continuation",
                        &format!(
                            "Response was truncated at the provider output budget; continuing generation (round {rounds}/{MAX_AUTO_CONTINUATIONS})."
                        ),
                    );
                    let resumed = fetch_continuation(
                        &state.next_upstream_client(),
                        &state.direct_provider_client,
                        &config,
                        &target_str,
                        next,
                        route.context_window,
                        config.auto_trim_on_context_overflow,
                        output_ceiling,
                        &state.observability,
                    )
                    .await;
                    match resumed {
                        Some((part, part_finish, _, _)) => {
                            continuation_used = true;
                            combined.push_str(&part);
                            if part_finish != "length" {
                                break;
                            }
                        }
                        None => break,
                    }
                }
                if continuation_used {
                    if let Some(choice) = value
                        .get_mut("choices")
                        .and_then(Value::as_array_mut)
                        .and_then(|choices| choices.get_mut(0))
                    {
                        if let Some(message) = choice.get_mut("message") {
                            message["content"] = Value::String(combined);
                        }
                        choice["finish_reason"] = Value::String("stop".to_string());
                    }
                }
            }
        }

        if local_tools_enabled {
            if let Some(calls) = local_tool_calls(&value) {
                // A provider may hallucinate a local tool that was never
                // offered for this request. Validate the ENTIRE batch before
                // executing any tool: Web permission is never Shell permission.
                if calls
                    .iter()
                    .any(|(_, name, _)| !local_tool_allowed(name, web_enabled, shell_enabled))
                {
                    return api_error(
                        StatusCode::BAD_GATEWAY,
                        "api_error",
                        "The provider requested a local tool that is disabled for this request.",
                    );
                }
                if web_round >= WEB_TOOL_MAX_ROUNDS {
                    return api_error(
                        StatusCode::BAD_GATEWAY,
                        "api_error",
                        "The local tool loop exceeded its safety limit. Start a new request and try a narrower question.",
                    );
                }
                web_round += 1;
                let mut assistant = value
                    .pointer("/choices/0/message")
                    .cloned()
                    .unwrap_or_else(|| json!({ "role": "assistant", "content": null }));
                // Keep the assistant tool-call message consistent with the
                // canonical names used by the next upstream request when a
                // compatible model emitted one of the supported aliases.
                normalize_web_assistant_message(&mut assistant);
                if let Some(messages) = payload.get_mut("messages").and_then(Value::as_array_mut) {
                    messages.push(assistant);
                }
                for (id, name, arguments) in calls {
                    let slots = if name == "claude_proxy_shell" {
                        &state.shell_slots
                    } else {
                        &state.web_slots
                    };
                    let local_permit = match slots.clone().acquire_owned().await {
                        Ok(permit) => permit,
                        Err(_) => {
                            return api_error(
                                StatusCode::SERVICE_UNAVAILABLE,
                                "api_error",
                                "The proxy is shutting down and cannot run another local tool.",
                            )
                        }
                    };
                    let local_started = Instant::now();
                    let result =
                        execute_local_tool(&config, &state.observability, &name, &arguments).await;
                    drop(local_permit);
                    web_tool_count += 1;
                    let local_elapsed_ms = local_started.elapsed().as_millis();
                    state.observability.log(
                        "info",
                        "web bridge",
                        &format!(
                            "Request {request_id} ran local {name} tool in {local_elapsed_ms} ms."
                        ),
                    );
                    if let Some(messages) =
                        payload.get_mut("messages").and_then(Value::as_array_mut)
                    {
                        messages.push(json!({
                            "role": "tool",
                            "tool_call_id": id,
                            "content": result
                        }));
                    }
                }
                // The first call may be forced in Required mode. Further
                // calls are left to the model so it can fetch a source after
                // search or stop when it has enough evidence.
                payload["tool_choice"] = json!("auto");
                continue;
            }
        }
        break value;
    };
    let total_elapsed_ms = request_started.elapsed().as_millis();
    state.observability.log(
        "info",
        "concurrency",
        &format!("Request {request_id} completed {} model round(s) and {web_tool_count} web tool call(s) in {total_elapsed_ms} ms.", web_round + 1),
    );
    if trimmed_turns > proactive_trimmed_turns {
        state.observability.log(
            "warning",
            "context",
            &format!("Context budgeting trimmed {trimmed_turns} older conversation turn(s) for this request."),
        );
    }
    let estimated_input_tokens = estimate_tokens(&payload);
    match openai_to_anthropic(&completion, requested_model) {
        Ok(mut body) => {
            let provider_usage = completion.get("usage").unwrap_or(&Value::Null);
            let parsed_usage = parse_provider_usage(provider_usage);
            let actual_input = parsed_usage.input_tokens;
            let actual_output = parsed_usage.output_tokens;
            let input_tokens = actual_input.unwrap_or(estimated_input_tokens);
            let output_tokens = actual_output.unwrap_or_else(|| {
                estimate_tokens(
                    completion
                        .pointer("/choices/0/message")
                        .unwrap_or(&Value::Null),
                )
            });
            let cache_stats_reported = parsed_usage.cache_stats_reported && actual_input.is_some();
            let cached_input_tokens = if cache_stats_reported {
                parsed_usage
                    .cached_input_tokens
                    .unwrap_or_default()
                    .min(input_tokens)
            } else {
                0
            };
            let cache_write_input_tokens = if cache_stats_reported {
                parsed_usage
                    .cache_write_input_tokens
                    .unwrap_or_default()
                    .min(input_tokens.saturating_sub(cached_input_tokens))
            } else {
                0
            };
            if let Some(usage) = body.get_mut("usage") {
                usage["input_tokens"] = json!(input_tokens);
                usage["output_tokens"] = json!(output_tokens);
            }
            if let Err(error) = state.observability.record_usage(
                &provider_label(&config),
                &route.upstream_model,
                requested_model,
                input_tokens,
                output_tokens,
                cached_input_tokens,
                cache_write_input_tokens,
                if cache_stats_reported {
                    actual_input.unwrap_or_default()
                } else {
                    0
                },
                cache_stats_reported,
                parsed_usage.cached_input_tokens.is_some() && actual_input.is_some(),
                parsed_usage.cache_write_input_tokens.is_some() && actual_input.is_some(),
                actual_input.is_none(),
                actual_output.is_none(),
            ) {
                state.observability.log(
                    "warning",
                    "usage",
                    &format!("Could not save token usage history: {error}"),
                );
            }
            if client_wants_stream && local_tools_enabled {
                return stream_anthropic_message(&body, trimmed_turns);
            }
            let mut response = cors_response(Json(body).into_response());
            if trimmed_turns > 0 {
                response.headers_mut().insert(
                    "x-qbs-context-trimmed-turns",
                    HeaderValue::from_str(&trimmed_turns.to_string())
                        .unwrap_or(HeaderValue::from_static("1")),
                );
            }
            response
        }
        Err(error) => {
            state.observability.log(
                "error",
                "provider",
                "The provider response could not be converted to a Claude response.",
            );
            api_error(StatusCode::BAD_GATEWAY, "api_error", &error)
        }
    }
}

fn is_context_overflow(message: &str) -> bool {
    let message = message.to_ascii_lowercase();
    (message.contains("input length")
        && (message.contains("maximum allowed") || message.contains("exceeds")))
        || ((message.contains("context length")
            || message.contains("context window")
            || message.contains("context length exceeded"))
            && (message.contains("exceed") || message.contains("too long")))
}

fn provider_error_identifier(body: &str) -> Option<String> {
    let value = serde_json::from_str::<Value>(body).ok()?;
    let error = value.get("error").unwrap_or(&value);
    ["code", "type"].into_iter().find_map(|field| {
        let identifier = error.get(field)?.as_str()?.trim();
        let safe = !identifier.is_empty()
            && identifier.len() <= 80
            && identifier
                .chars()
                .all(|character| character.is_ascii_alphanumeric() || "_.-".contains(character));
        safe.then(|| identifier.to_owned())
    })
}

fn provider_transport_error_kind(error: &reqwest::Error) -> &'static str {
    if error.is_timeout() {
        "timeout"
    } else if error.is_connect() {
        "connection failure"
    } else {
        "transport failure"
    }
}

fn is_stream_usage_unsupported(message: &str) -> bool {
    let message = message.to_ascii_lowercase();
    let mentions_option = message.contains("stream_options") || message.contains("include_usage");
    let rejects_option = [
        "unsupported",
        "unknown",
        "unrecognized",
        "not allowed",
        "invalid",
        "extra fields",
        "additional properties",
    ]
    .iter()
    .any(|phrase| message.contains(phrase));
    mentions_option && rejects_option
}

fn trim_oldest_turn(payload: &mut Value) -> bool {
    let Some(messages) = payload.get_mut("messages").and_then(Value::as_array_mut) else {
        return false;
    };
    let Some(first_user) = messages
        .iter()
        .position(|message| message.get("role").and_then(Value::as_str) == Some("user"))
    else {
        return false;
    };
    let Some(next_user) =
        messages
            .iter()
            .enumerate()
            .skip(first_user + 1)
            .find_map(|(index, message)| {
                (message.get("role").and_then(Value::as_str) == Some("user")).then_some(index)
            })
    else {
        return false;
    };
    // Cowork can send multiple setup/user messages before any model answer.
    // That is NOT a complete historical turn and cannot be trimmed. A
    // complete removable turn must contain an assistant reply before the
    // next user message. Otherwise we would delete essential instructions
    // while falsely logging "removed an older conversation turn".
    if !messages[first_user + 1..next_user]
        .iter()
        .any(|message| message.get("role").and_then(Value::as_str) == Some("assistant"))
    {
        return false;
    }
    messages.drain(first_user..next_user);
    true
}

fn trim_to_context_budget(payload: &mut Value, context_window: u64, output_budget: u64) -> usize {
    if context_window == 0 {
        return 0;
    }
    let reserved_output = output_budget.min(context_window.saturating_sub(1));
    let input_budget = context_window
        .saturating_sub(reserved_output)
        .saturating_sub(512)
        .max(1);
    let mut trimmed_turns = 0usize;
    while estimate_tokens(payload) > input_budget && trim_oldest_turn(payload) {
        trimmed_turns += 1;
    }
    trimmed_turns
}

fn resolve_route<'a>(routes: &'a [ModelRoute], requested: &str) -> Option<&'a ModelRoute> {
    let requested = requested.trim();
    routes
        .iter()
        .find(|route| route.route_id.eq_ignore_ascii_case(requested))
        .or_else(|| {
            let without_suffix = requested.split("[1m]").next().unwrap_or(requested);
            routes.iter().find(|route| {
                without_suffix.starts_with(&route.route_id)
                    && without_suffix[route.route_id.len()..]
                        .chars()
                        .all(|c| c == '-' || c.is_ascii_digit())
            })
        })
}

/// Compare the ordered, non-empty human-text segments, not the last 'user'
/// role. In Claude, tool results arrive inside user messages that may contain
/// no human text; after translation they become OpenAI 'tool' messages.
fn ordered_user_texts(value: &Value) -> Vec<String> {
    value
        .get("messages")
        .and_then(Value::as_array)
        .map(|messages| {
            messages
                .iter()
                .filter(|message| message.get("role").and_then(Value::as_str) == Some("user"))
                .map(|message| anthropic_text(message.get("content").unwrap_or(&Value::Null)))
                .filter(|text| !text.is_empty())
                .collect()
        })
        .unwrap_or_default()
}

fn user_text_order_preserved(request: &Value, payload: &Value) -> bool {
    ordered_user_texts(request) == ordered_user_texts(payload)
}

fn cowork_control_content(content: &Value) -> Result<String, String> {
    match content {
        Value::String(text) => Ok(text.clone()),
        Value::Array(blocks)
            if blocks.iter().all(|block| {
                block.get("type").and_then(Value::as_str) == Some("text")
                    && block.get("text").and_then(Value::as_str).is_some()
            }) =>
        {
            Ok(anthropic_text(content))
        }
        _ => Err(
            "COWORK_CONTROL_MESSAGE_CONTENT: system/developer message must contain text blocks."
                .into(),
        ),
    }
}

fn cowork_tool_content(content: &Value) -> Result<String, String> {
    match content {
        Value::String(text) => Ok(text.clone()),
        Value::Null => Ok(String::new()),
        Value::Array(blocks)
            if blocks.iter().all(|block| {
                block.get("type").and_then(Value::as_str) == Some("text")
                    && block.get("text").and_then(Value::as_str).is_some()
            }) =>
        {
            Ok(anthropic_text(content))
        }
        Value::Object(_) => Ok(content.to_string()),
        _ => Err(
            "COWORK_TOOL_RESULT_CONTENT: native tool result contains unsupported content.".into(),
        ),
    }
}

fn anthropic_to_openai(request: &Value, route: &ModelRoute) -> Result<Value, String> {
    let mut messages = Vec::new();
    if let Some(system) = request.get("system") {
        let text = anthropic_text(system);
        if !text.trim().is_empty() {
            messages.push(json!({ "role": "system", "content": text }));
        }
    }
    if let Some(input_messages) = request.get("messages").and_then(Value::as_array) {
        for (index, message) in input_messages.iter().enumerate() {
            let role = message
                .get("role")
                .and_then(Value::as_str)
                .ok_or_else(|| format!("COWORK_MISSING_MESSAGE_ROLE: message_index={index}."))?;
            let content = message.get("content").unwrap_or(&Value::Null);
            match role {
                "assistant" => {
                    let (body, calls) = assistant_content(content);
                    let has_calls = !calls.is_empty();
                    let mut item = json!({ "role": "assistant", "content": body });
                    if !calls.is_empty() {
                        item["tool_calls"] = Value::Array(calls);
                    }
                    if body != Value::Null || has_calls {
                        messages.push(item);
                    }
                }
                "user" => {
                    let (body, results) = user_content(content);
                    // A completed assistant tool_call must be followed by its
                    // tool response before any new user instruction. Anthropic
                    // permits a tool_result and text in one user content array,
                    // but Chat Completions requires the tool reply first.
                    messages.extend(results);
                    if body != Value::Null {
                        messages.push(json!({ "role": "user", "content": body }));
                    }
                }
                "system" | "developer" => {
                    // Control instructions must stay control instructions.
                    // Never change their priority by relabelling them as user.
                    let text = cowork_control_content(content)?;
                    messages.push(json!({ "role": role, "content": text }));
                }
                "tool" | "tool_result" => {
                    // Some Cowork transports send native tool-role messages
                    // instead of wrapping tool_result inside user content.
                    // Without a call ID, we cannot safely attach the result.
                    let call_id = message
                        .get("tool_call_id")
                        .or_else(|| message.get("tool_use_id"))
                        .and_then(Value::as_str)
                        .filter(|value| !value.trim().is_empty())
                        .ok_or_else(|| {
                            format!(
                                "COWORK_TOOL_RESULT_MISSING_ID: message_index={index}; no tool result was attributed to a fabricated call."
                            )
                        })?;
                    let text = cowork_tool_content(content)?;
                    messages.push(json!({
                        "role": "tool",
                        "tool_call_id": call_id,
                        "content": text
                    }));
                }
                _ => {
                    // Role identifiers are protocol metadata. Only expose a
                    // short safe identifier, never client content or secrets.
                    let display = if role.len() <= 32
                        && role.chars().all(|ch| ch.is_ascii_lowercase() || ch == '_')
                    {
                        role
                    } else {
                        "other"
                    };
                    return Err(format!(
                        "COWORK_UNSUPPORTED_MESSAGE_ROLE: message_index={index}, role={display}. Supported roles: user, assistant, system, developer, tool."
                    ));
                }
            }
        }
    }
    if messages.is_empty() {
        return Err("The request contains no messages that can be sent.".into());
    }
    if messages
        .first()
        .and_then(|message| message.get("role"))
        .and_then(Value::as_str)
        == Some("system")
    {
        let system_messages: Vec<Value> = messages
            .iter()
            .filter(|message| message.get("role").and_then(Value::as_str) == Some("system"))
            .cloned()
            .collect();
        if system_messages.len() > 1 {
            let combined = system_messages
                .iter()
                .filter_map(|message| message.get("content").and_then(Value::as_str))
                .collect::<Vec<_>>()
                .join("\n\n");
            messages
                .retain(|message| message.get("role").and_then(Value::as_str) != Some("system"));
            messages.insert(0, json!({ "role": "system", "content": combined }));
        }
    }

    let mut output = json!({ "model": route.upstream_model, "messages": messages });
    for key in [
        "temperature",
        "top_p",
        "top_k",
        "stream",
        "parallel_tool_calls",
        "metadata",
        "seed",
    ] {
        if let Some(value) = request.get(key) {
            output[key] = value.clone();
        }
    }
    apply_reasoning_controls(request, &mut output, route);
    if let Some(stop) = request.get("stop_sequences").and_then(Value::as_array) {
        output["stop"] = Value::Array(stop.clone());
    }
    if let Some(tools) = request.get("tools").and_then(Value::as_array) {
        let converted: Vec<Value> = tools.iter().filter(|tool| tool.get("type").and_then(Value::as_str).is_none_or(|kind| kind == "custom" || kind == "function")).map(|tool| {
            let name = tool.get("name").cloned().unwrap_or(Value::String(String::new()));
            let description = tool.get("description").cloned().unwrap_or(Value::String(String::new()));
            let parameters = tool.get("input_schema").or_else(|| tool.get("parameters")).cloned().unwrap_or(json!({ "type": "object", "properties": {} }));
            json!({ "type": "function", "function": { "name": name, "description": description, "parameters": parameters } })
        }).collect();
        if !converted.is_empty() {
            output["tools"] = Value::Array(converted);
        }
    }
    if let Some(choice) = request.get("tool_choice") {
        let converted = match choice.get("type").and_then(Value::as_str) {
            Some("auto") => Some(json!("auto")),
            Some("any") => Some(json!("required")),
            Some("tool") => choice
                .get("name")
                .map(|name| json!({ "type": "function", "function": { "name": name } })),
            _ => None,
        };
        if let Some(choice) = converted {
            output["tool_choice"] = choice;
        }
    }
    if let Some(requested) = request.get("max_tokens").and_then(Value::as_u64) {
        let mut cap = requested;
        if route.max_output_tokens > 0 {
            cap = cap.min(route.max_output_tokens);
        }
        if route.context_window > 0 {
            let input = estimate_tokens(request).saturating_add(512);
            cap = cap.min(route.context_window.saturating_sub(input).max(1));
        }
        output["max_tokens"] = json!(cap.max(1));
    }
    Ok(output)
}

/// Translate Claude's per-request reasoning controls to the OpenAI-compatible
/// extensions understood by Qwen/SGLang providers. Everything is derived from
/// this request and written into its payload, so changing effort in one chat
/// cannot mutate another chat or a global proxy setting.
fn apply_reasoning_controls(request: &Value, output: &mut Value, route: &ModelRoute) {
    let effort = request
        .get("effort")
        .or_else(|| {
            request
                .get("output_config")
                .and_then(|value| value.get("effort"))
        })
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned);

    let thinking_type = request
        .get("thinking")
        .and_then(|value| value.get("type"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned);

    let mut chat_template_kwargs = request
        .get("chat_template_kwargs")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();

    if let Some(effort) = effort {
        let value = Value::String(effort);
        // Some OpenAI-compatible gateways consume the normalized top-level
        // field, while SGLang consumes the same value in chat_template_kwargs.
        output["reasoning_effort"] = value.clone();
        chat_template_kwargs.insert("reasoning_effort".into(), value);
    }

    if let Some(thinking_type) = thinking_type {
        match thinking_type.as_str() {
            "disabled" | "none" => {
                chat_template_kwargs.insert("enable_thinking".into(), json!(false));
            }
            "enabled" | "adaptive" => {
                chat_template_kwargs.insert("enable_thinking".into(), json!(true));
            }
            _ => {}
        }
    }

    // The Anthropic client can ask for adaptive thinking while the selected
    // Qwen provider explicitly requires a non-thinking chat template.
    // Provider route policy wins; never send contradictory effort signals.
    match effective_thinking_mode(route) {
        "disabled" => {
            chat_template_kwargs.remove("reasoning_effort");
            chat_template_kwargs.insert("enable_thinking".into(), json!(false));
            if let Some(object) = output.as_object_mut() {
                object.remove("reasoning_effort");
            }
        }
        "enabled" => {
            chat_template_kwargs.insert("enable_thinking".into(), json!(true));
        }
        _ => {}
    }

    if !chat_template_kwargs.is_empty() {
        output["chat_template_kwargs"] = Value::Object(chat_template_kwargs);
    }
}

fn web_tools_enabled(config: &AppConfig) -> bool {
    config.web_access_mode == "auto" || config.web_access_mode == "required"
}

fn web_tool_definitions() -> Vec<Value> {
    vec![
        json!({
            "type": "function",
            "function": {
                "name": "claude_proxy_web_search",
                "description": "Search the public web when the user asks for current, recent, changing, or source-based information. Use this before answering questions about news, prices, current software documentation, live events, or facts that may have changed. The tool runs on the user's computer and returns titles, URLs, and short snippets.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "query": { "type": "string", "description": "A focused web search query." },
                        "domains": { "type": "array", "items": { "type": "string" }, "description": "Optional domain allowlist such as [\"docs.rs\", \"qwen.readthedocs.io\"]." },
                        "recency_days": { "type": "integer", "minimum": 1, "maximum": 3650, "description": "Optional recency window in days." }
                    },
                    "required": ["query"]
                }
            }
        }),
        json!({
            "type": "function",
            "function": {
                "name": "claude_proxy_web_fetch",
                "description": "Read a public HTTPS or HTTP web page when the user needs the contents of a specific URL or when a search result must be inspected. Only fetch public web addresses; do not use this tool for local files, localhost, private network addresses, credentials, or internal services.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "url": { "type": "string", "description": "The public HTTP or HTTPS URL to read." },
                        "max_chars": { "type": "integer", "minimum": 1000, "maximum": WEB_FETCH_MAX_CHARS, "description": "Maximum amount of page text to return." }
                    },
                    "required": ["url"]
                }
            }
        }),
    ]
}

fn add_web_tools(payload: &mut Value, config: &AppConfig, web_intent: WebIntent) -> bool {
    if !web_tools_enabled(config)
        || matches!(web_intent, WebIntent::NoWeb | WebIntent::ReuseContext)
    {
        return false;
    }
    let instruction = web_bridge_system_instruction(config, web_intent);
    if let Some(messages) = payload.get_mut("messages").and_then(Value::as_array_mut) {
        if let Some(system) = messages
            .iter_mut()
            .find(|message| message.get("role").and_then(Value::as_str) == Some("system"))
        {
            if let Some(content) = system
                .get("content")
                .and_then(Value::as_str)
                .map(str::to_owned)
            {
                if !content.contains(WEB_BRIDGE_SYSTEM_MARKER) {
                    let mut content = content;
                    if !content.trim().is_empty() {
                        content.push_str("\n\n");
                    }
                    content.push_str(&instruction);
                    system["content"] = Value::String(content);
                }
            }
        } else {
            messages.insert(
                0,
                json!({
                    "role": "system",
                    "content": instruction
                }),
            );
        }
    }
    if !payload.get("tools").is_some_and(Value::is_array) {
        payload["tools"] = Value::Array(Vec::new());
    }
    let tools = payload
        .get_mut("tools")
        .and_then(Value::as_array_mut)
        .expect("web tool list was initialized as an array");
    let names: HashSet<String> = tools
        .iter()
        .filter_map(|tool| tool.pointer("/function/name").and_then(Value::as_str))
        .map(str::to_owned)
        .collect();
    for tool in web_tool_definitions() {
        let name = tool
            .pointer("/function/name")
            .and_then(Value::as_str)
            .unwrap_or_default();
        if !names.contains(name) {
            tools.push(tool);
        }
    }
    if web_intent == WebIntent::Fetch {
        payload["tool_choice"] = json!({
            "type": "function",
            "function": { "name": "claude_proxy_web_fetch" }
        });
    } else if web_intent == WebIntent::Search
        || (config.web_access_mode == "required" && web_intent != WebIntent::ReuseContext)
    {
        payload["tool_choice"] = json!({
            "type": "function",
            "function": { "name": "claude_proxy_web_search" }
        });
    }
    true
}

fn web_bridge_system_instruction(config: &AppConfig, web_intent: WebIntent) -> String {
    let mut instruction = format!(
        "{WEB_BRIDGE_SYSTEM_MARKER} Use only the provided claude_proxy_web_search and claude_proxy_web_fetch tools for web access; they run on the user's computer. Do not use provider-side, Cowork, browser, or other external web tools. In automatic mode, answer stable questions without a web call and search only when current information or sources are needed. For GitHub repository work, use the bridge when the user first names or links a repository. In later turns, reuse fetched context for opinions and explanations; fetch the repository again when asked to inspect or change its current code."
    );
    if web_intent == WebIntent::Fetch {
        instruction.push_str(" Fetch the latest GitHub URL supplied in this conversation directly, without searching first.");
    } else if web_intent == WebIntent::Search {
        instruction.push_str(" The current turn is about finding a GitHub/repository source; search the web for relevant repository results.");
    } else if web_intent == WebIntent::ReuseContext {
        instruction.push_str(" The current turn asks for an opinion or explanation about the repository already discussed; answer from the conversation context and do not search again.");
    }
    if !config.web_preferred_domains.is_empty() {
        instruction.push_str(" Preferred source domains when relevant (these are source hints, not a network allowlist): ");
        instruction.push_str(&config.web_preferred_domains.join(", "));
        instruction.push('.');
    }
    instruction
}

fn build_web_client(config: &AppConfig) -> Result<Client, String> {
    let mut builder = Client::builder()
        .connect_timeout(Duration::from_secs(15))
        .read_timeout(Duration::from_secs(45))
        .pool_idle_timeout(Duration::from_secs(30))
        .redirect(Policy::none())
        .user_agent("Claude-Proxy-WebBridge/2.0.9");
    if config.web_proxy_mode == "manual" {
        let proxy_url = normalize_web_proxy_url(&config.web_proxy_url)?;
        if proxy_url.is_empty() {
            return Err("A manual web proxy address has not been configured.".into());
        }
        builder = builder.proxy(
            Proxy::all(proxy_url)
                .map_err(|error| format!("The web proxy could not be configured: {error}"))?,
        );
    } else {
        // Direct mode must not silently inherit HTTP(S)_PROXY from the parent
        // process. The user selected the route explicitly in the UI.
        builder = builder.no_proxy();
    }
    builder
        .build()
        .map_err(|error| format!("The web client could not be initialized: {error}"))
}

fn is_private_ip(ip: std::net::IpAddr) -> bool {
    match ip {
        std::net::IpAddr::V4(ip) => {
            ip.is_loopback()
                || ip.is_private()
                || ip.is_link_local()
                || ip.is_unspecified()
                || ip.is_broadcast()
                || ip.is_documentation()
                || ip.octets()[0] == 100 && (64..=127).contains(&ip.octets()[1])
        }
        std::net::IpAddr::V6(ip) => {
            ip.is_loopback()
                || ip.is_unspecified()
                || ip.is_unique_local()
                || ip.is_unicast_link_local()
                || ip.is_multicast()
        }
    }
}

fn validate_web_url(raw: &str) -> Result<Url, String> {
    let url = Url::parse(raw).map_err(|error| format!("The web URL is invalid: {error}"))?;
    if !matches!(url.scheme(), "http" | "https") {
        return Err("Web tools only allow http:// and https:// URLs.".into());
    }
    if url.host_str().is_none() || !url.username().is_empty() || url.password().is_some() {
        return Err("The web URL must contain a public host without credentials.".into());
    }
    let host = url.host_str().unwrap_or_default().to_ascii_lowercase();
    if host == "localhost"
        || host.ends_with(".localhost")
        || host.ends_with(".local")
        || host.ends_with(".internal")
        || host == "metadata.google.internal"
        || host == "169.254.169.254"
    {
        return Err("Web tools cannot access local, private, or metadata hosts.".into());
    }
    if let Ok(ip) = host.parse::<std::net::IpAddr>() {
        if is_private_ip(ip) {
            return Err("Web tools cannot access local or private IP addresses.".into());
        }
    }
    Ok(url)
}

async fn validate_resolved_web_host(url: &Url) -> Result<(), String> {
    let Some(host) = url.host_str() else {
        return Err("The web URL must contain a public host.".into());
    };
    if host.parse::<std::net::IpAddr>().is_ok() {
        return Ok(());
    }
    let port = url.port_or_known_default().unwrap_or(443);
    let addresses = tokio::net::lookup_host((host, port))
        .await
        .map_err(|error| format!("The web host could not be resolved: {error}"))?
        .collect::<Vec<SocketAddr>>();
    if addresses.is_empty() || addresses.iter().any(|address| is_private_ip(address.ip())) {
        return Err("The web host resolves to a local or private address and was blocked.".into());
    }
    Ok(())
}

async fn web_get(client: &Client, initial_url: Url) -> Result<reqwest::Response, String> {
    let mut url = initial_url;
    for _ in 0..4 {
        validate_web_url(url.as_str())?;
        validate_resolved_web_host(&url).await?;
        let response = client
            .get(url.clone())
            .header(
                reqwest::header::ACCEPT,
                "text/html, application/xhtml+xml, application/json, text/plain;q=0.8",
            )
            .send()
            .await
            .map_err(|error| format!("The web request failed: {error}"))?;
        if response.status().is_redirection() {
            let location = response
                .headers()
                .get(reqwest::header::LOCATION)
                .and_then(|value| value.to_str().ok())
                .ok_or_else(|| {
                    "The web server returned a redirect without a location.".to_string()
                })?;
            url = url
                .join(location)
                .map_err(|error| format!("The web redirect is invalid: {error}"))?;
            continue;
        }
        return Ok(response);
    }
    Err("The web request exceeded the redirect limit.".into())
}

fn decode_html_entities(value: &str) -> String {
    value
        .replace("&amp;", "&")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&#x27;", "'")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&nbsp;", " ")
}

fn html_to_text(html: &str) -> String {
    let mut text = String::with_capacity(html.len().min(WEB_FETCH_MAX_CHARS * 2));
    let mut in_tag = false;
    let mut in_ignored = false;
    let mut tag = String::new();
    for character in html.chars() {
        if character == '<' {
            in_tag = true;
            tag.clear();
            continue;
        }
        if in_tag {
            if character == '>' {
                in_tag = false;
                let normalized = tag.trim().to_ascii_lowercase();
                if normalized.starts_with("script")
                    || normalized.starts_with("style")
                    || normalized.starts_with("noscript")
                {
                    in_ignored = true;
                } else if normalized.starts_with("/script")
                    || normalized.starts_with("/style")
                    || normalized.starts_with("/noscript")
                {
                    in_ignored = false;
                } else if !in_ignored
                    && (normalized.starts_with("br")
                        || normalized.starts_with("/p")
                        || normalized.starts_with("/div")
                        || normalized.starts_with("/li")
                        || normalized.starts_with("/h"))
                {
                    text.push('\n');
                }
            } else if tag.len() < 80 {
                tag.push(character);
            }
            continue;
        }
        if !in_ignored {
            text.push(character);
        }
    }
    let decoded = decode_html_entities(&text);
    let mut result = String::new();
    for line in decoded
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
    {
        if !result.is_empty() {
            result.push('\n');
        }
        result.push_str(line);
        if result.chars().count() >= WEB_FETCH_MAX_CHARS {
            break;
        }
    }
    truncate(&result, WEB_FETCH_MAX_CHARS)
}

fn html_attr(fragment: &str, name: &str) -> Option<String> {
    let marker = format!("{name}=\"");
    let start = fragment.find(&marker)? + marker.len();
    let end = fragment[start..].find('"')? + start;
    Some(decode_html_entities(&fragment[start..end]))
}

fn extract_search_results(html: &str, base_url: &Url) -> Vec<Value> {
    let mut results = Vec::new();
    let mut cursor = 0usize;
    while results.len() < 8 {
        let Some(marker) = html[cursor..].find("result__a") else {
            break;
        };
        let marker_start = cursor + marker;
        let Some(anchor_start) = html[..marker_start].rfind("<a") else {
            break;
        };
        let Some(anchor_end_rel) = html[anchor_start..].find('>') else {
            break;
        };
        let anchor_end = anchor_start + anchor_end_rel;
        let Some(close_rel) = html[anchor_end + 1..].find("</a>") else {
            break;
        };
        let close = anchor_end + 1 + close_rel;
        let Some(raw_href) = html_attr(&html[anchor_start..=anchor_end], "href") else {
            cursor = close + 4;
            continue;
        };
        let href = base_url.join(&raw_href).ok().and_then(|url| {
            if url.path().contains("/l/") {
                url.query_pairs()
                    .find(|(key, _)| key == "uddg")
                    .map(|(_, value)| value.into_owned())
            } else {
                Some(url.to_string())
            }
        });
        let Some(href) = href else {
            cursor = close + 4;
            continue;
        };
        if validate_web_url(&href).is_err() {
            cursor = close + 4;
            continue;
        }
        let title = html_to_text(&html[anchor_end + 1..close]);
        let snippet_start = html[close + 4..]
            .find("result__snippet")
            .map(|offset| close + 4 + offset);
        let snippet = snippet_start
            .and_then(|start| html[start..].find('>').map(|offset| start + offset + 1))
            .and_then(|start| html[start..].find('<').map(|offset| start + offset))
            .map(|end| html_to_text(&html[snippet_start.unwrap_or(end)..end]))
            .unwrap_or_default();
        results.push(json!({ "title": title, "url": href, "snippet": snippet }));
        cursor = close + 4;
    }
    results
}

async fn read_bounded_web_text(mut response: reqwest::Response) -> Result<String, String> {
    let mut body = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|_| "The public web response could not be read.".to_string())?
    {
        if body.len().saturating_add(chunk.len()) > WEB_RESPONSE_MAX_BYTES {
            return Err("The public web response exceeded the 2 MiB safety limit.".into());
        }
        body.extend_from_slice(&chunk);
    }
    Ok(String::from_utf8_lossy(&body).into_owned())
}

async fn web_search(config: &AppConfig, arguments: &Value) -> String {
    let query = arguments
        .get("query")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|query| !query.is_empty())
        .unwrap_or_default();
    if query.is_empty() {
        return json!({ "error": "A non-empty search query is required." }).to_string();
    }
    if query.chars().count() > 500 {
        return json!({ "error": "The search query is too long." }).to_string();
    }
    let mut query = query.to_string();
    if let Some(domains) = arguments.get("domains").and_then(Value::as_array) {
        let sites = domains
            .iter()
            .filter_map(Value::as_str)
            .map(str::trim)
            .filter(|domain| !domain.is_empty() && !domain.contains(char::is_whitespace))
            .take(8)
            .map(|domain| format!("site:{domain}"))
            .collect::<Vec<_>>();
        if !sites.is_empty() {
            query.push(' ');
            query.push_str(&sites.join(" "));
        }
    }
    let mut endpoint = match Url::parse(&config.web_search_url) {
        Ok(url) => url,
        Err(error) => {
            return json!({ "error": format!("The search endpoint is invalid: {error}") })
                .to_string()
        }
    };
    endpoint.query_pairs_mut().append_pair("q", &query);
    if let Some(days) = arguments.get("recency_days").and_then(Value::as_u64) {
        let period = if days <= 1 {
            "d"
        } else if days <= 7 {
            "w"
        } else if days <= 31 {
            "m"
        } else {
            "y"
        };
        endpoint.query_pairs_mut().append_pair("df", period);
    }
    let client = match build_web_client(config) {
        Ok(client) => client,
        Err(error) => return json!({ "error": error }).to_string(),
    };
    let response = match web_get(&client, endpoint.clone()).await {
        Ok(response) => response,
        Err(error) => return json!({ "error": error }).to_string(),
    };
    let status = response.status();
    let body = match read_bounded_web_text(response).await {
        Ok(body) => body,
        Err(error) => return json!({ "error": error }).to_string(),
    };
    if !status.is_success() {
        return json!({ "error": format!("The search service returned HTTP {}.", status.as_u16()) }).to_string();
    }
    let results = extract_search_results(&body, &endpoint);
    if results.is_empty() {
        return json!({ "error": "Search returned no verifiable results. Do not guess or fabricate sources.", "query": query }).to_string();
    }
    let retrieved_at_unix = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    json!({ "query": query, "results": results, "retrieved_at_unix": retrieved_at_unix })
        .to_string()
}

async fn web_fetch(config: &AppConfig, arguments: &Value) -> String {
    let raw_url = arguments
        .get("url")
        .and_then(Value::as_str)
        .map(str::trim)
        .unwrap_or_default();
    let url = match validate_web_url(raw_url) {
        Ok(url) => url,
        Err(error) => return json!({ "error": error }).to_string(),
    };
    let max_chars = arguments
        .get("max_chars")
        .and_then(Value::as_u64)
        .unwrap_or(WEB_FETCH_MAX_CHARS as u64)
        .clamp(1_000, WEB_FETCH_MAX_CHARS as u64) as usize;
    let client = match build_web_client(config) {
        Ok(client) => client,
        Err(error) => return json!({ "error": error }).to_string(),
    };
    let response = match web_get(&client, url.clone()).await {
        Ok(response) => response,
        Err(error) => return json!({ "error": error }).to_string(),
    };
    let status = response.status();
    let content_type = response
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default()
        .to_ascii_lowercase();
    let source_url = response.url().to_string();
    let body = match read_bounded_web_text(response).await {
        Ok(body) => body,
        Err(error) => return json!({ "error": error, "url": url }).to_string(),
    };
    if !status.is_success() {
        return json!({ "error": format!("The page returned HTTP {}.", status.as_u16()), "url": url }).to_string();
    }
    let content = if content_type.contains("html") {
        html_to_text(&body)
    } else {
        truncate(&decode_html_entities(&body), max_chars)
    };
    let retrieved_at_unix = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    json!({ "url": url, "source_url": source_url, "retrieved_at_unix": retrieved_at_unix, "content_type": content_type, "content": truncate(&content, max_chars) })
        .to_string()
}

fn shell_tools_enabled(config: &AppConfig) -> bool {
    config.shell_access_mode == "auto"
}

fn shell_tool_definition() -> Value {
    json!({
        "type": "function",
        "function": {
            "name": "claude_proxy_shell",
            "description": "Run a single short command with the Windows command interpreter (cmd.exe) or PowerShell on the user's computer and return its combined output. Use it for read-only or narrowly scoped commands such as file/directory listings, environment checks, git or build status, and log inspection. Never use it to delete data, change system settings, install software, or run interactive programs. Commands that finish too slowly are terminated after the timeout.",
            "parameters": {
                "type": "object",
                "properties": {
                    "command": { "type": "string", "description": "The command line to run, without a shell prefix. For example: 'dir', 'git status', 'Get-ChildItem -Name'." },
                    "shell": { "type": "string", "enum": ["powershell", "cmd"], "description": "Which interpreter to use. Defaults to powershell." },
                    "working_directory": { "type": "string", "description": "Optional absolute directory the command runs in. Defaults to the proxy's working directory." },
                    "timeout_seconds": { "type": "integer", "minimum": 5, "maximum": SHELL_MAX_TIMEOUT_SECS, "description": "Optional wall-clock limit in seconds. Defaults to 60." }
                },
                "required": ["command"]
            }
        }
    })
}

fn shell_bridge_system_instruction() -> String {
    format!(
        "{SHELL_BRIDGE_SYSTEM_MARKER} Use only the provided claude_proxy_shell tool to run commands on this computer. Keep each command short, non-interactive, and scoped to the user's request. Prefer read-only commands. Do not chain long-running or interactive commands, do not enter credentials, and do not use the shell to bypass a request the user has not asked for. Report output exactly as returned, and say when a command was cancelled by the timeout instead of guessing its result."
    )
}

fn add_shell_tool(payload: &mut Value, config: &AppConfig) -> bool {
    if !shell_tools_enabled(config) {
        return false;
    }
    let instruction = shell_bridge_system_instruction();
    if let Some(messages) = payload.get_mut("messages").and_then(Value::as_array_mut) {
        if let Some(system) = messages
            .iter_mut()
            .find(|message| message.get("role").and_then(Value::as_str) == Some("system"))
        {
            if let Some(content) = system
                .get("content")
                .and_then(Value::as_str)
                .map(str::to_owned)
            {
                if !content.contains(SHELL_BRIDGE_SYSTEM_MARKER) {
                    let mut content = content;
                    if !content.trim().is_empty() {
                        content.push_str("\n\n");
                    }
                    content.push_str(&instruction);
                    system["content"] = Value::String(content);
                }
            }
        } else {
            messages.insert(
                0,
                json!({
                    "role": "system",
                    "content": instruction
                }),
            );
        }
    }
    if !payload.get("tools").is_some_and(Value::is_array) {
        payload["tools"] = Value::Array(Vec::new());
    }
    let tools = payload
        .get_mut("tools")
        .and_then(Value::as_array_mut)
        .expect("tool list was initialized as an array");
    let names: HashSet<String> = tools
        .iter()
        .filter_map(|tool| tool.pointer("/function/name").and_then(Value::as_str))
        .map(str::to_owned)
        .collect();
    let tool = shell_tool_definition();
    let name = tool
        .pointer("/function/name")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    if !names.contains(&name) {
        tools.push(tool);
    }
    true
}

/// Run a shell command synchronously; callers schedule this on the blocking
/// pool so the proxy event loop is never blocked by a slow command. Only the
/// Windows build supports the tool; elsewhere it returns a clear error so the
/// model can report that the capability is unavailable on this system.
fn run_shell_command(
    command: &str,
    shell: &str,
    working_directory: Option<&str>,
    timeout: Duration,
) -> io::Result<String> {
    let started = Instant::now();
    let mut child = if shell == "cmd" {
        let mut child = std::process::Command::new("cmd.exe");
        child.args(["/D", "/C", command]);
        child
    } else {
        let mut child = std::process::Command::new("powershell.exe");
        child.args(["/NoProfile", "/NonInteractive", "/Command", command]);
        child
    };
    if let Some(directory) = working_directory {
        child.current_dir(directory);
    }
    child.stdout(Stdio::piped()).stderr(Stdio::piped());
    let child = child.spawn().map_err(|error| {
        io::Error::new(
            error.kind(),
            format!("Could not start the shell interpreter: {error}"),
        )
    })?;
    let output = match wait_with_timeout(child, timeout) {
        Ok(output) => output,
        Err(error) => return Err(error),
    };
    let elapsed_ms = started.elapsed().as_millis();
    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    let status = output.status.code().unwrap_or(-1);
    if !output.status.success() {
        return Err(io::Error::new(
            io::ErrorKind::Other,
            format!(
                "The command exited with code {status} after {elapsed_ms} ms.\nSTDOUT:\n{}\nSTDERR:\n{}",
                truncate(&stdout, SHELL_OUTPUT_MAX_CHARS / 2),
                truncate(&stderr, SHELL_OUTPUT_MAX_CHARS / 2)
            ),
        ));
    }
    let mut text = stdout;
    if !stderr.trim().is_empty() {
        if !text.is_empty() {
            text.push('\n');
        }
        text.push_str("[stderr]\n");
        text.push_str(&stderr);
    }
    Ok(text)
}

fn wait_with_timeout(
    mut child: std::process::Child,
    timeout: Duration,
) -> io::Result<std::process::Output> {
    let stdout = child.stdout.take().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::Other,
            "The shell stdout pipe was unavailable.",
        )
    })?;
    let stderr = child.stderr.take().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::Other,
            "The shell stderr pipe was unavailable.",
        )
    })?;
    // Drain both pipes while the child runs. Waiting first can deadlock when
    // the child fills an OS pipe buffer before it exits.
    let stdout_reader = std::thread::spawn(move || {
        let mut output = Vec::new();
        let mut pipe = stdout;
        pipe.read_to_end(&mut output).map(|_| output)
    });
    let stderr_reader = std::thread::spawn(move || {
        let mut output = Vec::new();
        let mut pipe = stderr;
        pipe.read_to_end(&mut output).map(|_| output)
    });
    let deadline = Instant::now() + timeout;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) => {
                if Instant::now() >= deadline {
                    let _ = child.kill();
                    let _ = child.wait();
                    let _ = stdout_reader.join();
                    let _ = stderr_reader.join();
                    return Err(io::Error::new(
                        io::ErrorKind::TimedOut,
                        format!(
                            "The command was cancelled after {} seconds.",
                            timeout.as_secs()
                        ),
                    ));
                }
                std::thread::sleep(Duration::from_millis(100));
            }
            Err(error) => {
                let _ = child.kill();
                let _ = child.wait();
                let _ = stdout_reader.join();
                let _ = stderr_reader.join();
                return Err(error);
            }
        }
    };
    let stdout = stdout_reader
        .join()
        .map_err(|_| io::Error::new(io::ErrorKind::Other, "The shell stdout reader failed."))??;
    let stderr = stderr_reader
        .join()
        .map_err(|_| io::Error::new(io::ErrorKind::Other, "The shell stderr reader failed."))??;
    Ok(std::process::Output {
        status,
        stdout,
        stderr,
    })
}

struct ShellExecution {
    shell: String,
    command: String,
    working_directory: Option<String>,
    timeout: Duration,
}

/// Validate the shell tool's arguments into a runnable execution plan. Only
/// the two supported interpreters are accepted; anything else falls back to
/// PowerShell. The timeout is clamped to the bridge's hard limits.
fn build_shell_execution(arguments: &Value) -> Result<ShellExecution, String> {
    let command = arguments
        .get("command")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|command| !command.is_empty())
        .ok_or_else(|| "The command parameter is required.".to_string())?
        .to_string();
    if command.chars().count() > SHELL_COMMAND_MAX_CHARS {
        return Err(format!(
            "The command exceeds the {}-character limit.",
            SHELL_COMMAND_MAX_CHARS
        ));
    }
    let shell = arguments
        .get("shell")
        .and_then(Value::as_str)
        .map(str::trim)
        .map(str::to_ascii_lowercase)
        .filter(|shell| !shell.is_empty())
        .map(|shell| {
            if shell == "cmd" || shell == "cmd.exe" {
                "cmd"
            } else {
                "powershell"
            }
        })
        .unwrap_or("powershell");
    let working_directory = arguments
        .get("working_directory")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|directory| !directory.is_empty())
        .map(str::to_owned);
    let timeout_secs = arguments
        .get("timeout_seconds")
        .and_then(Value::as_u64)
        .unwrap_or(SHELL_DEFAULT_TIMEOUT_SECS)
        .clamp(5, SHELL_MAX_TIMEOUT_SECS);
    Ok(ShellExecution {
        shell: shell.to_string(),
        command,
        working_directory,
        timeout: Duration::from_secs(timeout_secs),
    })
}

async fn shell_command(observability: &ObservabilityStore, arguments: &Value) -> String {
    let execution = match build_shell_execution(arguments) {
        Ok(execution) => execution,
        Err(error) => return json!({ "error": error }).to_string(),
    };
    if !cfg!(windows) {
        return json!({ "error": "Shell access is only available in the Windows build of Claude Proxy." })
            .to_string();
    }
    let shell = execution.shell.clone();
    let command = execution.command;
    let working_directory = execution.working_directory.clone();
    let timeout = execution.timeout;
    let started = Instant::now();
    let command_for_run = command.clone();
    let shell_for_run = shell.clone();
    let run = tokio::task::spawn_blocking(move || {
        run_shell_command(
            &command_for_run,
            &shell_for_run,
            working_directory.as_deref(),
            timeout,
        )
    })
    .await;
    let result = match run {
        Ok(result) => result,
        Err(error) => {
            observability.log(
                "error",
                "shell bridge",
                &format!("The shell tool could not be scheduled: {error}"),
            );
            return json!({ "error": "The shell tool could not be scheduled on this computer." })
                .to_string();
        }
    };
    let elapsed_ms = started.elapsed().as_millis();
    match result {
        Ok(text) => {
            observability.log(
                "info",
                "shell bridge",
                &format!("The shell tool ran a {shell} command in {elapsed_ms} ms."),
            );
            let truncated = text.chars().count() > SHELL_OUTPUT_MAX_CHARS;
            json!({
                "command": command,
                "shell": shell,
                "output": truncate(&text, SHELL_OUTPUT_MAX_CHARS),
                "truncated": truncated
            })
            .to_string()
        }
        Err(error) => {
            observability.log(
                "warning",
                "shell bridge",
                &format!("A shell tool command failed after {elapsed_ms} ms: {error}"),
            );
            json!({ "error": error.to_string() }).to_string()
        }
    }
}

async fn execute_local_tool(
    config: &AppConfig,
    observability: &ObservabilityStore,
    name: &str,
    arguments: &Value,
) -> String {
    match name {
        "claude_proxy_web_search" => web_search(config, arguments).await,
        "claude_proxy_web_fetch" => web_fetch(config, arguments).await,
        "claude_proxy_shell" => shell_command(observability, arguments).await,
        _ => json!({ "error": "Unknown local tool." }).to_string(),
    }
}

/// Some OpenAI-compatible model servers return a generic web-tool name even
/// though the request contains the bridge's canonical tools. Normalize only
/// the small set of unambiguous aliases below; arbitrary client tools must
/// remain client-side and must not be executed by the proxy.
fn normalize_web_tool_name(name: &str) -> Option<&'static str> {
    match name.trim().to_ascii_lowercase().as_str() {
        "claude_proxy_web_search" | "web_search" | "web_search_preview" | "claude_web_search" => {
            Some("claude_proxy_web_search")
        }
        "claude_proxy_web_fetch" | "web_fetch" | "web_fetch_preview" | "claude_web_fetch" => {
            Some("claude_proxy_web_fetch")
        }
        "claude_proxy_shell"
        | "shell"
        | "run_shell"
        | "run_command"
        | "execute_shell_command"
        | "terminal" => Some("claude_proxy_shell"),
        _ => None,
    }
}

fn normalize_web_assistant_message(message: &mut Value) {
    let Some(calls) = message.get_mut("tool_calls").and_then(Value::as_array_mut) else {
        return;
    };
    for call in calls {
        let Some(name) = call.pointer("/function/name").and_then(Value::as_str) else {
            continue;
        };
        let Some(canonical) = normalize_web_tool_name(name) else {
            continue;
        };
        if let Some(name_value) = call.pointer_mut("/function/name") {
            *name_value = Value::String(canonical.into());
        }
    }
}

fn local_tool_allowed(name: &str, web_enabled: bool, shell_enabled: bool) -> bool {
    match name {
        "claude_proxy_web_search" | "claude_proxy_web_fetch" => web_enabled,
        "claude_proxy_shell" => shell_enabled,
        _ => false,
    }
}

/// Return only calls owned by the local tool bridge (web + shell). Calls for
/// tools supplied by the Claude client must remain client-side and be returned
/// as normal tool_use blocks instead of being executed by the proxy.
fn local_tool_calls(completion: &Value) -> Option<Vec<(String, String, Value)>> {
    let calls = completion
        .pointer("/choices/0/message/tool_calls")
        .and_then(Value::as_array)?;
    if calls.is_empty() {
        return None;
    }
    let mut result = Vec::with_capacity(calls.len());
    for call in calls {
        let id = call
            .get("id")
            .and_then(Value::as_str)
            .filter(|id| !id.trim().is_empty())
            .unwrap_or("local_tool_call")
            .to_string();
        let function = call.get("function")?;
        let name = function.get("name").and_then(Value::as_str)?;
        let canonical = normalize_web_tool_name(name)?;
        let arguments = match function.get("arguments") {
            Some(Value::String(arguments)) => serde_json::from_str(arguments).unwrap_or_else(
                |_| json!({ "error": "The model returned invalid JSON tool arguments." }),
            ),
            Some(value) => value.clone(),
            None => json!({}),
        };
        result.push((id, canonical.to_string(), arguments));
    }
    Some(result)
}

fn assistant_content(content: &Value) -> (Value, Vec<Value>) {
    let Some(blocks) = content.as_array() else {
        return (content.clone(), Vec::new());
    };
    let mut parts = Vec::new();
    let mut calls = Vec::new();
    for block in blocks {
        match block.get("type").and_then(Value::as_str) {
            Some("text") => {
                if let Some(text) = block.get("text").and_then(Value::as_str) {
                    parts.push(json!({ "type": "text", "text": text }));
                }
            }
            Some("image") => {
                if let Some(image) = anthropic_image(block) {
                    parts.push(image);
                }
            }
            Some("tool_use") => {
                let input = block.get("input").cloned().unwrap_or(json!({}));
                calls.push(json!({ "id": block.get("id").and_then(Value::as_str).unwrap_or("tool_call"), "type": "function", "function": { "name": block.get("name").and_then(Value::as_str).unwrap_or("tool"), "arguments": input.to_string() } }));
            }
            _ => {}
        }
    }
    (parts_to_openai_content(parts), calls)
}

fn user_content(content: &Value) -> (Value, Vec<Value>) {
    let Some(blocks) = content.as_array() else {
        return (content.clone(), Vec::new());
    };
    let mut parts = Vec::new();
    let mut results = Vec::new();
    for block in blocks {
        match block.get("type").and_then(Value::as_str) {
            Some("text") => {
                if let Some(text) = block.get("text").and_then(Value::as_str) {
                    parts.push(json!({ "type": "text", "text": text }));
                }
            }
            Some("image") => {
                if let Some(image) = anthropic_image(block) {
                    parts.push(image);
                }
            }
            Some("tool_result") => {
                let mut result = json!({ "role": "tool", "tool_call_id": block.get("tool_use_id").and_then(Value::as_str).unwrap_or("tool_call"), "content": anthropic_text(block.get("content").unwrap_or(&Value::Null)) });
                if block.get("is_error").and_then(Value::as_bool) == Some(true) {
                    result["content"] = json!(format!(
                        "Tool error: {}",
                        result["content"].as_str().unwrap_or("error")
                    ));
                }
                results.push(result);
            }
            _ => {}
        }
    }
    (parts_to_openai_content(parts), results)
}

fn anthropic_text(content: &Value) -> String {
    match content {
        Value::String(text) => text.clone(),
        Value::Array(blocks) => blocks
            .iter()
            .filter_map(|block| {
                if block.get("type").and_then(Value::as_str) == Some("text") {
                    block.get("text").and_then(Value::as_str)
                } else {
                    None
                }
            })
            .collect::<Vec<_>>()
            .join("\n"),
        _ => String::new(),
    }
}

fn anthropic_image(block: &Value) -> Option<Value> {
    let source = block.get("source")?;
    let media_type = source
        .get("media_type")
        .and_then(Value::as_str)
        .unwrap_or("image/png");
    let url = match source.get("type").and_then(Value::as_str) {
        Some("base64") => format!("data:{media_type};base64,{}", source.get("data")?.as_str()?),
        Some("url") => source.get("url")?.as_str()?.to_string(),
        _ => return None,
    };
    Some(json!({ "type": "image_url", "image_url": { "url": url } }))
}

fn parts_to_openai_content(parts: Vec<Value>) -> Value {
    if parts.is_empty() {
        Value::Null
    } else if parts
        .iter()
        .all(|part| part.get("type").and_then(Value::as_str) == Some("text"))
    {
        Value::String(
            parts
                .iter()
                .filter_map(|part| part.get("text").and_then(Value::as_str))
                .collect::<Vec<_>>()
                .join("\n"),
        )
    } else {
        Value::Array(parts)
    }
}

fn openai_to_anthropic(completion: &Value, requested_model: &str) -> Result<Value, String> {
    let choice = completion
        .get("choices")
        .and_then(Value::as_array)
        .and_then(|choices| choices.first())
        .ok_or_else(|| "The OpenAI-compatible response did not include choices.".to_string())?;
    let message = choice
        .get("message")
        .ok_or_else(|| "The model response did not include a message.".to_string())?;
    let mut content = Vec::new();
    let text = match message.get("content") {
        Some(Value::String(text)) => text.clone(),
        Some(Value::Array(parts)) => parts
            .iter()
            .filter_map(|part| part.get("text").and_then(Value::as_str))
            .collect::<Vec<_>>()
            .join(""),
        _ => String::new(),
    };
    if !text.is_empty() {
        content.push(json!({ "type": "text", "text": text }));
    }
    if let Some(calls) = message.get("tool_calls").and_then(Value::as_array) {
        for call in calls {
            let function = call.get("function").unwrap_or(&Value::Null);
            let input = function
                .get("arguments")
                .and_then(Value::as_str)
                .and_then(|arguments| serde_json::from_str::<Value>(arguments).ok())
                .unwrap_or(json!({}));
            content.push(json!({ "type": "tool_use", "id": call.get("id").and_then(Value::as_str).unwrap_or("tool_call"), "name": function.get("name").and_then(Value::as_str).unwrap_or("tool"), "input": input }));
        }
    }
    let finish = choice
        .get("finish_reason")
        .and_then(Value::as_str)
        .unwrap_or("stop");
    let stop_reason = if !content
        .iter()
        .all(|block| block.get("type").and_then(Value::as_str) == Some("text"))
    {
        "tool_use"
    } else if finish == "length" {
        "max_tokens"
    } else {
        "end_turn"
    };
    let usage = completion.get("usage").cloned().unwrap_or(json!({}));
    Ok(json!({
        "id": completion.get("id").and_then(Value::as_str).unwrap_or("msg_qbs2"),
        "type": "message",
        "role": "assistant",
        "model": requested_model,
        "content": content,
        "stop_reason": stop_reason,
        "stop_sequence": null,
        "usage": {
            "input_tokens": usage.get("prompt_tokens").and_then(Value::as_u64).unwrap_or(0),
            "output_tokens": usage.get("completion_tokens").and_then(Value::as_u64).unwrap_or(0)
        }
    }))
}

fn estimate_tokens(request: &Value) -> u64 {
    fn visit(value: &Value, ascii: &mut u64, non_ascii: &mut u64) {
        match value {
            Value::String(text) => {
                for character in text.chars() {
                    if character.is_ascii() {
                        *ascii += 1;
                    } else {
                        *non_ascii += 1;
                    }
                }
            }
            Value::Array(items) => {
                for item in items {
                    visit(item, ascii, non_ascii);
                }
            }
            Value::Object(items) => {
                let is_anthropic_base64_image = items.get("type").and_then(Value::as_str)
                    == Some("image")
                    && items
                        .get("source")
                        .and_then(|source| source.get("type"))
                        .and_then(Value::as_str)
                        == Some("base64");
                if is_anthropic_base64_image {
                    *ascii = ascii.saturating_add(2_304);
                    return;
                }
                for (key, item) in items {
                    if key == "url"
                        && item
                            .as_str()
                            .is_some_and(|url| url.starts_with("data:image/"))
                    {
                        *ascii = ascii.saturating_add(2_304);
                        continue;
                    }
                    visit(&Value::String(key.clone()), ascii, non_ascii);
                    visit(item, ascii, non_ascii);
                }
            }
            _ => {}
        }
    }
    let (mut ascii, mut non_ascii) = (0, 0);
    visit(request, &mut ascii, &mut non_ascii);
    ascii
        .div_ceil(3)
        .saturating_add(non_ascii)
        .saturating_add(64)
}

fn estimate_tokens_from_counts(ascii: u64, non_ascii: u64) -> u64 {
    ascii
        .div_ceil(3)
        .saturating_add(non_ascii)
        .saturating_add(64)
}

/// Best-effort estimate of the cumulative output tokens a client has received
/// so far across an initial round plus continuation rounds.
///
/// When the provider supplies real usage metadata (`*estimated` is `false`)
/// we prefer it; otherwise we fall back to the character-count heuristic used
/// everywhere else in this file. This lets the gateway keep the *client-side*
/// output budget in mind when it decides how much of a truncated response it
/// is allowed to merge back into a single Anthropic message.
fn cumulative_output_tokens(
    state_output_tokens: u64,
    state_output_estimated: bool,
    state_output_ascii: u64,
    state_output_non_ascii: u64,
    continuation_output_tokens: u64,
) -> u64 {
    let from_state = if state_output_estimated {
        estimate_tokens_from_counts(state_output_ascii, state_output_non_ascii)
    } else {
        state_output_tokens
    };
    from_state.saturating_add(continuation_output_tokens)
}

#[derive(Clone, Copy, Default)]
struct ParsedProviderUsage {
    input_tokens: Option<u64>,
    output_tokens: Option<u64>,
    cached_input_tokens: Option<u64>,
    cache_write_input_tokens: Option<u64>,
    cache_stats_reported: bool,
}

fn parse_provider_usage(usage: &Value) -> ParsedProviderUsage {
    let prompt_tokens = usage.get("prompt_tokens").and_then(Value::as_u64);
    let input_tokens = usage.get("input_tokens").and_then(Value::as_u64);
    let output_tokens = usage
        .get("completion_tokens")
        .or_else(|| usage.get("output_tokens"))
        .and_then(Value::as_u64);
    let cached_input_tokens = usage
        .pointer("/prompt_tokens_details/cached_tokens")
        .or_else(|| usage.pointer("/input_tokens_details/cached_tokens"))
        .or_else(|| usage.pointer("/input_tokens_details/input_cached_tokens"))
        .or_else(|| usage.get("cache_read_input_tokens"))
        .or_else(|| usage.get("input_cached_tokens"))
        .or_else(|| usage.get("cached_tokens"))
        .and_then(Value::as_u64);
    let cache_write_input_tokens = usage
        .get("cache_creation_input_tokens")
        .or_else(|| usage.get("cache_write_input_tokens"))
        .or_else(|| usage.get("input_cache_write_tokens"))
        .or_else(|| usage.pointer("/prompt_tokens_details/cache_write_tokens"))
        .or_else(|| usage.pointer("/input_tokens_details/cache_creation_tokens"))
        .or_else(|| usage.pointer("/input_tokens_details/cache_write_tokens"))
        .and_then(Value::as_u64);
    let cache_stats_reported = cached_input_tokens.is_some() || cache_write_input_tokens.is_some();

    // OpenAI's prompt_tokens already includes cached tokens. Anthropic-style
    // input_tokens excludes its separately reported cache read/write counts.
    let total_input_tokens = if let Some(prompt_tokens) = prompt_tokens {
        Some(prompt_tokens)
    } else {
        input_tokens.map(|input_tokens| {
            if usage.get("cache_read_input_tokens").is_some()
                || usage.get("cache_creation_input_tokens").is_some()
            {
                input_tokens
                    .saturating_add(cached_input_tokens.unwrap_or_default())
                    .saturating_add(cache_write_input_tokens.unwrap_or_default())
            } else {
                input_tokens
            }
        })
    };

    ParsedProviderUsage {
        input_tokens: total_input_tokens,
        output_tokens,
        cached_input_tokens,
        cache_write_input_tokens,
        cache_stats_reported,
    }
}

/// Convert the completed local web-tool response to the Anthropic event
/// sequence expected by a streaming Claude client. The provider interaction is
/// intentionally completed before this point; the event stream keeps the
/// client contract stable without exposing the internal web-tool rounds.
fn stream_anthropic_message(body: &Value, trimmed_turns: usize) -> Response {
    let message_id = body.get("id").and_then(Value::as_str).unwrap_or("msg_qbs2");
    let model = body.get("model").cloned().unwrap_or(Value::Null);
    let usage = body.get("usage").cloned().unwrap_or_else(|| json!({}));
    let input_tokens = usage
        .get("input_tokens")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let output_tokens = usage
        .get("output_tokens")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let mut events = Vec::new();
    events.push(encode_sse(
        "message_start",
        json!({
            "type": "message_start",
            "message": {
                "id": message_id,
                "type": "message",
                "role": "assistant",
                "model": model,
                "content": [],
                "stop_reason": null,
                "stop_sequence": null,
                "usage": { "input_tokens": input_tokens, "output_tokens": 0 }
            }
        }),
    ));
    if let Some(content) = body.get("content").and_then(Value::as_array) {
        for (index, block) in content.iter().enumerate() {
            match block.get("type").and_then(Value::as_str) {
                Some("text") => {
                    let text = block
                        .get("text")
                        .and_then(Value::as_str)
                        .unwrap_or_default();
                    events.push(encode_sse(
                        "content_block_start",
                        json!({ "type": "content_block_start", "index": index, "content_block": { "type": "text", "text": "" } }),
                    ));
                    if !text.is_empty() {
                        events.push(encode_sse(
                            "content_block_delta",
                            json!({ "type": "content_block_delta", "index": index, "delta": { "type": "text_delta", "text": text } }),
                        ));
                    }
                }
                Some("tool_use") => {
                    let id = block
                        .get("id")
                        .and_then(Value::as_str)
                        .unwrap_or("tool_call");
                    let name = block.get("name").and_then(Value::as_str).unwrap_or("tool");
                    let input = block.get("input").cloned().unwrap_or_else(|| json!({}));
                    events.push(encode_sse(
                        "content_block_start",
                        json!({ "type": "content_block_start", "index": index, "content_block": { "type": "tool_use", "id": id, "name": name, "input": {} } }),
                    ));
                    events.push(encode_sse(
                        "content_block_delta",
                        json!({ "type": "content_block_delta", "index": index, "delta": { "type": "input_json_delta", "partial_json": input.to_string() } }),
                    ));
                }
                _ => {}
            }
            if block.get("type").is_some() {
                events.push(encode_sse(
                    "content_block_stop",
                    json!({ "type": "content_block_stop", "index": index }),
                ));
            }
        }
    }
    events.push(encode_sse(
        "message_delta",
        json!({
            "type": "message_delta",
            "delta": {
                "stop_reason": body.get("stop_reason").cloned().unwrap_or(json!("end_turn")),
                "stop_sequence": null
            },
            "usage": { "output_tokens": output_tokens }
        }),
    ));
    events.push(encode_sse(
        "message_stop",
        json!({ "type": "message_stop" }),
    ));

    let mut response = Response::new(Body::from(events.join("")));
    *response.status_mut() = StatusCode::OK;
    response.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("text/event-stream; charset=utf-8"),
    );
    response.headers_mut().insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("no-cache, no-transform"),
    );
    response
        .headers_mut()
        .insert(header::CONNECTION, HeaderValue::from_static("keep-alive"));
    if trimmed_turns > 0 {
        response.headers_mut().insert(
            "x-qbs-context-trimmed-turns",
            HeaderValue::from_str(&trimmed_turns.to_string())
                .unwrap_or(HeaderValue::from_static("1")),
        );
    }
    cors_response(response)
}

fn translate_stream_chunk(
    state: &mut AnthropicStreamState,
    v2: &mut Option<StreamMachine>,
    parsed: &Value,
) -> Result<Vec<(String, Value)>, StreamContractError> {
    let legacy = state.accept(parsed);
    if let Some(machine) = v2.as_mut() {
        machine.ingest(parsed)
    } else {
        Ok(legacy)
    }
}

fn stream_openai_response(
    upstream: reqwest::Response,
    model: String,
    upstream_model: String,
    provider: String,
    input_tokens: u64,
    trimmed_turns: usize,
    observability: ObservabilityStore,
    upstream_permit: OwnedSemaphorePermit,
    continuation_client: Client,
    continuation_direct_client: Client,
    continuation_config: AppConfig,
    continuation_target: String,
    continuation_payload: Value,
    continuation_context_window: u64,
    continuation_output_limit: u64,
    client_output_budget: Option<u64>,
    v2_stream_enabled: bool,
    v2_require_measured_output: bool,
) -> Response {
    let mut bytes = upstream.bytes_stream();
    let stream = async_stream::stream! {
        let _upstream_permit = upstream_permit;
        let mut state = AnthropicStreamState {
            input_tokens,
            input_estimated: true,
            output_estimated: true,
            ..AnthropicStreamState::default()
        };
        let mut continuation_input_tokens: u64 = 0;
        let mut continuation_output_tokens: u64 = 0;
        let mut continuation_rounds = 0usize;
        let mut v2_machine = v2_stream_enabled.then(StreamMachine::default);
        let message_id = format!("msg_{}", uuid::Uuid::new_v4().simple());
        yield Ok::<Bytes, io::Error>(Bytes::from(encode_sse("message_start", json!({ "type": "message_start", "message": { "id": message_id, "type": "message", "role": "assistant", "model": model.clone(), "content": [], "stop_reason": null, "stop_sequence": null, "usage": { "input_tokens": input_tokens, "output_tokens": 0 } } }))));
        let mut buffer: Vec<u8> = Vec::new();
        let mut done = false;
        let mut stream_failed = false;
        while let Some(chunk) = bytes.next().await {
            match chunk {
                Ok(chunk) => buffer.extend_from_slice(&chunk),
                Err(error) => {
                    stream_failed = true;
                    let failure_kind = provider_transport_error_kind(&error);
                    observability.log(
                        "error",
                        "provider stream",
                        &format!("The provider stream ended with a {failure_kind}."),
                    );
                    yield Ok(Bytes::from(encode_sse("error", json!({ "type": "error", "error": { "type": "api_error", "message": error.to_string() } }))));
                    break;
                }
            }
            if buffer.len() > 16 * 1024 * 1024 {
                stream_failed = true;
                observability.log(
                    "error",
                    "provider stream",
                    "The provider stream exceeded the 16 MiB event buffer limit.",
                );
                yield Ok(Bytes::from(encode_sse("error", json!({ "type": "error", "error": { "type": "api_error", "message": "The provider stream exceeded the maximum event size." } }))));
                break;
            }
            while let Some(event) = take_sse_event(&mut buffer) {
                let data = event.lines().filter_map(|line| line.strip_prefix("data:")).map(str::trim_start).collect::<Vec<_>>().join("\n");
                if data.is_empty() { continue; }
                if data == "[DONE]" { done = true; break; }
                let Ok(parsed) = serde_json::from_str::<Value>(&data) else {
                    if v2_stream_enabled {
                        stream_failed = true;
                        observability.log("error", "v2 stream", "Invalid JSON in provider SSE event.");
                        yield Ok(Bytes::from(encode_sse("error", json!({ "type": "error", "error": { "type": "api_error", "message": "Invalid JSON in upstream SSE event." } }))));
                        break;
                    }
                    continue;
                };
                if parsed.get("error").is_some() {
                    stream_failed = true;
                    observability.log("error", "provider stream", "The provider returned an error during a streamed response.");
                    yield Ok(Bytes::from(encode_sse("error", json!({ "type": "error", "error": { "type": "api_error", "message": "The provider returned an error during the streamed response. See Diagnostics for details." } }))));
                    break;
                }
                match translate_stream_chunk(&mut state, &mut v2_machine, &parsed) {
                    Ok(events) => {
                        for (event_name, event_data) in events {
                            yield Ok(Bytes::from(encode_sse(&event_name, event_data)));
                        }
                    }
                    Err(error) => {
                        stream_failed = true;
                        observability.log("error", "v2 stream", &format!("Rejected invalid provider SSE sequence: {error:?}"));
                        yield Ok(Bytes::from(encode_sse("error", json!({ "type": "error", "error": { "type": "api_error", "message": "Invalid streamed tool-call metadata. No tool was executed." } }))));
                        break;
                    }
                }
            }
            if done || stream_failed { break; }
        }
        let tail = String::from_utf8_lossy(&buffer);
        if !stream_failed && !done && !tail.trim().is_empty() {
            for line in tail.lines().filter_map(|line| line.strip_prefix("data:")).map(str::trim_start) {
                if line == "[DONE]" { done = true; continue; }
                if let Ok(parsed) = serde_json::from_str::<Value>(line) {
                    if parsed.get("error").is_some() {
                        stream_failed = true;
                        observability.log("error", "provider stream", "The provider returned an error in the final stream event.");
                        yield Ok(Bytes::from(encode_sse("error", json!({ "type": "error", "error": { "type": "api_error", "message": "The provider returned an error during the streamed response. See Diagnostics for details." } }))));
                        break;
                    }
                    match translate_stream_chunk(&mut state, &mut v2_machine, &parsed) {
                        Ok(events) => {
                            for (event_name, event_data) in events {
                                yield Ok(Bytes::from(encode_sse(&event_name, event_data)));
                            }
                        }
                        Err(error) => {
                            stream_failed = true;
                            observability.log("error", "v2 stream", &format!("Rejected invalid final SSE sequence: {error:?}"));
                            yield Ok(Bytes::from(encode_sse("error", json!({ "type": "error", "error": { "type": "api_error", "message": "Incomplete or invalid streamed tool call." } }))));
                            break;
                        }
                    }
                }
            }
        }
        // In strict V2 mode, a provider finish_reason without the terminal
        // [DONE] is not proof that the SSE transport completed. An upstream
        // connection can close after a finish marker but before usage/error
        // frames. Legacy adapters retain their previous permissive behavior.
        let completed = if v2_stream_enabled {
            done
        } else {
            done || state.stop_reason.is_some()
        };
        if !stream_failed && !completed {
            observability.log("error", "provider stream", "The provider stream ended before a completion marker was received.");
            yield Ok(Bytes::from(encode_sse("error", json!({ "type": "error", "error": { "type": "api_error", "message": "The provider stream ended before the response completed. Retry the request." } }))));
            stream_failed = true;
        }
        if !stream_failed
            && !v2_stream_enabled
            && state.stop_reason.as_deref() == Some("length")
            && state.tools.is_empty()
            && continuation_safe(&state.text_content)
        {
            // The provider hit its per-response output budget. Instead of
            // surfacing a truncated `max_tokens` response (which the client
            // treats as a fatal error and abandons the whole run), ask the
            // provider to resume right where the cut happened and merge the
            // extra rounds into this same streamed message. The client sees
            // one long `end_turn` response.
            //
            // Snapshot the first round's usage: each continuation round's
            // usage metadata overwrites `state.input_tokens`/`output_tokens`
            // inside `accept`, so restore the totals afterwards.
            let base_input_tokens = state.input_tokens;
            let base_output_tokens = state.output_tokens;
            let base_input_estimated = state.input_estimated;
            let base_output_estimated = state.output_estimated;
            while continuation_rounds < MAX_AUTO_CONTINUATIONS
                && state.stop_reason.as_deref() == Some("length")
            {
                continuation_rounds += 1;
                // Keep the client's own output-token invariant intact. The
                // desktop/Cowork client hard-fails the whole run ("response
                // exceeded the 32000 output token maximum") if the total
                // output tokens of one assistant message exceed the budget it
                // requested (`max_tokens`). If we keep merging continuation
                // rounds into this single message we would silently blow past
                // that budget and break a perfectly long session. Instead,
                // stop as soon as we have delivered ~the client's budget and
                // hand the rest back as a normal `max_tokens` truncation: the
                // client's own continue loop then transparently resumes it as
                // the next turn.
                if let Some(budget) = client_output_budget {
                    let cumulative = cumulative_output_tokens(
                        state.output_tokens,
                        state.output_estimated,
                        state.output_ascii_chars,
                        state.output_non_ascii_chars,
                        continuation_output_tokens,
                    );
                    if cumulative >= budget {
                        observability.log(
                            "info",
                            "continuation",
                            &format!(
                                "Reached the client's {}-token output budget; closing the response as max_tokens so the client resumes it as a new turn.",
                                budget
                            ),
                        );
                        break;
                    }
                }
                let next = build_continuation_payload(&continuation_payload, &state.text_content);
                // Cap this round's output to the *remaining* client budget so
                // the total merged output can never exceed the client's
                // invariant. With this cap in place the break above is a
                // belt-and-suspenders guard rather than the only protection.
                let cumulative = cumulative_output_tokens(
                    state.output_tokens,
                    state.output_estimated,
                    state.output_ascii_chars,
                    state.output_non_ascii_chars,
                    continuation_output_tokens,
                );
                let remaining = budget_remaining(client_output_budget, cumulative);
                let next = cap_continuation_max_tokens(next, client_output_budget, remaining);
                let output_ceiling = continuation_output_ceiling(
                    &next,
                    continuation_output_limit,
                    remaining,
                );
                observability.log(
                    "info",
                    "continuation",
                    &format!(
                        "Response was truncated at the provider output budget; continuing generation (round {continuation_rounds}/{MAX_AUTO_CONTINUATIONS})."
                    ),
                );
                match fetch_continuation(
                    &continuation_client,
                    &continuation_direct_client,
                    &continuation_config,
                    &continuation_target,
                    next,
                    continuation_context_window,
                    continuation_config.auto_trim_on_context_overflow,
                    output_ceiling,
                    &observability,
                )
                .await
                {
                    Some((text, finish_reason, round_input, round_output)) => {
                        continuation_input_tokens = continuation_input_tokens.saturating_add(round_input);
                        continuation_output_tokens = continuation_output_tokens.saturating_add(round_output);
                        let events = state.accept(&json!({
                            "choices": [{
                                "delta": { "content": text },
                                "finish_reason": finish_reason
                            }]
                        }));
                        for (event_name, event_data) in events {
                            yield Ok(Bytes::from(encode_sse(&event_name, event_data)));
                        }
                    }
                    None => break,
                }
            }
            if continuation_rounds > 0 {
                // Combine first-round and continuation-round usage. When a
                // round omitted usage metadata, keep the character-based
                // estimate for that part.
                let last_output = state.output_tokens;
                let last_input = state.input_tokens;
                let continuation_input =
                    continuation_input_tokens.max(last_input.saturating_sub(base_input_tokens));
                state.input_tokens = base_input_tokens.saturating_add(continuation_input);
                if base_input_estimated || !state.input_estimated {
                    state.input_estimated = false;
                }
                state.output_tokens =
                    base_output_tokens.saturating_add(continuation_output_tokens.max(last_output));
                if base_output_estimated || !state.output_estimated {
                    state.output_estimated = false;
                }
            }
            // Final invariant: the response we are about to close must not
            // report more output tokens than the client's budget, otherwise
            // the client hard-fails the run. If a continuation round landed
            // the total just over the line, clamp the reported figure so the
            // message stays within the client's invariant; the remainder is
            // still resumed by the client's own continue loop on the next
            // turn.
            if let Some(budget) = client_output_budget {
                let reported = if state.output_estimated {
                    estimate_tokens_from_counts(state.output_ascii_chars, state.output_non_ascii_chars)
                } else {
                    state.output_tokens
                };
                if reported > budget {
                    observability.log(
                        "warning",
                        "continuation",
                        &format!(
                            "Clamped reported output_tokens from {} to the client budget {} to keep the response within the client's invariant.",
                            reported, budget
                        ),
                    );
                    state.output_tokens = budget;
                    state.output_estimated = false;
                }
            }
        }
        if !stream_failed && completed && v2_require_measured_output && state.output_estimated {
            stream_failed = true;
            observability.log(
                "error",
                "v2 output budget",
                "Provider omitted measured output usage in strict streamed mode.",
            );
            yield Ok(Bytes::from(encode_sse(
                "error",
                json!({ "type": "error", "error": { "type": "api_error", "message": "The provider omitted measured output usage; this response cannot be verified against the Desktop output limit." } }),
            )));
        }
        if !stream_failed && completed {
            if let Some(machine) = v2_machine.take() {
                let measured = (!state.output_estimated).then_some(state.output_tokens);
                let fallback = if state.output_estimated {
                    estimate_tokens_from_counts(state.output_ascii_chars, state.output_non_ascii_chars)
                } else {
                    state.output_tokens
                };
                match machine.finish(measured, fallback, client_output_budget) {
                    Ok(events) => {
                        for (event_name, event_data) in events {
                            yield Ok(Bytes::from(encode_sse(&event_name, event_data)));
                        }
                    }
                    Err(error) => {
                        stream_failed = true;
                        observability.log("error", "v2 stream", &format!("Provider SSE completion failed validation: {error:?}"));
                        yield Ok(Bytes::from(encode_sse("error", json!({ "type": "error", "error": { "type": "api_error", "message": "The provider stream ended with incomplete tool calls or exceeded the client's output limit." } }))));
                    }
                }
            } else {
                for (event_name, event_data) in state.finish() {
                    yield Ok(Bytes::from(encode_sse(&event_name, event_data)));
                }
            }
        }
        if !stream_failed && completed {
            let output_tokens = if state.output_estimated {
                estimate_tokens_from_counts(state.output_ascii_chars, state.output_non_ascii_chars)
            } else {
                state.output_tokens
            };
            let cache_stats_reported = state.cache_stats_reported && !state.input_estimated;
            let cached_input_tokens = if cache_stats_reported {
                state.cached_input_tokens.min(state.input_tokens)
            } else {
                0
            };
            let cache_write_input_tokens = if cache_stats_reported {
                state
                    .cache_write_input_tokens
                    .min(state.input_tokens.saturating_sub(cached_input_tokens))
            } else {
                0
            };
            if let Err(error) = observability.record_usage(
                &provider,
                &upstream_model,
                &model,
                state.input_tokens,
                output_tokens,
                cached_input_tokens,
                cache_write_input_tokens,
                if cache_stats_reported {
                    state.input_tokens
                } else {
                    0
                },
                cache_stats_reported,
                state.cached_input_reported && !state.input_estimated,
                state.cache_write_reported && !state.input_estimated,
                state.input_estimated,
                state.output_estimated,
            ) {
                observability.log("warning", "usage", &format!("Could not save token usage history: {error}"));
            }
        }
    };
    let mut response = Response::new(Body::from_stream(stream));
    *response.status_mut() = StatusCode::OK;
    response.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("text/event-stream; charset=utf-8"),
    );
    response.headers_mut().insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("no-cache, no-transform"),
    );
    response
        .headers_mut()
        .insert(header::CONNECTION, HeaderValue::from_static("keep-alive"));
    if trimmed_turns > 0 {
        response.headers_mut().insert(
            "x-qbs-context-trimmed-turns",
            HeaderValue::from_str(&trimmed_turns.to_string())
                .unwrap_or(HeaderValue::from_static("1")),
        );
    }
    cors_response(response)
}

#[derive(Default)]
struct AnthropicStreamState {
    next_index: usize,
    text_index: Option<usize>,
    tools: BTreeMap<i64, StreamTool>,
    stop_reason: Option<String>,
    input_tokens: u64,
    input_estimated: bool,
    cached_input_tokens: u64,
    cache_write_input_tokens: u64,
    cache_stats_reported: bool,
    cached_input_reported: bool,
    cache_write_reported: bool,
    output_tokens: u64,
    output_estimated: bool,
    output_ascii_chars: u64,
    output_non_ascii_chars: u64,
    text_content: String,
}

struct StreamTool {
    block_index: usize,
}

impl AnthropicStreamState {
    fn accept(&mut self, chunk: &Value) -> Vec<(String, Value)> {
        let mut events = Vec::new();
        let delta = chunk.pointer("/choices/0/delta").unwrap_or(&Value::Null);
        if let Some(text) = delta
            .get("content")
            .and_then(Value::as_str)
            .filter(|text| !text.is_empty())
        {
            self.count_output(text);
            self.text_content.push_str(text);
            let index = *self.text_index.get_or_insert_with(|| {
                let index = self.next_index;
                self.next_index += 1;
                events.push(("content_block_start".into(), json!({ "type": "content_block_start", "index": index, "content_block": { "type": "text", "text": "" } })));
                index
            });
            events.push(("content_block_delta".into(), json!({ "type": "content_block_delta", "index": index, "delta": { "type": "text_delta", "text": text } })));
        }
        if let Some(calls) = delta.get("tool_calls").and_then(Value::as_array) {
            for call in calls {
                let upstream_index = call.get("index").and_then(Value::as_i64).unwrap_or(0);
                if !self.tools.contains_key(&upstream_index) {
                    let index = self.next_index;
                    self.next_index += 1;
                    let id = call
                        .get("id")
                        .and_then(Value::as_str)
                        .unwrap_or("tool_call")
                        .to_string();
                    let name = call
                        .pointer("/function/name")
                        .and_then(Value::as_str)
                        .unwrap_or("tool")
                        .to_string();
                    events.push(("content_block_start".into(), json!({ "type": "content_block_start", "index": index, "content_block": { "type": "tool_use", "id": id, "name": name, "input": {} } })));
                    self.tools
                        .insert(upstream_index, StreamTool { block_index: index });
                }
                if let Some(arguments) = call
                    .pointer("/function/arguments")
                    .and_then(Value::as_str)
                    .filter(|value| !value.is_empty())
                {
                    self.count_output(arguments);
                    if let Some(tool) = self.tools.get(&upstream_index) {
                        events.push(("content_block_delta".into(), json!({ "type": "content_block_delta", "index": tool.block_index, "delta": { "type": "input_json_delta", "partial_json": arguments } })));
                    }
                }
            }
        }
        if let Some(reason) = chunk
            .pointer("/choices/0/finish_reason")
            .and_then(Value::as_str)
        {
            self.stop_reason = Some(reason.to_string());
        }
        let usage = parse_provider_usage(chunk.get("usage").unwrap_or(&Value::Null));
        if let Some(tokens) = usage.input_tokens {
            self.input_tokens = tokens;
            self.input_estimated = false;
        }
        if let Some(tokens) = usage.output_tokens {
            self.output_tokens = tokens;
            self.output_estimated = false;
        }
        if usage.cache_stats_reported {
            self.cache_stats_reported = true;
            if let Some(tokens) = usage.cached_input_tokens {
                self.cached_input_tokens = tokens;
                self.cached_input_reported = true;
            }
            if let Some(tokens) = usage.cache_write_input_tokens {
                self.cache_write_input_tokens = tokens;
                self.cache_write_reported = true;
            }
        }
        events
    }

    fn finish(&self) -> Vec<(String, Value)> {
        let mut events = Vec::new();
        if let Some(index) = self.text_index {
            events.push((
                "content_block_stop".into(),
                json!({ "type": "content_block_stop", "index": index }),
            ));
        }
        let mut tools: Vec<&StreamTool> = self.tools.values().collect();
        tools.sort_by_key(|tool| tool.block_index);
        for tool in tools {
            events.push((
                "content_block_stop".into(),
                json!({ "type": "content_block_stop", "index": tool.block_index }),
            ));
        }
        let stop = if !self.tools.is_empty() {
            "tool_use"
        } else {
            match self.stop_reason.as_deref().unwrap_or("stop") {
                "length" => "max_tokens",
                _ => "end_turn",
            }
        };
        let output_tokens = if self.output_estimated {
            estimate_tokens_from_counts(self.output_ascii_chars, self.output_non_ascii_chars)
        } else {
            self.output_tokens
        };
        events.push(("message_delta".into(), json!({ "type": "message_delta", "delta": { "stop_reason": stop, "stop_sequence": null }, "usage": { "output_tokens": output_tokens } })));
        events.push(("message_stop".into(), json!({ "type": "message_stop" })));
        events
    }

    fn count_output(&mut self, text: &str) {
        for character in text.chars() {
            if character.is_ascii() {
                self.output_ascii_chars = self.output_ascii_chars.saturating_add(1);
            } else {
                self.output_non_ascii_chars = self.output_non_ascii_chars.saturating_add(1);
            }
        }
    }
}

fn take_sse_event(buffer: &mut Vec<u8>) -> Option<String> {
    let mut separator = None;
    for index in 0..buffer.len() {
        if buffer.get(index..index + 4) == Some(b"\r\n\r\n") {
            separator = Some((index, 4));
            break;
        }
        if buffer.get(index..index + 2) == Some(b"\n\n") {
            separator = Some((index, 2));
            break;
        }
    }
    let (index, separator_len) = separator?;
    let event = buffer.drain(..index).collect::<Vec<_>>();
    buffer.drain(..separator_len);
    Some(String::from_utf8_lossy(&event).replace("\r", ""))
}

fn encode_sse(event_name: &str, data: Value) -> String {
    format!("event: {event_name}\ndata: {data}\n\n")
}

/// Maximum number of automatic continuation rounds per response. A 32K output
/// budget is normally enough for a single round; two extra rounds cover very
/// long generation work (large files, long analyses) without letting a
/// runaway loop burn provider credits.
const MAX_AUTO_CONTINUATIONS: usize = 3;

const CONTINUE_PROMPT: &str = "\n\nContinue exactly from the character where the previous response was cut off. Do not repeat, summarize, or restate anything that was already written, and do not add headings or comments about continuing. Resume seamlessly so the combined text reads as one uninterrupted response.";

/// Tokens of client output budget that are still undelivered after
/// `cumulative` tokens have been sent. `None` means "no budget to enforce".
fn budget_remaining(budget: Option<u64>, cumulative: u64) -> Option<u64> {
    budget.and_then(|budget| budget.checked_sub(cumulative))
}

/// Lower the continuation round's `max_tokens` so the provider cannot emit
/// more tokens than the client's remaining output budget. Without this cap a
/// single continuation round could push the merged message past the client's
/// `max_tokens` invariant and make the whole run fail.
fn cap_continuation_max_tokens(
    mut payload: Value,
    client_output_budget: Option<u64>,
    remaining: Option<u64>,
) -> Value {
    if let (Some(budget), Some(remaining)) = (client_output_budget, remaining) {
        if budget > 0 && remaining < budget {
            let cap = remaining.max(1).min(budget);
            let current = payload
                .get("max_tokens")
                .and_then(Value::as_u64)
                .unwrap_or(u64::MAX);
            if current > cap {
                payload["max_tokens"] = Value::from(cap);
            }
        }
    }
    payload
}

fn continuation_output_ceiling(
    payload: &Value,
    route_output_limit: u64,
    remaining_client_budget: Option<u64>,
) -> Option<u64> {
    if let Some(remaining) = remaining_client_budget {
        Some(if route_output_limit > 0 {
            remaining.min(route_output_limit)
        } else {
            remaining
        })
    } else if route_output_limit > 0 {
        Some(route_output_limit)
    } else {
        payload.get("max_tokens").and_then(Value::as_u64)
    }
}

fn refit_continuation_max_tokens(
    payload: &mut Value,
    context_window: u64,
    output_ceiling: Option<u64>,
) {
    let Some(output_ceiling) = output_ceiling else {
        return;
    };
    let available = if context_window > 0 {
        context_window
            .saturating_sub(estimate_tokens(payload))
            .saturating_sub(512)
            .max(1)
    } else {
        output_ceiling
    };
    payload["max_tokens"] = json!(output_ceiling.min(available).max(1));
}

/// True when the partial assistant message from a truncated round is safe to
/// replay to the provider as conversation context. Tool-call turns are not
/// replayed: their `tool_calls` JSON would be cut mid-stream and the provider
/// would either reject the history or complete garbage.
fn continuation_safe(completed_text: &str) -> bool {
    !completed_text.is_empty() && !has_open_json_string(completed_text)
}

/// A JSON object may be legitimately open at a truncation point (the model
/// was in the middle of an argument). A cut that lands *inside* a JSON string
/// literal, however, would break JSON parsing of the replayed history, so it
/// is treated as unsafe.
fn has_open_json_string(text: &str) -> bool {
    let mut in_string = false;
    let mut escaped = false;
    for character in text.chars() {
        if in_string {
            if escaped {
                escaped = false;
            } else if character == '\\' {
                escaped = true;
            } else if character == '"' {
                in_string = false;
            }
        } else if character == '"' {
            in_string = true;
        }
    }
    in_string
}

/// Build the OpenAI payload for one continuation round: the original request
/// plus the truncated partial assistant message and a resume instruction.
/// Continuation rounds are always fetched as complete JSON (not streamed), so
/// the streaming flags are forced off regardless of the original request.
fn build_continuation_payload(base: &Value, partial_text: &str) -> Value {
    let mut continuation = base.clone();
    if let Some(messages) = continuation
        .get_mut("messages")
        .and_then(Value::as_array_mut)
    {
        messages.push(json!({
            "role": "assistant",
            "content": partial_text
        }));
        messages.push(json!({
            "role": "user",
            "content": CONTINUE_PROMPT
        }));
    }
    continuation["stream"] = json!(false);
    if let Some(object) = continuation.as_object_mut() {
        object.remove("stream_options");
    }
    continuation
}

/// Parse provider usage metadata out of an OpenAI-style response object.
fn provider_usage(value: &Value) -> (u64, u64) {
    let usage = value.get("usage").unwrap_or(&Value::Null);
    let input = usage
        .get("prompt_tokens")
        .and_then(Value::as_u64)
        .or_else(|| usage.get("input_tokens").and_then(Value::as_u64))
        .unwrap_or(0);
    let output = usage
        .get("completion_tokens")
        .and_then(Value::as_u64)
        .or_else(|| usage.get("output_tokens").and_then(Value::as_u64))
        .unwrap_or(0);
    (input, output)
}

/// Fetch the full next round of a truncated response from the provider.
/// Returns `(text, finish_reason, input_tokens, output_tokens)`, or `None`
/// when the round could not be completed or did not produce plain text worth
/// continuing.
async fn fetch_continuation(
    client: &Client,
    direct: &Client,
    config: &AppConfig,
    target: &str,
    mut payload: Value,
    context_window: u64,
    auto_trim_on_overflow: bool,
    output_ceiling: Option<u64>,
    observability: &ObservabilityStore,
) -> Option<(String, String, u64, u64)> {
    let mut trimmed = 0usize;
    let response = loop {
        refit_continuation_max_tokens(&mut payload, context_window, output_ceiling);
        let mut request = client
            .post(target)
            .header(header::CONTENT_TYPE, "application/json")
            .header(header::ACCEPT, "application/json");
        request = match add_upstream_auth(request, config) {
            Ok(request) => request,
            Err(error) => {
                observability.log(
                    "warning",
                    "continuation",
                    &format!("Continuation could not be sent: {error}"),
                );
                return None;
            }
        };
        let response = match send_provider_with_fallback(
            request.json(&payload),
            direct,
            observability,
            config,
        )
        .await
        {
            Ok(response) => response,
            Err(error) => {
                observability.log(
                    "warning",
                    "continuation",
                    &format!("The provider rejected the continuation request: {error}"),
                );
                return None;
            }
        };
        if response.status().is_success() {
            break response;
        }
        let status = response.status();
        let message = response
            .text()
            .await
            .unwrap_or_else(|_| "The error response could not be read.".into());
        // A long response re-sends its own (growing) partial text as context.
        // In a long session that replay can itself overflow the provider's
        // context window. Recover the same way the main request does: drop the
        // oldest complete turns and retry, rather than abandoning the run.
        if auto_trim_on_overflow && is_context_overflow(&message) && trim_oldest_turn(&mut payload)
        {
            trimmed += 1;
            observability.log(
                "warning",
                "continuation",
                &format!(
                    "Continuation hit the provider context limit (HTTP {status}); dropped an older turn (turn {trimmed}) and retried."
                ),
            );
            continue;
        }
        observability.log(
            "warning",
            "continuation",
            &format!(
                "The provider returned HTTP {status} for the continuation request; the partial response is kept as is.",
            ),
        );
        return None;
    };
    let value: Value = match response.json().await {
        Ok(value) => value,
        Err(_) => {
            observability.log(
                "warning",
                "continuation",
                "The continuation response was not valid JSON.",
            );
            return None;
        }
    };
    let message = value
        .pointer("/choices/0/message")
        .cloned()
        .unwrap_or_else(|| json!({}));
    let finish = value
        .pointer("/choices/0/finish_reason")
        .and_then(Value::as_str)
        .unwrap_or("stop");
    // A continuation that itself starts a tool call would produce a broken
    // tool_use block after the merged text; stop continuing instead.
    if message.get("tool_calls").is_some_and(Value::is_array) && finish != "length" {
        observability.log(
            "warning",
            "continuation",
            "The continuation started a tool call; keeping the partial response as is.",
        );
        return None;
    }
    let text = match message.get("content") {
        Some(Value::String(text)) if !text.is_empty() => text.clone(),
        Some(Value::Array(parts)) => parts
            .iter()
            .filter_map(|part| part.get("text").and_then(Value::as_str))
            .collect::<Vec<_>>()
            .join(""),
        _ => {
            observability.log(
                "warning",
                "continuation",
                "The continuation produced no text; keeping the partial response as is.",
            );
            return None;
        }
    };
    let (input_tokens, output_tokens) = provider_usage(&value);
    Some((text, finish.to_string(), input_tokens, output_tokens))
}

#[tauri::command]
async fn apply_claude_desktop(
    app: AppHandle,
    state: TauriState<'_, BridgeState>,
) -> Result<String, String> {
    start_proxy_inner(state.inner()).await?;
    update_status_icon(&app, true);
    let mut config = state.config.read().await.clone();
    let token = ensure_gateway_token(&mut config);
    persist_config(&state.config_path, &config)?;
    *state.config.write().await = config.clone();
    write_desktop_profile(&config, &token)?;
    set_claude_output_limit_environment(config.claude_code_max_output_tokens)?;
    Ok("Claude Desktop settings were updated. Fully quit and reopen the app.".into())
}

#[tauri::command]
async fn apply_claude_cli(
    app: AppHandle,
    state: TauriState<'_, BridgeState>,
) -> Result<String, String> {
    start_proxy_inner(state.inner()).await?;
    update_status_icon(&app, true);
    let mut config = state.config.read().await.clone();
    let token = ensure_gateway_token(&mut config);
    persist_config(&state.config_path, &config)?;
    *state.config.write().await = config.clone();
    write_cli_settings(&config, &token)?;
    Ok("Claude Code settings were updated. Close and reopen any running CLI sessions.".into())
}

#[tauri::command]
async fn quit_application(
    app: AppHandle,
    state: TauriState<'_, BridgeState>,
) -> Result<(), String> {
    stop_proxy_inner(state.inner()).await;
    app.exit(0);
    Ok(())
}

fn require_ready(config: &AppConfig) -> Result<(), String> {
    if config.base_url.trim().is_empty() {
        return Err("Enter a provider URL.".into());
    }
    if config.api_key.trim().is_empty() {
        return Err("Enter an API key.".into());
    }
    if normalize_routes(config.routes.clone()).is_empty() {
        return Err("Map at least one Claude role to a provider model.".into());
    }
    if !Url::parse(&config.base_url).is_ok_and(|url| matches!(url.scheme(), "http" | "https")) {
        return Err("Enter a valid provider URL starting with http or https.".into());
    }
    Ok(())
}

fn write_desktop_profile(config: &AppConfig, token: &str) -> Result<(), String> {
    require_ready(config)?;
    let local_app_data = std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .or_else(|| {
            std::env::var_os("USERPROFILE")
                .map(|path| PathBuf::from(path).join("AppData").join("Local"))
        })
        .ok_or_else(|| "The LocalAppData folder could not be found.".to_string())?;
    let normal_dir =
        pick_claude_dir(&local_app_data, false).unwrap_or_else(|| local_app_data.join("Claude"));
    let threep_dir =
        pick_claude_dir(&local_app_data, true).unwrap_or_else(|| local_app_data.join("Claude-3p"));
    let config_library = threep_dir.join("configLibrary");
    let profile_path = config_library.join(format!("{PROFILE_ID}.json"));
    let meta_path = config_library.join("_meta.json");
    let base_url = format!(
        "http://{}:{}{}",
        LOCAL_HOST, config.proxy_port, CLAUDE_DESKTOP_PREFIX
    );
    let routes = normalize_routes(config.routes.clone());
    let labels = route_labels(&routes);
    let inference_models: Vec<Value> = routes
        .iter()
        .zip(labels)
        .map(|(route, label)| {
            json!({
                "name": route.route_id,
                "labelOverride": label
            })
        })
        .collect();
    let profile = json!({
        "inferenceProvider": "gateway",
        "inferenceGatewayBaseUrl": base_url,
        "inferenceGatewayAuthScheme": "bearer",
        "inferenceGatewayApiKey": token,
        "inferenceModels": inference_models
    });

    let normal_config = normal_dir.join("claude_desktop_config.json");
    let threep_config = threep_dir.join("claude_desktop_config.json");
    let mut normal = read_json_object(&normal_config)?;
    let mut third_party = read_json_object(&threep_config)?;
    normal["deploymentMode"] = json!("3p");
    third_party["deploymentMode"] = json!("3p");

    let mut meta = read_json_object(&meta_path)?;
    let entries = meta
        .get("entries")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let mut entries: Vec<Value> = entries
        .into_iter()
        .filter(|entry| entry.get("id").and_then(Value::as_str) != Some(PROFILE_ID))
        .collect();
    entries.push(json!({ "id": PROFILE_ID, "name": PROFILE_NAME }));
    meta["entries"] = Value::Array(entries);
    meta["appliedId"] = json!(PROFILE_ID);

    let writes = vec![
        (
            normal_config,
            serde_json::to_vec_pretty(&normal).map_err(|error| error.to_string())?,
        ),
        (
            threep_config,
            serde_json::to_vec_pretty(&third_party).map_err(|error| error.to_string())?,
        ),
        (
            profile_path,
            serde_json::to_vec_pretty(&profile).map_err(|error| error.to_string())?,
        ),
        (
            meta_path,
            serde_json::to_vec_pretty(&meta).map_err(|error| error.to_string())?,
        ),
    ];
    transactional_writes(writes)
}

fn pick_claude_dir(local_app_data: &Path, third_party: bool) -> Option<PathBuf> {
    let exact = local_app_data.join(if third_party { "Claude-3p" } else { "Claude" });
    if exact.is_dir() {
        return Some(exact);
    }
    let mut candidates: Vec<PathBuf> = fs::read_dir(local_app_data)
        .ok()?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.is_dir())
        .filter(|path| {
            let name = path
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or_default();
            name.starts_with("Claude") && name.contains("-3p") == third_party
        })
        .collect();
    candidates.sort();
    candidates.into_iter().next()
}

fn read_json_object(path: &Path) -> Result<Value, String> {
    if !path.exists() {
        return Ok(json!({}));
    }
    let body = fs::read_to_string(path)
        .map_err(|error| format!("Could not read {}: {error}", path.display()))?;
    let value: Value = serde_json::from_str(&body).map_err(|error| {
        format!(
            "File {} does not contain valid JSON: {error}",
            path.display()
        )
    })?;
    if value.is_object() {
        Ok(value)
    } else {
        Err(format!(
            "File {} must contain a JSON object.",
            path.display()
        ))
    }
}

fn transactional_writes(writes: Vec<(PathBuf, Vec<u8>)>) -> Result<(), String> {
    let snapshots: Vec<(PathBuf, Option<Vec<u8>>)> = writes
        .iter()
        .map(|(path, _)| {
            let content = if path.exists() {
                Some(fs::read(path).map_err(|error| {
                    format!(
                        "Could not read the previous version of {}: {error}",
                        path.display()
                    )
                })?)
            } else {
                None
            };
            Ok((path.clone(), content))
        })
        .collect::<Result<_, String>>()?;
    for (path, _) in &writes {
        if path.exists() {
            make_backup(path)?;
        }
    }
    for (path, body) in writes {
        let result = if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .map_err(|error| format!("Could not create folder {}: {error}", parent.display()))
                .and_then(|()| write_atomic(&path, &body))
        } else {
            write_atomic(&path, &body)
        };
        if let Err(error) = result {
            for (restore_path, content) in &snapshots {
                if let Some(content) = content {
                    let _ = write_atomic(restore_path, content);
                } else {
                    let _ = fs::remove_file(restore_path);
                }
            }
            return Err(error);
        }
    }
    Ok(())
}

fn make_backup(path: &Path) -> Result<PathBuf, String> {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    let mut backup = path.as_os_str().to_os_string();
    backup.push(format!(".qbs2-backup-{stamp}"));
    let backup = PathBuf::from(backup);
    fs::copy(path, &backup)
        .map_err(|error| format!("Could not create a backup of {}: {error}", path.display()))?;
    Ok(backup)
}

fn write_cli_settings(config: &AppConfig, token: &str) -> Result<(), String> {
    require_ready(config)?;
    let root = std::env::var_os("CLAUDE_CONFIG_DIR")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("USERPROFILE").map(|path| PathBuf::from(path).join(".claude")))
        .ok_or_else(|| "The Claude Code settings path could not be found.".to_string())?;
    let path = root.join("settings.json");
    let mut settings = read_json_object(&path)?;
    if !settings.get("env").is_some_and(Value::is_object) {
        settings["env"] = json!({});
    }
    let env = settings["env"].as_object_mut().expect("env normalized");
    env.insert(
        "ANTHROPIC_BASE_URL".into(),
        json!(format!("http://{}:{}", LOCAL_HOST, config.proxy_port)),
    );
    env.insert("ANTHROPIC_AUTH_TOKEN".into(), json!(token));
    if let Some(route) = default_route(config) {
        env.insert("ANTHROPIC_MODEL".into(), json!(route.route_id));
    }
    // These values are defaults for newly created CLI sessions only. A request
    // that contains its own `model` is resolved independently below, so changing
    // one chat's model never mutates this global default or another chat.
    for key in [
        "ANTHROPIC_MODEL",
        "ANTHROPIC_DEFAULT_SONNET_MODEL",
        "ANTHROPIC_DEFAULT_OPUS_MODEL",
        "ANTHROPIC_DEFAULT_HAIKU_MODEL",
        "ANTHROPIC_DEFAULT_FABLE_MODEL",
        "ANTHROPIC_DEFAULT_MYTHOS_MODEL",
    ] {
        env.remove(key);
    }
    for tier in ["sonnet", "opus", "haiku", "fable"] {
        if let Some(route) = config
            .routes
            .iter()
            .find(|route| route.tier.eq_ignore_ascii_case(tier))
        {
            env.insert(
                format!("ANTHROPIC_DEFAULT_{}_MODEL", tier.to_ascii_uppercase()),
                json!(route.route_id),
            );
        }
    }
    env.insert(
        "CLAUDE_CODE_MAX_OUTPUT_TOKENS".into(),
        json!(config.claude_code_max_output_tokens.to_string()),
    );
    transactional_writes(vec![(
        path,
        serde_json::to_vec_pretty(&settings).map_err(|error| error.to_string())?,
    )])
}

fn default_route(config: &AppConfig) -> Option<&ModelRoute> {
    config
        .routes
        .iter()
        .find(|route| route.is_default)
        .or_else(|| config.routes.first())
}

#[cfg(windows)]
fn set_claude_output_limit_environment(value: u64) -> Result<(), String> {
    let value = normalize_claude_code_max_output_tokens(value).to_string();
    let output = Command::new("reg.exe")
        .args([
            "ADD",
            r"HKCU\Environment",
            "/v",
            "CLAUDE_CODE_MAX_OUTPUT_TOKENS",
            "/t",
            "REG_SZ",
            "/d",
            &value,
            "/f",
        ])
        .output()
        .map_err(|error| format!("Could not update the Windows user environment: {error}"))?;
    if !output.status.success() {
        let detail = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Err(if detail.is_empty() {
            "Windows rejected the Claude output limit setting.".into()
        } else {
            format!("Windows rejected the Claude output limit setting: {detail}")
        });
    }

    // Notify the shell so newly launched apps can refresh their inherited
    // environment. Already-running Claude Desktop processes still need a restart.
    let environment = "Environment"
        .encode_utf16()
        .chain(Some(0))
        .collect::<Vec<_>>();
    let mut result = 0usize;
    #[link(name = "user32")]
    unsafe extern "system" {
        fn SendMessageTimeoutW(
            hwnd: *mut c_void,
            msg: u32,
            wparam: usize,
            lparam: isize,
            flags: u32,
            timeout: u32,
            result: *mut usize,
        ) -> *mut c_void;
    }
    const HWND_BROADCAST: *mut c_void = -1isize as *mut c_void;
    const WM_SETTINGCHANGE: u32 = 0x001A;
    const SMTO_ABORTIFHUNG: u32 = 0x0002;
    unsafe {
        let _ = SendMessageTimeoutW(
            HWND_BROADCAST,
            WM_SETTINGCHANGE,
            0,
            environment.as_ptr() as isize,
            SMTO_ABORTIFHUNG,
            5_000,
            &mut result,
        );
    }
    Ok(())
}

#[cfg(not(windows))]
fn set_claude_output_limit_environment(_value: u64) -> Result<(), String> {
    Err("Claude Desktop output limit can only be applied on Windows.".into())
}

#[tauri::command]
async fn open_creator_link() -> Result<(), String> {
    open_url("https://eitaa.com/salmanzadeh_ir")
}

#[cfg(windows)]
fn open_url(url: &str) -> Result<(), String> {
    let wide: Vec<u16> = url.encode_utf16().chain(Some(0)).collect();
    #[link(name = "shell32")]
    unsafe extern "system" {
        fn ShellExecuteW(
            hwnd: *mut c_void,
            operation: *const u16,
            file: *const u16,
            parameters: *const u16,
            directory: *const u16,
            show: i32,
        ) -> *mut c_void;
    }
    let result = unsafe {
        ShellExecuteW(
            std::ptr::null_mut(),
            std::ptr::null(),
            wide.as_ptr(),
            std::ptr::null(),
            std::ptr::null(),
            1,
        )
    } as isize;
    if result > 32 {
        Ok(())
    } else {
        Err("Could not open the creator page.".into())
    }
}

#[cfg(not(windows))]
fn open_url(_url: &str) -> Result<(), String> {
    Err("The creator link is available only in the Windows app.".into())
}

#[repr(C)]
struct DataBlob {
    size: u32,
    data: *mut u8,
}

#[cfg(windows)]
fn protect_secret(value: &str) -> Result<String, String> {
    if value.is_empty() {
        return Ok(String::new());
    }
    #[link(name = "crypt32")]
    unsafe extern "system" {
        fn CryptProtectData(
            input: *const DataBlob,
            description: *const u16,
            entropy: *const DataBlob,
            reserved: *mut c_void,
            prompt: *mut c_void,
            flags: u32,
            output: *mut DataBlob,
        ) -> i32;
    }
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn LocalFree(memory: *mut c_void) -> *mut c_void;
    }
    let bytes = value.as_bytes();
    let input = DataBlob {
        size: bytes.len() as u32,
        data: bytes.as_ptr() as *mut u8,
    };
    let mut output = DataBlob {
        size: 0,
        data: std::ptr::null_mut(),
    };
    let ok = unsafe {
        CryptProtectData(
            &input,
            std::ptr::null(),
            std::ptr::null(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            1,
            &mut output,
        )
    };
    if ok == 0 {
        return Err("Windows could not encrypt the API key.".into());
    }
    let cipher = unsafe { std::slice::from_raw_parts(output.data, output.size as usize) }.to_vec();
    unsafe {
        LocalFree(output.data as *mut c_void);
    }
    Ok(format!("dpapi:{}", hex_encode(&cipher)))
}

#[cfg(windows)]
fn unprotect_secret(value: &str) -> Result<String, String> {
    if value.is_empty() {
        return Ok(String::new());
    }
    let encoded = value
        .strip_prefix("dpapi:")
        .ok_or_else(|| "The saved key format is not recognized.".to_string())?;
    let cipher = hex_decode(encoded)?;
    #[link(name = "crypt32")]
    unsafe extern "system" {
        fn CryptUnprotectData(
            input: *const DataBlob,
            description: *mut *mut u16,
            entropy: *const DataBlob,
            reserved: *mut c_void,
            prompt: *mut c_void,
            flags: u32,
            output: *mut DataBlob,
        ) -> i32;
    }
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn LocalFree(memory: *mut c_void) -> *mut c_void;
    }
    let input = DataBlob {
        size: cipher.len() as u32,
        data: cipher.as_ptr() as *mut u8,
    };
    let mut output = DataBlob {
        size: 0,
        data: std::ptr::null_mut(),
    };
    let ok = unsafe {
        CryptUnprotectData(
            &input,
            std::ptr::null_mut(),
            std::ptr::null(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            1,
            &mut output,
        )
    };
    if ok == 0 {
        return Err("Windows could not decrypt the saved key.".into());
    }
    let plain = unsafe { std::slice::from_raw_parts(output.data, output.size as usize) }.to_vec();
    unsafe {
        LocalFree(output.data as *mut c_void);
    }
    String::from_utf8(plain).map_err(|_| "The decrypted key is not valid text.".into())
}

#[cfg(not(windows))]
fn protect_secret(value: &str) -> Result<String, String> {
    Ok(format!("plain:{}", hex_encode(value.as_bytes())))
}

#[cfg(not(windows))]
fn unprotect_secret(value: &str) -> Result<String, String> {
    let encoded = value.strip_prefix("plain:").unwrap_or(value);
    String::from_utf8(hex_decode(encoded)?).map_err(|error| error.to_string())
}

fn hex_encode(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn hex_decode(value: &str) -> Result<Vec<u8>, String> {
    if value.len() % 2 != 0 {
        return Err("The encrypted data is incomplete.".into());
    }
    value
        .as_bytes()
        .chunks_exact(2)
        .map(|pair| {
            let text = std::str::from_utf8(pair).map_err(|error| error.to_string())?;
            u8::from_str_radix(text, 16).map_err(|error| error.to_string())
        })
        .collect()
}

fn show_main_window(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.set_focus();
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ProviderProxyMode {
    Auto,
    System,
    Direct,
}

impl ProviderProxyMode {
    fn from_setting(raw: &str) -> Self {
        match raw.trim().to_ascii_lowercase().as_str() {
            "system" => Self::System,
            "auto" => Self::Auto,
            _ => Self::Direct,
        }
    }
}

// One explicit, persisted policy for model listing, inference and continuation.
// Web Search has an independent client. No silent proxy fallback.
async fn send_provider_with_fallback(
    request: reqwest::RequestBuilder,
    direct: &Client,
    observability: &ObservabilityStore,
    config: &AppConfig,
) -> Result<reqwest::Response, reqwest::Error> {
    send_provider_with_mode(
        request,
        direct,
        observability,
        ProviderProxyMode::from_setting(&config.provider_network_mode),
    )
    .await
}

async fn send_provider_with_mode(
    request: reqwest::RequestBuilder,
    direct: &Client,
    observability: &ObservabilityStore,
    mode: ProviderProxyMode,
) -> Result<reqwest::Response, reqwest::Error> {
    if mode == ProviderProxyMode::Direct {
        return direct.execute(request.build()?).await;
    }
    let fallback = if mode == ProviderProxyMode::Auto {
        request.try_clone()
    } else {
        None
    };
    match request.send().await {
        Ok(response) => Ok(response),
        Err(error) if error.is_connect() && fallback.is_some() => {
            observability.log(
                "warning",
                "provider network",
                "System proxy connection failed. Trying a direct provider connection once. This bypasses the configured system proxy; set CLAUDE_PROXY_PROVIDER_PROXY_MODE=system to disable direct fallback.",
            );
            direct
                .execute(fallback.expect("checked above").build()?)
                .await
        }
        Err(error) => Err(error),
    }
}

fn build_direct_provider_client() -> Result<Client, String> {
    Client::builder()
        .no_proxy()
        .connect_timeout(Duration::from_secs(20))
        .read_timeout(Duration::from_secs(180))
        .pool_idle_timeout(Duration::from_secs(90))
        .pool_max_idle_per_host(16)
        .tcp_nodelay(true)
        .build()
        .map_err(|error| error.to_string())
}

fn build_http_client() -> Result<Client, String> {
    Client::builder()
        .connect_timeout(Duration::from_secs(20))
        .read_timeout(Duration::from_secs(180))
        .pool_idle_timeout(Duration::from_secs(90))
        .pool_max_idle_per_host(16)
        .tcp_nodelay(true)
        .build()
        .map_err(|error| error.to_string())
}

pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            let app_data = app
                .path()
                .app_data_dir()
                .map_err(|error| error.to_string())?;
            let config_path = app_data.join("bridge-config.json");
            let observability = ObservabilityStore::new(
                app_data.join("usage-history.jsonl"),
                app_data.join("diagnostics.jsonl"),
            );
            let config = load_config(&config_path);
            let auto_start = configured(&config);
            let upstream_clients: Vec<Client> = (0..UPSTREAM_CONCURRENCY)
                .map(|_| build_http_client())
                .collect::<Result<_, _>>()?;
            let client = upstream_clients
                .first()
                .cloned()
                .ok_or_else(|| "The upstream client pool could not be initialized.".to_string())?;
            let direct_provider_client = build_direct_provider_client()?;
            let state = BridgeState {
                app: app.handle().clone(),
                config: Arc::new(RwLock::new(config)),
                runtime: Arc::new(Mutex::new(None)),
                client,
                direct_provider_client,
                upstream_clients: Arc::new(upstream_clients),
                upstream_cursor: Arc::new(AtomicUsize::new(0)),
                upstream_slots: Arc::new(Semaphore::new(UPSTREAM_CONCURRENCY)),
                web_slots: Arc::new(Semaphore::new(WEB_CONCURRENCY)),
                shell_slots: Arc::new(Semaphore::new(SHELL_CONCURRENCY)),
                config_path,
                observability,
                active_requests: Arc::new(AtomicUsize::new(0)),
            };
            app.manage(state.clone());

            let open_item = MenuItem::with_id(app, "open", "Open window", true, None::<&str>)?;
            let quit_item = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&open_item, &quit_item])?;
            let icon = status_icon(false);
            TrayIconBuilder::with_id(STATUS_TRAY_ID)
                .icon(icon)
                .tooltip("Claude Proxy")
                .menu(&menu)
                .show_menu_on_left_click(false)
                .on_menu_event(|app, event| match event.id.as_ref() {
                    "open" => show_main_window(app),
                    "quit" => app.exit(0),
                    _ => {}
                })
                .on_tray_icon_event(|tray, event| {
                    if let TrayIconEvent::Click {
                        button: MouseButton::Left,
                        button_state: MouseButtonState::Up,
                        ..
                    } = event
                    {
                        show_main_window(tray.app_handle());
                    }
                })
                .build(app)?;

            if auto_start {
                let app_handle = app.handle().clone();
                let observability = state.observability.clone();
                tauri::async_runtime::spawn(async move {
                    if start_proxy_inner(&state).await.is_ok() {
                        update_status_icon(&app_handle, true);
                        observability.log("info", "proxy", "Local proxy started automatically.");
                    } else {
                        observability.log("error", "proxy", "Automatic proxy startup failed; open the app and review Provider connection settings.");
                    }
                });
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_config,
            get_usage_report,
            get_diagnostics,
            clear_diagnostics,
            save_config,
            get_proxy_status,
            fetch_models,
            ping_provider,
            probe_provider_fidelity,
            start_proxy,
            stop_proxy,
            test_local_route,
            test_web_access,
            apply_claude_code_web_mcp,
            apply_claude_desktop,
            apply_claude_cli,
            open_creator_link,
            quit_application
        ])
        .run(tauri::generate_context!())
        .expect("Claude Proxy failed to start");
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fable_route() -> ModelRoute {
        ModelRoute {
            route_id: "claude-fable-5-1".into(),
            display_name: "Fable 5.1 · qwen38-sglang".into(),
            upstream_model: "qwen38-sglang".into(),
            tier: "fable".into(),
            thinking_mode: String::new(),
            context_window: 200_000,
            max_output_tokens: 8_000,
            is_default: true,
        }
    }

    #[test]
    fn new_install_has_no_provider_key_model_or_route_defaults() {
        let config = AppConfig::default();
        assert!(config.provider_name.is_empty());
        assert!(config.base_url.is_empty());
        assert!(config.api_key.is_empty());
        assert!(config.upstream_models.is_empty());
        assert!(config.routes.is_empty());
    }

    #[test]
    fn proxy_status_icons_load_as_square_pngs() {
        for active in [false, true] {
            let icon = status_icon(active);
            assert_eq!(icon.width(), 128);
            assert_eq!(icon.height(), 128);
            assert_eq!(icon.rgba().len(), 128 * 128 * 4);
        }
    }

    #[test]
    fn only_claude_role_aliases_are_published() {
        assert!(is_claude_safe_route("claude-fable-5-1"));
        assert!(is_claude_safe_route("claude-sonnet-5-5"));
        assert!(is_claude_safe_route("claude-opus-4-8"));
        assert!(!is_claude_safe_route("qwen38-sglang"));
        assert!(!is_claude_safe_route("claude-mythos-5"));
        assert!(!is_claude_safe_route("claude-fable-"));
    }

    #[test]
    fn routes_are_migrated_to_the_current_four_role_ids_without_dropping_mappings() {
        let routes = normalize_routes(vec![
            ModelRoute {
                route_id: "claude-fable-5".into(),
                display_name: "old fable".into(),
                upstream_model: "provider-fable".into(),
                tier: "fable".into(),
                thinking_mode: String::new(),
                context_window: 0,
                max_output_tokens: 0,
                is_default: true,
            },
            ModelRoute {
                route_id: "claude-opus-4-8".into(),
                display_name: "old opus".into(),
                upstream_model: "provider-opus".into(),
                tier: "opus".into(),
                thinking_mode: String::new(),
                context_window: 0,
                max_output_tokens: 0,
                is_default: false,
            },
            ModelRoute {
                route_id: "claude-mythos-5".into(),
                display_name: "removed".into(),
                upstream_model: "provider-mythos".into(),
                tier: "mythos".into(),
                thinking_mode: String::new(),
                context_window: 0,
                max_output_tokens: 0,
                is_default: false,
            },
        ]);
        assert_eq!(routes.len(), 2);
        assert_eq!(routes[0].route_id, "claude-fable-5-1");
        assert_eq!(routes[0].upstream_model, "provider-fable");
        assert_eq!(routes[1].route_id, "claude-opus-5-5");
        assert_eq!(routes[1].upstream_model, "provider-opus");
    }

    #[test]
    fn duplicate_labels_are_disambiguated_for_client_model_lists() {
        let routes = vec![
            fable_route(),
            ModelRoute {
                route_id: "claude-opus-5-5".into(),
                display_name: "Shared label".into(),
                upstream_model: "provider-opus".into(),
                tier: "opus".into(),
                thinking_mode: String::new(),
                context_window: 0,
                max_output_tokens: 0,
                is_default: false,
            },
            ModelRoute {
                route_id: "claude-sonnet-5-5".into(),
                display_name: "Shared label".into(),
                upstream_model: "provider-sonnet".into(),
                tier: "sonnet".into(),
                thinking_mode: String::new(),
                context_window: 0,
                max_output_tokens: 0,
                is_default: false,
            },
        ];
        let labels = route_labels(&routes);
        assert_eq!(labels[1], "Shared label");
        assert_eq!(labels[2], "Shared label · claude-sonnet-5-5");
    }

    #[test]
    fn each_request_model_resolves_to_its_own_route() {
        let routes = vec![
            fable_route(),
            ModelRoute {
                route_id: "claude-opus-5-5".into(),
                display_name: "Opus 5.5".into(),
                upstream_model: "provider-opus".into(),
                tier: "opus".into(),
                thinking_mode: String::new(),
                context_window: 0,
                max_output_tokens: 0,
                is_default: false,
            },
        ];
        assert_eq!(
            resolve_route(&routes, "claude-fable-5-1")
                .unwrap()
                .upstream_model,
            "qwen38-sglang"
        );
        assert_eq!(
            resolve_route(&routes, "claude-opus-5-5")
                .unwrap()
                .upstream_model,
            "provider-opus"
        );
    }

    #[test]
    fn system_prompt_is_first_and_model_name_is_remapped() {
        let request = json!({
            "model": "claude-fable-5-1",
            "max_tokens": 64_000,
            "system": [{ "type": "text", "text": "rules" }],
            "messages": [{ "role": "user", "content": "hi" }]
        });
        let converted = anthropic_to_openai(&request, &fable_route()).unwrap();
        assert_eq!(converted["model"], "qwen38-sglang");
        assert_eq!(converted["messages"][0]["role"], "system");
        assert_eq!(converted["messages"][0]["content"], "rules");
        assert!(converted["max_tokens"].as_u64().unwrap() <= 8_000);
    }

    #[test]
    fn reasoning_controls_are_scoped_to_each_request() {
        let high_request = json!({
            "model": "claude-fable-5-1",
            "max_tokens": 512,
            "output_config": { "effort": "high" },
            "thinking": { "type": "adaptive" },
            "messages": [{ "role": "user", "content": "first" }]
        });
        let low_request = json!({
            "model": "claude-fable-5-1",
            "max_tokens": 512,
            "output_config": { "effort": "low" },
            "thinking": { "type": "disabled" },
            "messages": [{ "role": "user", "content": "second" }]
        });

        // Generic providers should continue to honor the client's
        // per-request reasoning controls, unlike the Qwen38 override.
        let mut generic_route = fable_route();
        generic_route.upstream_model = "generic-provider".into();
        let high = anthropic_to_openai(&high_request, &generic_route).unwrap();
        let low = anthropic_to_openai(&low_request, &generic_route).unwrap();

        assert_eq!(high["reasoning_effort"], "high");
        assert_eq!(high["chat_template_kwargs"]["reasoning_effort"], "high");
        assert_eq!(high["chat_template_kwargs"]["enable_thinking"], true);
        assert_eq!(low["reasoning_effort"], "low");
        assert_eq!(low["chat_template_kwargs"]["reasoning_effort"], "low");
        assert_eq!(low["chat_template_kwargs"]["enable_thinking"], false);
        assert_ne!(high["reasoning_effort"], low["reasoning_effort"]);
    }

    #[test]
    fn existing_chat_template_options_are_preserved() {
        let request = json!({
            "model": "claude-fable-5-1",
            "max_tokens": 512,
            "effort": "medium",
            "thinking": { "type": "enabled" },
            "chat_template_kwargs": { "enable_thinking": false, "custom_flag": true },
            "messages": [{ "role": "user", "content": "hello" }]
        });

        let mut generic_route = fable_route();
        generic_route.upstream_model = "generic-provider".into();
        let converted = anthropic_to_openai(&request, &generic_route).unwrap();
        assert_eq!(converted["reasoning_effort"], "medium");
        assert_eq!(converted["chat_template_kwargs"]["custom_flag"], true);
        assert_eq!(converted["chat_template_kwargs"]["enable_thinking"], true);
    }

    #[test]
    fn cowork_tool_results_do_not_count_as_different_human_instructions() {
        let request = json!({
            "model":"claude-fable-5-1",
            "system":"Cowork session bootstrap",
            "tools":[{"name":"Bash","input_schema":{"type":"object"}}],
            "messages":[
                {"role":"user","content":"سلام"},
                {"role":"assistant","content":[{"type":"tool_use","id":"c1",
                    "name":"Bash","input":{"command":"pwd"}}]},
                {"role":"user","content":[{"type":"tool_result","tool_use_id":"c1",
                    "content":"mock result"}]}
            ]
        });
        let output = anthropic_to_openai(&request, &fable_route()).unwrap();
        assert!(user_text_order_preserved(&request, &output));
        assert_eq!(ordered_user_texts(&request), vec!["سلام"]);
        assert_eq!(ordered_user_texts(&output), vec!["سلام"]);
        assert_eq!(output["messages"][3]["role"], "tool");
    }

    #[test]
    fn cowork_user_instructions_must_not_be_changed_or_relabelled() {
        let request = json!({
            "model":"claude-fable-5-1",
            "messages":[
                {"role":"user","content":"Cowork bootstrap"},
                {"role":"user","content":[{"type":"text","text":"سلام"}]}
            ]
        });
        let mut output = anthropic_to_openai(&request, &fable_route()).unwrap();
        assert!(user_text_order_preserved(&request, &output));
        output["messages"][1]["content"] = json!("unrelated task");
        assert!(!user_text_order_preserved(&request, &output));
        let valid_developer = json!({
            "model":"claude-fable-5-1",
            "messages":[
                {"role":"developer","content":"Keep the agent session isolated."},
                {"role":"user","content":"سلام"}
            ]
        });
        let developer_output = anthropic_to_openai(&valid_developer, &fable_route()).unwrap();
        assert_eq!(developer_output["messages"][0]["role"], "developer");
        assert_eq!(
            developer_output["messages"][0]["content"],
            "Keep the agent session isolated."
        );
        assert!(user_text_order_preserved(
            &valid_developer,
            &developer_output
        ));

        let invalid = json!({
            "model":"claude-fable-5-1",
            "messages":[
                {"role":"user","content":"سلام"},
                {"role":"unexpected_role","content":"never change this to user"}
            ]
        });
        let error = anthropic_to_openai(&invalid, &fable_route()).unwrap_err();
        assert!(error.contains("COWORK_UNSUPPORTED_MESSAGE_ROLE"));
        assert!(error.contains("message_index=1"));
    }

    #[test]
    fn web_capability_probes_must_verify_real_results_not_http_200_alone() {
        let empty = evaluate_web_probe("search", r#"{"query":"x","results":[]}"#, 8);
        assert!(!empty.ok);
        assert!(empty.detail.contains("no parseable results"));
        let valid = evaluate_web_probe(
            "search",
            r#"{"query":"x","results":[{"title":"Example","url":"https://example.com"}]}"#,
            9,
        );
        assert!(valid.ok);
        let denied = evaluate_web_probe("github", r#"{"error":"The page returned HTTP 403."}"#, 11);
        assert!(!denied.ok);
        assert!(denied.detail.contains("denied"));
        let wrong_page = evaluate_web_probe(
            "github",
            r#"{"url":"https://github.com","content":"Sign in to continue"}"#,
            11,
        );
        assert!(!wrong_page.ok);
        let readable = evaluate_web_probe(
            "github",
            r#"{"content":"thedotmack/claude-mem README"}"#,
            12,
        );
        assert!(readable.ok);
        assert_eq!(
            web_probe_failure("DNS could not resolve"),
            "DNS resolution failed (may differ from proxy DNS)"
        );
    }

    #[test]
    fn mcp_registration_preserves_other_claude_code_settings_and_tools() {
        let mut value = json!({
            "theme":"dark",
            "mcpServers":{"other":{"type":"stdio","command":"example"}}
        });
        register_claude_code_web_mcp(&mut value, 18183, "safe-test-token").unwrap();
        assert_eq!(value["theme"], "dark");
        assert_eq!(value["mcpServers"]["other"]["command"], "example");
        assert_eq!(
            value["mcpServers"][MCP_SERVER_NAME]["url"],
            "http://127.0.0.1:18183/mcp"
        );
        assert_eq!(
            value["mcpServers"][MCP_SERVER_NAME]["headers"]["Authorization"],
            "Bearer safe-test-token"
        );
        register_claude_code_web_mcp(&mut value, 18183, "rotated").unwrap();
        assert_eq!(
            value["mcpServers"][MCP_SERVER_NAME]["headers"]["Authorization"],
            "Bearer rotated"
        );
        let mut conflict = json!({
            "mcpServers":{"claude-proxy-web":{"type":"stdio","command":"unrelated"}}
        });
        assert!(register_claude_code_web_mcp(&mut conflict, 18183, "x").is_err());
        assert_eq!(
            conflict["mcpServers"]["claude-proxy-web"]["command"],
            "unrelated"
        );
    }

    #[test]
    fn mcp_requires_local_origin_correct_host_auth_and_opt_in() {
        let mut cfg = AppConfig::default();
        cfg.proxy_port = 18183;
        cfg.gateway_token = "test-secret".into();
        cfg.web_access_mode = "auto".into();
        let mut headers = HeaderMap::new();
        headers.insert(header::HOST, HeaderValue::from_static("127.0.0.1:18183"));
        headers.insert(
            header::AUTHORIZATION,
            HeaderValue::from_static("Bearer test-secret"),
        );
        assert_eq!(mcp_access_allowed(&headers, &cfg), Ok(()));
        headers.insert(
            header::ORIGIN,
            HeaderValue::from_static("https://evil.example"),
        );
        assert_eq!(
            mcp_access_allowed(&headers, &cfg),
            Err(StatusCode::FORBIDDEN)
        );
        headers.remove(header::ORIGIN);
        headers.insert(header::HOST, HeaderValue::from_static("evil.example"));
        assert_eq!(
            mcp_access_allowed(&headers, &cfg),
            Err(StatusCode::FORBIDDEN)
        );
        headers.insert(header::HOST, HeaderValue::from_static("127.0.0.1:18183"));
        headers.remove(header::AUTHORIZATION);
        assert_eq!(
            mcp_access_allowed(&headers, &cfg),
            Err(StatusCode::UNAUTHORIZED)
        );
        headers.insert(
            header::AUTHORIZATION,
            HeaderValue::from_static("Bearer test-secret"),
        );
        cfg.web_access_mode = "disabled".into();
        assert_eq!(
            mcp_access_allowed(&headers, &cfg),
            Err(StatusCode::FORBIDDEN)
        );
        let tools = mcp_tool_list();
        assert_eq!(tools.len(), 2);
        assert!(tools.iter().all(|value| value.get("inputSchema").is_some()));
        assert!(tools
            .iter()
            .all(|value| { value.pointer("/annotations/readOnlyHint") == Some(&json!(true)) }));
    }

    #[test]
    fn cowork_native_message_roles_keep_control_and_tool_semantics() {
        let request = json!({
            "model": "claude-fable-5-1",
            "messages": [
                {"role":"system","content":[{"type":"text","text":"System startup"}]},
                {"role":"developer","content":"Workspace policy"},
                {"role":"user","content":"Read the workspace folder"},
                {"role":"assistant","content":[{"type":"tool_use","id":"call-7",
                    "name":"Bash","input":{"command":"pwd"}}]},
                {"role":"tool","tool_call_id":"call-7","content":[
                    {"type":"text","text":"C:/workspace"}]},
                {"role":"user","content":"سلام"}
            ]
        });
        let actual = anthropic_to_openai(&request, &fable_route()).unwrap();
        assert_eq!(actual["messages"][0]["role"], "system");
        assert_eq!(actual["messages"][0]["content"], "System startup");
        assert_eq!(actual["messages"][1]["role"], "developer");
        assert_eq!(actual["messages"][1]["content"], "Workspace policy");
        assert_eq!(actual["messages"][3]["tool_calls"][0]["id"], "call-7");
        assert_eq!(actual["messages"][4]["role"], "tool");
        assert_eq!(actual["messages"][4]["tool_call_id"], "call-7");
        assert_eq!(actual["messages"][4]["content"], "C:/workspace");
        assert!(user_text_order_preserved(&request, &actual));

        let no_call_id = json!({
            "model":"claude-fable-5-1",
            "messages":[{"role":"tool","content":"orphaned result"}]
        });
        assert!(anthropic_to_openai(&no_call_id, &fable_route())
            .unwrap_err()
            .contains("COWORK_TOOL_RESULT_MISSING_ID"));
        let missing_role = json!({
            "model":"claude-fable-5-1",
            "messages":[{"content":"cannot invent a user role"}]
        });
        assert!(anthropic_to_openai(&missing_role, &fable_route())
            .unwrap_err()
            .contains("COWORK_MISSING_MESSAGE_ROLE"));
    }

    #[test]
    fn qwen38_legacy_route_defaults_to_no_thinking_and_keeps_farsi_greeting() {
        let legacy = json!({
            "routeId": "claude-fable-5-1", "displayName": "Qwen38",
            "upstreamModel": "qwen38", "tier": "fable",
            "contextWindow": 128000, "maxOutputTokens": 4096,
            "isDefault": true
        });
        let route: ModelRoute = serde_json::from_value(legacy).unwrap();
        assert_eq!(effective_thinking_mode(&route), "disabled");
        let request = json!({
            "model": "claude-fable-5-1",
            "system": [{"type":"text","text":"You are an assistant."}],
            "thinking": {"type":"adaptive"},
            "output_config": {"effort":"high"},
            "messages": [{"role":"user", "content":"سلام"}],
            "max_tokens": 200, "stream": false
        });
        let converted = anthropic_to_openai(&request, &route).unwrap();
        assert_eq!(converted["model"], "qwen38");
        assert_eq!(converted["messages"][0]["content"], "You are an assistant.");
        assert_eq!(converted["messages"][1]["role"], "user");
        assert_eq!(converted["messages"][1]["content"], "سلام");
        assert_eq!(converted["chat_template_kwargs"]["enable_thinking"], false);
        assert!(converted.get("reasoning_effort").is_none());
        assert!(converted["chat_template_kwargs"]
            .get("reasoning_effort")
            .is_none());
        assert!(converted.get("tools").is_none());
        assert!(converted.get("tool_choice").is_none());
    }

    #[test]
    fn qwen38_explicit_enable_and_other_models_are_independent() {
        let mut route = fable_route();
        route.thinking_mode = "enabled".into();
        let client_request = json!({
            "model": "claude-fable-5-1", "thinking": {"type": "disabled"},
            "messages": [{"role":"user", "content":"سلام"}]
        });
        let enabled = anthropic_to_openai(&client_request, &route).unwrap();
        assert_eq!(enabled["chat_template_kwargs"]["enable_thinking"], true);
        route.thinking_mode = "disabled".into();
        let disabled = anthropic_to_openai(&client_request, &route).unwrap();
        assert_eq!(disabled["chat_template_kwargs"]["enable_thinking"], false);
        route.upstream_model = "unrelated-model".into();
        route.thinking_mode = String::new();
        let unrelated = anthropic_to_openai(&client_request, &route).unwrap();
        assert_eq!(unrelated["chat_template_kwargs"]["enable_thinking"], false);
        let plain = anthropic_to_openai(
            &json!({
                "model":"claude-fable-5-1",
                "messages":[{"role":"user","content":"hi"}]
            }),
            &route,
        )
        .unwrap();
        assert!(plain.get("chat_template_kwargs").is_none());
    }

    #[test]
    fn tool_use_and_result_are_translated() {
        let request = json!({
            "model": "claude-fable-5-1",
            "max_tokens": 512,
            "messages": [
                { "role": "assistant", "content": [{ "type": "tool_use", "id": "call-1", "name": "lookup", "input": { "q": "hi" } }] },
                { "role": "user", "content": [{ "type": "tool_result", "tool_use_id": "call-1", "content": "found" }] }
            ]
        });
        let converted = anthropic_to_openai(&request, &fable_route()).unwrap();
        assert_eq!(
            converted["messages"][0]["tool_calls"][0]["function"]["name"],
            "lookup"
        );
        assert_eq!(converted["messages"][1]["role"], "tool");
        assert_eq!(converted["messages"][1]["tool_call_id"], "call-1");
    }

    #[test]
    fn mixed_tool_result_and_user_text_keep_valid_chat_completion_order() {
        let request = json!({
            "model": "claude-fable-5-1",
            "max_tokens": 512,
            "messages": [
                { "role": "assistant", "content": [
                    { "type": "tool_use", "id": "call-read", "name": "Read", "input": { "file_path": "README.md" } }
                ] },
                { "role": "user", "content": [
                    { "type": "text", "text": "Summarize only the file you just read." },
                    { "type": "tool_result", "tool_use_id": "call-read", "content": "README fixture result" }
                ] }
            ]
        });
        let converted = anthropic_to_openai(&request, &fable_route()).unwrap();
        let messages = converted["messages"].as_array().unwrap();
        assert_eq!(messages.len(), 3);
        assert_eq!(messages[0]["role"], "assistant");
        assert_eq!(messages[0]["tool_calls"][0]["id"], "call-read");
        assert_eq!(messages[1]["role"], "tool");
        assert_eq!(messages[1]["tool_call_id"], "call-read");
        assert_eq!(messages[1]["content"], "README fixture result");
        assert_eq!(messages[2]["role"], "user");
        assert_eq!(
            messages[2]["content"],
            "Summarize only the file you just read."
        );
    }

    #[test]
    fn isolated_probe_rejects_marker_only_wrong_language_or_wrong_position() {
        assert_eq!(
            isolated_probe_checks("FA-527", "FA-527", "Persian", 0),
            (false, true, false)
        );
        assert_eq!(
            isolated_probe_checks("Hello FA-527", "FA-527", "Persian", 0),
            (false, true, false)
        );
        assert_eq!(
            isolated_probe_checks("سلام FA-527 extra", "FA-527", "Persian", 0),
            (false, false, true)
        );
        assert_eq!(
            isolated_probe_checks("سلام FA-527", "FA-527", "Persian", 0),
            (true, true, true)
        );
        assert_eq!(
            isolated_probe_checks("你好 ZH-884", "ZH-884", "Chinese", 0),
            (true, true, true)
        );
        assert_eq!(
            isolated_probe_checks("你好 ZH-884", "ZH-884", "Chinese", 1),
            (false, true, true)
        );
    }

    #[test]
    fn cow_agent_one_token_budget_is_an_explicit_capacity_error() {
        let mut route = fable_route();
        route.context_window = 128;
        let request = json!({
            "model":"claude-fable-5-1",
            "max_tokens": 1024,
            "tools":[{"name":"Bash","description":"run a command",
                      "input_schema":{"type":"object"}}],
            "messages":[
                {"role":"user","content":"Cowork agent bootstrap"},
                {"role":"user","content":"سلام"}
            ]
        });
        let converted = anthropic_to_openai(&request, &route).unwrap();
        assert!(client_tools_take_precedence(&request));
        assert!(agent_output_budget_collapsed(
            &request,
            &converted,
            route.context_window
        ));
        assert!(!agent_output_budget_collapsed(&request, &converted, 0));
    }

    #[test]
    fn cowork_and_code_tool_context_is_never_proactively_trimmed() {
        let config = AppConfig::default();
        assert!(config.auto_trim_on_context_overflow);
        let agent_request = json!({
            "tools": [{"name":"Bash","input_schema":{"type":"object"}}],
            "messages": [
                {"role":"user","content":"<system-reminder>Bootstrap agent session</system-reminder>"},
                {"role":"user","content":"سلام"}
            ]
        });
        assert!(client_tools_take_precedence(&agent_request));
        assert!(!may_auto_trim_context(
            &config,
            client_tools_take_precedence(&agent_request)
        ));
        // A long tool definition should never justify erasing the bootstrap
        // turn before the provider has even evaluated the request.
        let mut payload = json!({"messages": [
            {"role":"system","content":"original system"},
            {"role":"user","content":"Cowork bootstrap"},
            {"role":"user","content":"سلام"}
        ]});
        if may_auto_trim_context(&config, true) {
            trim_to_context_budget(&mut payload, 1, 512);
        }
        assert_eq!(payload["messages"].as_array().unwrap().len(), 3);
        assert!(may_auto_trim_context(&config, false));
        let mut disabled = config;
        disabled.auto_trim_on_context_overflow = false;
        assert!(!may_auto_trim_context(&disabled, false));
    }

    #[test]
    fn two_user_messages_without_assistant_are_not_a_completed_turn() {
        let mut payload = json!({"messages":[
            {"role":"system","content":"Cowork system"},
            {"role":"user","content":"Cowork workspace bootstrap"},
            {"role":"user","content":"سلام"}
        ]});
        let before = payload.clone();
        assert!(!trim_oldest_turn(&mut payload));
        assert_eq!(payload, before);
        assert_eq!(trim_to_context_budget(&mut payload, 1, 512), 0);
        assert_eq!(payload, before);
    }

    #[test]
    fn context_recovery_drops_only_an_old_complete_turn() {
        let mut payload = json!({ "messages": [
            { "role": "system", "content": "keep system prompt" },
            { "role": "user", "content": "old request" },
            { "role": "assistant", "content": "old answer" },
            { "role": "user", "content": "current request" },
            { "role": "assistant", "content": null, "tool_calls": [{ "id": "t1" }] },
            { "role": "tool", "tool_call_id": "t1", "content": "tool result" },
            { "role": "assistant", "content": "current answer" }
        ]});
        assert!(trim_oldest_turn(&mut payload));
        assert_eq!(payload["messages"][0]["content"], "keep system prompt");
        assert_eq!(payload["messages"][1]["content"], "current request");
        assert_eq!(payload["messages"].as_array().unwrap().len(), 5);
        assert!(!trim_oldest_turn(&mut payload));
    }

    #[test]
    fn context_recovery_has_no_fixed_trim_budget() {
        // Regression test: long conversations used to stop being trimmed after
        // a fixed number of retries, dead-ending the chat while older complete
        // turns were still droppable. Trimming must continue until only the
        // current turn remains, no matter how many turns that takes.
        let mut payload = json!({ "messages": [
            { "role": "system", "content": "keep system prompt" },
        ]});
        for turn in 0..80u32 {
            let messages = payload["messages"].as_array_mut().unwrap();
            messages.push(json!({ "role": "user", "content": format!("old request {turn}") }));
            messages.push(json!({ "role": "assistant", "content": format!("old answer {turn}") }));
        }
        payload["messages"]
            .as_array_mut()
            .unwrap()
            .push(json!({ "role": "user", "content": "current request" }));
        let mut dropped = 0usize;
        while trim_oldest_turn(&mut payload) {
            dropped += 1;
        }
        assert_eq!(dropped, 80);
        assert_eq!(payload["messages"].as_array().unwrap().len(), 2);
        assert_eq!(payload["messages"][0]["content"], "keep system prompt");
        assert_eq!(payload["messages"][1]["content"], "current request");
    }

    #[test]
    fn continuation_output_limit_refits_after_context_is_trimmed() {
        let mut payload = json!({
            "max_tokens": 2_048,
            "messages": [
                { "role": "user", "content": "old request ".repeat(300) },
                { "role": "assistant", "content": "old answer ".repeat(300) },
                { "role": "user", "content": "current request" },
                { "role": "assistant", "content": "partial response ".repeat(300) },
                { "role": "user", "content": CONTINUE_PROMPT }
            ]
        });
        let context_window = estimate_tokens(&payload) + 512 + 100;

        refit_continuation_max_tokens(&mut payload, context_window, Some(2_048));
        let before_trim = payload["max_tokens"].as_u64().unwrap();
        assert_eq!(before_trim, 100);

        assert!(trim_oldest_turn(&mut payload));
        refit_continuation_max_tokens(&mut payload, context_window, Some(2_048));
        assert!(payload["max_tokens"].as_u64().unwrap() > before_trim);
        assert!(payload["max_tokens"].as_u64().unwrap() <= 2_048);
    }

    #[test]
    fn detects_gateway_input_overflow_error() {
        assert!(is_context_overflow(
            "Input length (199437 tokens) exceeds the maximum allowed length (199423 tokens)"
        ));
        assert!(!is_context_overflow("connection reset by peer"));
    }

    #[test]
    fn openai_completion_becomes_anthropic_message() {
        let completion = json!({
            "id": "chat-1",
            "choices": [{ "message": { "content": "hello" }, "finish_reason": "stop" }],
            "usage": { "prompt_tokens": 4, "completion_tokens": 2 }
        });
        let converted = openai_to_anthropic(&completion, "claude-fable-5-1").unwrap();
        assert_eq!(converted["type"], "message");
        assert_eq!(converted["content"][0]["text"], "hello");
        assert_eq!(converted["usage"]["input_tokens"], 4);
        assert_eq!(converted["stop_reason"], "end_turn");
    }

    #[tokio::test]
    async fn direct_connection_failure_reports_tcp_stage_without_false_tls_claim() {
        let dead = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = dead.local_addr().unwrap().port();
        drop(dead);
        let url = format!("http://127.0.0.1:{port}/v1/models");
        let client = build_direct_provider_client().unwrap();
        let error = client.get(&url).send().await.unwrap_err();
        let report = diagnose_provider_transport(&url, &AppConfig::default(), &error).await;
        assert!(report.starts_with("PROVIDER_TCP:"), "{report}");
        assert!(!report.contains("PROVIDER_TLS_OR_HTTP:"));
    }

    #[test]
    fn provider_route_mode_is_persistent_and_defaults_to_direct() {
        let legacy: DiskConfig = serde_json::from_str("{}").unwrap();
        assert_eq!(legacy.provider_network_mode, "direct");
        assert_eq!(normalize_provider_network_mode("SYSTEM"), "system");
        assert_eq!(normalize_provider_network_mode("auto"), "direct");
    }

    #[test]
    fn provider_url_adds_v1_only_when_missing() {
        assert_eq!(
            upstream_url("http://localhost:8000/api/v1", "models").unwrap(),
            "http://localhost:8000/api/v1/models"
        );
        assert_eq!(
            upstream_url("http://localhost:8000", "models").unwrap(),
            "http://localhost:8000/v1/models"
        );
    }

    #[test]
    fn web_settings_normalize_common_proxy_inputs() {
        assert_eq!(normalize_web_access_mode("AUTO"), "auto");
        assert_eq!(normalize_web_access_mode("anything else"), "disabled");
        assert_eq!(normalize_web_proxy_mode("manual"), "manual");
        assert_eq!(
            normalize_web_proxy_url("127.0.0.1:10808").unwrap(),
            "http://127.0.0.1:10808"
        );
        assert!(normalize_web_proxy_url("ftp://127.0.0.1:21").is_err());
        assert_eq!(
            normalize_web_search_url("").unwrap(),
            DEFAULT_WEB_SEARCH_URL
        );
    }

    #[test]
    fn web_url_guard_blocks_local_targets() {
        assert!(validate_web_url("http://127.0.0.1:18181").is_err());
        assert!(validate_web_url("http://localhost:8080").is_err());
        assert!(validate_web_url("https://example.com").is_ok());
        assert!(validate_web_url("https://user:secret@example.com").is_err());
    }

    #[test]
    fn web_tools_are_added_even_when_client_did_not_send_tools() {
        let mut config = AppConfig::default();
        config.web_access_mode = "auto".into();
        let mut payload = json!({ "messages": [{ "role": "user", "content": "latest news" }] });
        assert!(add_web_tools(&mut payload, &config, WebIntent::Automatic));
        assert_eq!(payload["tools"].as_array().unwrap().len(), 2);
        assert_eq!(
            payload["tools"][0]["function"]["name"],
            "claude_proxy_web_search"
        );
    }

    #[test]
    fn simple_messages_skip_the_web_bridge_entirely() {
        let request = json!({ "messages": [
            { "role": "user", "content": "سلام" },
            { "role": "assistant", "content": "سلام! چطور می‌تونم کمکت کنم؟" },
            { "role": "user", "content": "دقیق بررسی کن ببین چرا این اشتباه رو کردی" }
        ] });
        assert_eq!(classify_web_intent(&request), WebIntent::NoWeb);

        let mut config = AppConfig::default();
        config.web_access_mode = "auto".into();
        let mut payload = json!({ "messages": [{ "role": "user", "content": "hello" }] });
        assert!(!add_web_tools(&mut payload, &config, WebIntent::NoWeb));
        assert!(payload.get("tools").is_none());
        assert!(payload.get("tool_choice").is_none());
        assert!(payload["messages"]
            .as_array()
            .unwrap()
            .iter()
            .all(|message| message.get("role").and_then(Value::as_str) != Some("system")));
    }

    #[test]
    fn fresh_information_requests_keep_the_web_bridge() {
        let request = json!({ "messages": [
            { "role": "user", "content": "what's the latest Claude Agent SDK release?" }
        ] });
        assert_ne!(classify_web_intent(&request), WebIntent::NoWeb);

        let persian =
            json!({ "messages": [{ "role": "user", "content": "آخرین قیمت بیت‌کوین چیه؟" }] });
        assert_ne!(classify_web_intent(&persian), WebIntent::NoWeb);
    }

    #[test]
    fn old_repository_context_does_not_trigger_web_on_new_greeting() {
        let request = json!({ "messages": [
            { "role": "user", "content": "ببین توی این ریپو چه کدی هست" },
            { "role": "assistant", "content": "ok" },
            { "role": "user", "content": "سلام" }
        ] });
        assert_eq!(classify_web_intent(&request), WebIntent::NoWeb);
        let mut config = AppConfig::default();
        config.web_access_mode = "required".into();
        let mut payload = json!({ "messages": [{ "role": "user", "content": "سلام" }] });
        assert!(!add_web_tools(
            &mut payload,
            &config,
            classify_web_intent(&request)
        ));
        assert!(payload.get("tools").is_none());
        assert!(payload.get("tool_choice").is_none());
    }

    #[test]
    fn old_repository_context_does_not_trigger_web_on_language_only_turns() {
        for directive in [
            "فقط به فارسی پاسخ بده و عبارت FA-527 را در پایان بنویس.",
            "请只用中文回答，并在结尾写 ZH-884",
            "hello",
        ] {
            let request = json!({ "messages": [
                { "role": "user", "content": "Please search GitHub for an API project" },
                { "role": "assistant", "content": "I found some candidates." },
                { "role": "user", "content": directive }
            ] });
            assert_eq!(classify_web_intent(&request), WebIntent::NoWeb);
        }
    }

    #[test]
    fn greeting_with_claude_environment_metadata_never_requests_web() {
        let raw = "<system-reminder>\nWorking in a GitHub repository. Search the latest source.\n</system-reminder>\nسلام";
        assert_eq!(user_text_for_web_intent(raw), "سلام");
        let request = json!({
            "messages": [
                {"role":"user", "content":"Please search GitHub for a repository"},
                {"role":"assistant", "content":"Found a candidate."},
                {"role":"user", "content":[{"type":"text", "text":raw}]}
            ]
        });
        assert_eq!(classify_web_intent(&request), WebIntent::NoWeb);
        let mut config = AppConfig::default();
        config.web_access_mode = "required".into();
        let mut payload = json!({"messages":[{"role":"user", "content":raw}]});
        assert!(!add_web_tools(&mut payload, &config, WebIntent::NoWeb));
        assert_eq!(payload["messages"][0]["content"], raw);
        assert!(payload.get("tools").is_none());
        assert!(payload.get("tool_choice").is_none());
    }

    #[test]
    fn implicit_repository_mentions_do_not_invoke_web_tools() {
        for user_text in [
            "سلام",
            "I'm in a GitHub repository today.",
            "There is a repo called demo/test.",
            "<system-reminder>Search GitHub</system-reminder>hello",
        ] {
            let request = json!({"messages":[{"role":"user","content":user_text}]});
            assert_eq!(classify_web_intent(&request), WebIntent::NoWeb);
        }
        let direct =
            json!({"messages":[{"role":"user","content":"Please search GitHub for demo/test"}]});
        assert_eq!(classify_web_intent(&direct), WebIntent::Search);
    }

    #[test]
    fn reused_repository_opinion_does_not_inject_implicit_tools() {
        let mut config = AppConfig::default();
        config.web_access_mode = "required".into();
        let mut payload = json!({"messages":[{"role":"user","content":"What do you think?"}]});
        assert!(!add_web_tools(
            &mut payload,
            &config,
            WebIntent::ReuseContext
        ));
        assert!(payload.get("tools").is_none());
    }

    #[test]
    fn explicit_revisit_of_previous_repository_still_enables_web() {
        let request = json!({ "messages": [
            { "role": "user", "content": "Review the GitHub repository example/repo" },
            { "role": "assistant", "content": "Ready." },
            { "role": "user", "content": "دوباره ریپو رو بررسی کن" }
        ] });
        assert_eq!(classify_web_intent(&request), WebIntent::Fetch);
        let request2 = json!({ "messages": [
            { "role": "user", "content": "Review the GitHub repository example/repo" },
            { "role": "assistant", "content": "Ready." },
            { "role": "user", "content": "نظرت چیه؟" }
        ] });
        assert_eq!(classify_web_intent(&request2), WebIntent::ReuseContext);
    }

    #[test]
    fn strict_v2_preserves_client_tool_ownership_even_for_github_tasks() {
        let request = json!({
            "model": "claude-sonnet-5-5",
            "messages": [{"role":"user","content":"Read my local GitHub repo"}],
            "tools": [{"name":"Read","input_schema":{"type":"object"}}]
        });
        assert!(client_tools_take_precedence(&request));

        let mut config = AppConfig::default();
        config.web_access_mode = "required".into();
        config.shell_access_mode = "auto".into();
        let mut payload = anthropic_to_openai(
            &request,
            &ModelRoute {
                route_id: "claude-sonnet-5-5".into(),
                display_name: "Test".into(),
                upstream_model: "mock-upstream".into(),
                tier: "sonnet".into(),
                thinking_mode: String::new(),
                context_window: 128_000,
                max_output_tokens: 4_096,
                is_default: true,
            },
        )
        .unwrap();
        let web_intent = if client_tools_take_precedence(&request) {
            WebIntent::NoWeb
        } else {
            classify_web_intent(&request)
        };
        assert!(!add_web_tools(&mut payload, &config, web_intent));
        assert_eq!(payload["tools"].as_array().unwrap().len(), 1);
        assert_eq!(payload["tools"][0]["function"]["name"], "Read");
        assert!(payload.get("tool_choice").is_none());
    }

    #[test]
    fn web_access_never_authorizes_host_shell_and_shell_only_mode_works() {
        assert!(local_tool_allowed("claude_proxy_web_fetch", true, false));
        assert!(!local_tool_allowed("claude_proxy_shell", true, false));
        assert!(!local_tool_allowed("claude_proxy_web_search", false, true));
        assert!(local_tool_allowed("claude_proxy_shell", false, true));
        assert!(!local_tool_allowed("terminal", true, true));
        assert!(!local_tool_allowed("Read", true, true));
    }

    #[test]
    fn only_local_web_tool_calls_are_executed() {
        let completion = json!({
            "choices": [{ "message": { "tool_calls": [{
                "id": "call-1",
                "function": { "name": "claude_proxy_web_fetch", "arguments": "{\"url\":\"https://example.com\"}" }
            }] } }]
        });
        let calls = local_tool_calls(&completion).unwrap();
        assert_eq!(calls[0].0, "call-1");
        assert_eq!(calls[0].1, "claude_proxy_web_fetch");
        assert_eq!(calls[0].2["url"], "https://example.com");

        let client_tool = json!({
            "choices": [{ "message": { "tool_calls": [{
                "id": "call-2",
                "function": { "name": "client_lookup", "arguments": "{}" }
            }] } }]
        });
        assert!(local_tool_calls(&client_tool).is_none());
    }
}

#[cfg(test)]
#[path = "v2/gateway_integration.rs"]
mod v2_gateway_integration;
