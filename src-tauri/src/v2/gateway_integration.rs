//! End-to-end local HTTP regression: Claude-client request -> proxy -> fake
//! provider -> client tool_use -> client tool_result -> grounded final answer.
//! No real Claude Desktop, provider subscription or filesystem write is used.
use super::*;
use std::sync::atomic::AtomicUsize;

async fn fake_provider(Json(payload): Json<Value>) -> Json<Value> {
    let tools = payload.get("tools").and_then(Value::as_array);
    let messages = payload["messages"].as_array().expect("converted messages");
    let last = messages.last().expect("last message");
    let model = payload["model"].as_str().unwrap_or("missing");
    assert_eq!(model, "mock-upstream");
    let completion = if last["role"] == "tool" {
        assert_eq!(last["tool_call_id"], "call-readme-1");
        assert_eq!(
            last["content"],
            "# Example project\nTwo modules need tests."
        );
        json!({
            "id":"completion-final",
            "choices":[{"finish_reason":"stop","message":{
                "role":"assistant",
                "content":"I inspected README.md: the project needs tests for two modules."
            }}],
            "usage":{"prompt_tokens":220,"completion_tokens":18}
        })
    } else {
        assert_eq!(last["role"], "user");
        let tools = tools.expect("client-supplied Read tool preserved");
        assert_eq!(tools[0]["function"]["name"], "Read");
        json!({
            "id":"completion-tool",
            "choices":[{"finish_reason":"tool_calls","message":{
                "role":"assistant", "content":null,
                "tool_calls":[{"id":"call-readme-1","type":"function",
                    "function":{"name":"Read","arguments":"{\"path\":\"README.md\"}"}}]
            }}],
            "usage":{"prompt_tokens":180,"completion_tokens":12}
        })
    };
    Json(completion)
}

fn test_server_state(provider_port: u16, mode: ContractMode) -> (ServerState, std::path::PathBuf) {
    let mut config = AppConfig::default();
    config.base_url = format!("http://127.0.0.1:{provider_port}/v1");
    config.api_key = "mock-secret".into();
    config.gateway_token = "test-gateway-token".into();
    config.routes = vec![ModelRoute {
        route_id: "claude-sonnet-5-5".into(),
        display_name: "Sonnet test".into(),
        upstream_model: "mock-upstream".into(),
        tier: "sonnet".into(),
        context_window: 128_000,
        max_output_tokens: 4_096,
        is_default: true,
    }];
    let dir = std::env::temp_dir().join(format!("claude-proxy-v2-test-{}", uuid::Uuid::new_v4()));
    let client = reqwest::Client::new();
    (
        ServerState {
            config: Arc::new(RwLock::new(config)),
            upstream_clients: Arc::new(vec![client]),
            direct_provider_client: build_direct_provider_client().unwrap(),
            upstream_cursor: Arc::new(AtomicUsize::new(0)),
            upstream_slots: Arc::new(Semaphore::new(2)),
            web_slots: Arc::new(Semaphore::new(2)),
            shell_slots: Arc::new(Semaphore::new(2)),
            observability: ObservabilityStore::new(dir.join("usage.jsonl"), dir.join("logs.jsonl")),
            active_requests: Arc::new(AtomicUsize::new(0)),
            v2_contract_mode: mode,
            v2_stream_enabled: false,
            v2_output_guard_enabled: false,
            v2_capabilities: CapabilitySettings::from_strings("", ""),
        },
        dir,
    )
}

#[tokio::test]
async fn real_http_gateway_preserves_two_turn_code_tool_loop() {
    let provider = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let provider_port = provider.local_addr().unwrap().port();
    let provider_task = tokio::spawn(async move {
        let mock = Router::new().route("/v1/chat/completions", post(fake_provider));
        axum::serve(provider, mock).await.unwrap();
    });

    let (state, dir) = test_server_state(provider_port, ContractMode::Enforce);
    let gateway = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let gateway_port = gateway.local_addr().unwrap().port();
    let gateway_task = tokio::spawn(async move {
        let router = Router::new()
            .route("/v1/messages", post(anthropic_messages))
            .with_state(state);
        axum::serve(gateway, router).await.unwrap();
    });

    let endpoint = format!("http://127.0.0.1:{gateway_port}/v1/messages");
    let client = reqwest::Client::new();
    let user = json!({"role":"user","content":"Read the repository and review the project."});
    let request = json!({
        "model":"claude-sonnet-5-5","stream":false,"max_tokens":512,
        "tools":[{"name":"Read","description":"Read a file",
                 "input_schema":{"type":"object","properties":{"path":{"type":"string"}}}}],
        "messages":[user]
    });
    let first = client
        .post(&endpoint)
        .bearer_auth("test-gateway-token")
        .json(&request)
        .send()
        .await
        .unwrap();
    assert_eq!(first.status(), reqwest::StatusCode::OK);
    let first: Value = first.json().await.unwrap();
    assert_eq!(first["stop_reason"], "tool_use");
    assert_eq!(first["content"][0]["type"], "tool_use");
    assert_eq!(first["content"][0]["id"], "call-readme-1");
    assert_eq!(first["content"][0]["name"], "Read");
    assert_eq!(first["content"][0]["input"]["path"], "README.md");

    // In a real Claude Code client, this is where the client's approved
    // Read tool executes. We inject a deterministic fixture file result.
    let followup = json!({
        "model":"claude-sonnet-5-5","stream":false,"max_tokens":512,
        "messages":[
            {"role":"user","content":"Read the repository and review the project."},
            {"role":"assistant","content":first["content"]},
            {"role":"user","content":[{"type":"tool_result",
              "tool_use_id":"call-readme-1",
              "content":"# Example project\nTwo modules need tests."}]}
        ]
    });
    let second = client
        .post(&endpoint)
        .bearer_auth("test-gateway-token")
        .json(&followup)
        .send()
        .await
        .unwrap();
    assert_eq!(second.status(), reqwest::StatusCode::OK);
    let second: Value = second.json().await.unwrap();
    assert_eq!(second["stop_reason"], "end_turn");
    assert!(second["content"][0]["text"]
        .as_str()
        .unwrap()
        .contains("README.md"));
    assert!(second["content"][0]["text"]
        .as_str()
        .unwrap()
        .contains("two modules"));

    // Fail closed rather than silently dropping document context.
    let rejected = client
        .post(&endpoint)
        .bearer_auth("test-gateway-token")
        .json(&json!({"model":"claude-sonnet-5-5","messages":[{
            "role":"user","content":[{"type":"document","source":{"type":"base64",
            "media_type":"application/pdf","data":"secret"}}]}]}))
        .send()
        .await
        .unwrap();
    assert_eq!(rejected.status(), reqwest::StatusCode::BAD_REQUEST);
    let error: Value = rejected.json().await.unwrap();
    assert_eq!(error["error"]["type"], "invalid_request_error");

    gateway_task.abort();
    provider_task.abort();
    let _ = std::fs::remove_dir_all(dir);
}

async fn streamed_mock_provider(Json(payload): Json<Value>) -> Response {
    let messages = payload["messages"].as_array().expect("messages");
    let last = messages.last().expect("last message");
    let mut chunks = Vec::new();
    let requested = last["content"].as_str().unwrap_or("");
    let frame = |value: Value| format!("data: {}\n\n", value);
    if last["role"] == "tool" {
        assert_eq!(last["tool_call_id"], "call-readme");
        assert_eq!(last["content"], "README test fixture");
        chunks.push(frame(
            json!({"choices":[{"delta":{"content":"Inspected README.md."}}]}),
        ));
        chunks.push(frame(
            json!({"choices":[{"delta":{},"finish_reason":"stop"}]}),
        ));
    } else if requested == "bad stream" {
        chunks.push(frame(json!({"choices":[{"delta":{"tool_calls":[
            {"index":0,"id":"broken-id","function":{"name":"Read","arguments":"{invalid"}}
        ]}}]})));
        chunks.push(frame(
            json!({"choices":[{"delta":{},"finish_reason":"tool_calls"}]}),
        ));
    } else if requested == "disconnect" {
        chunks.push(frame(
            json!({"choices":[{"delta":{"content":"partial only"}}]}),
        ));
        // Deliberately close upstream without finish_reason or [DONE].
        return (
            [(header::CONTENT_TYPE, "text/event-stream")],
            chunks.join(""),
        )
            .into_response();
    } else if requested == "finish without done" {
        chunks.push(frame(
            json!({"choices":[{"delta":{"content":"partial response"},
            "finish_reason":"stop"}]}),
        ));
        // A provider may die after finish_reason but before its usage/[DONE].
        return (
            [(header::CONTENT_TYPE, "text/event-stream")],
            chunks.join(""),
        )
            .into_response();
    } else if requested == "missing output usage" {
        chunks.push(frame(
            json!({"choices":[{"delta":{"content":"has no verified usage"},
            "finish_reason":"stop"}]}),
        ));
        chunks.push("data: [DONE]\n\n".into());
        return (
            [(header::CONTENT_TYPE, "text/event-stream")],
            chunks.join(""),
        )
            .into_response();
    } else if requested == "over budget" {
        chunks.push(frame(
            json!({"choices":[{"delta":{"content":"text"},"finish_reason":"stop"}]}),
        ));
    } else if requested == "truncated" {
        chunks.push(frame(
            json!({"choices":[{"delta":{"content":"unfinished work"},"finish_reason":"length"}]}),
        ));
    } else {
        assert_eq!(payload["stream"], true);
        assert_eq!(payload["tools"][0]["function"]["name"], "Read");
        chunks.push(frame(
            json!({"choices":[{"delta":{"content":"Inspecting repository."}}]}),
        ));
        chunks.push(frame(json!({"choices":[{"delta":{"tool_calls":[
            {"index":1,"function":{"arguments":"{\"pattern\":"}},
            {"index":0,"function":{"arguments":"{\"path\":"}}
        ]}}]})));
        chunks.push(frame(json!({"choices":[{"delta":{"tool_calls":[
            {"index":0,"id":"call-readme","function":{"name":"Re","arguments":"\"README.md\"}"}},
            {"index":1,"id":"call-grep","function":{"name":"Grep","arguments":"\"TODO\"}"}}
        ]}}]})));
        chunks.push(frame(json!({"choices":[{"delta":{"tool_calls":[
            {"index":0,"function":{"name":"ad"}}
        ]},"finish_reason":"tool_calls"}]})));
    }
    let output = if requested == "over budget" { 129 } else { 30 };
    chunks.push(frame(json!({"choices":[],"usage":{
        "prompt_tokens":120,"completion_tokens":output
    }})));
    chunks.push("data: [DONE]\n\n".into());
    (
        [(header::CONTENT_TYPE, "text/event-stream")],
        chunks.join(""),
    )
        .into_response()
}

async fn start_streaming_fixture() -> (
    String,
    tokio::task::JoinHandle<()>,
    tokio::task::JoinHandle<()>,
    std::path::PathBuf,
) {
    start_streaming_fixture_with_guard(false).await
}

async fn start_streaming_fixture_with_guard(
    output_guard: bool,
) -> (
    String,
    tokio::task::JoinHandle<()>,
    tokio::task::JoinHandle<()>,
    std::path::PathBuf,
) {
    let provider = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = provider.local_addr().unwrap().port();
    let upstream = tokio::spawn(async move {
        axum::serve(
            provider,
            Router::new().route("/v1/chat/completions", post(streamed_mock_provider)),
        )
        .await
        .unwrap();
    });
    let (mut state, dir) = test_server_state(port, ContractMode::Enforce);
    state.v2_stream_enabled = true;
    state.v2_output_guard_enabled = output_guard;
    let gateway = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let gateway_port = gateway.local_addr().unwrap().port();
    let server = tokio::spawn(async move {
        axum::serve(
            gateway,
            Router::new()
                .route("/v1/messages", post(anthropic_messages))
                .with_state(state),
        )
        .await
        .unwrap();
    });
    (
        format!("http://127.0.0.1:{gateway_port}/v1/messages"),
        upstream,
        server,
        dir,
    )
}

async fn send_streaming_fixture(endpoint: &str, content: &str, with_tools: bool) -> Vec<Value> {
    let mut body = json!({
        "model":"claude-sonnet-5-5","stream":true,"max_tokens":128,
        "messages":[{"role":"user","content":content}]
    });
    if with_tools {
        body["tools"] = json!([
            {"name":"Read","input_schema":{"type":"object"}},
            {"name":"Grep","input_schema":{"type":"object"}}
        ]);
    }
    let response = reqwest::Client::new()
        .post(endpoint)
        .bearer_auth("test-gateway-token")
        .json(&body)
        .send()
        .await
        .unwrap();
    assert!(response.status().is_success());
    let raw = response.text().await.unwrap();
    raw.split("\n\n")
        .filter_map(|s| s.lines().find_map(|line| line.strip_prefix("data: ")))
        .map(|v| serde_json::from_str::<Value>(v).unwrap())
        .collect()
}

#[tokio::test]
async fn v2_sse_http_parallel_tools_have_complete_ids_and_valid_order() {
    let (endpoint, upstream, server, dir) = start_streaming_fixture().await;
    let events = send_streaming_fixture(&endpoint, "inspect repo", true).await;
    let names: Vec<&str> = events
        .iter()
        .filter_map(|v| v.get("type").and_then(Value::as_str))
        .collect();
    assert_eq!(names.first(), Some(&"message_start"));
    assert_eq!(names.last(), Some(&"message_stop"));
    let starts: Vec<&Value> = events
        .iter()
        .filter(|e| e["type"] == "content_block_start")
        .collect();
    assert_eq!(starts.len(), 3);
    assert_eq!(starts[0]["content_block"]["type"], "text");
    assert_eq!(starts[1]["content_block"]["id"], "call-readme");
    assert_eq!(starts[1]["content_block"]["name"], "Read");
    assert_eq!(starts[2]["content_block"]["id"], "call-grep");
    assert_eq!(starts[2]["content_block"]["name"], "Grep");
    let deltas: Vec<&Value> = events
        .iter()
        .filter(|e| e["type"] == "content_block_delta")
        .collect();
    assert_eq!(
        deltas[1]["delta"]["partial_json"],
        r#"{"path":"README.md"}"#
    );
    assert_eq!(deltas[2]["delta"]["partial_json"], r#"{"pattern":"TODO"}"#);
    let finish = events
        .iter()
        .find(|e| e["type"] == "message_delta")
        .unwrap();
    assert_eq!(finish["delta"]["stop_reason"], "tool_use");
    assert_eq!(finish["usage"]["output_tokens"], 30);
    upstream.abort();
    server.abort();
    let _ = std::fs::remove_dir_all(dir);
}

#[tokio::test]
async fn v2_sse_http_broken_tool_call_yields_error_without_false_success() {
    let (endpoint, upstream, server, dir) = start_streaming_fixture().await;
    let events = send_streaming_fixture(&endpoint, "bad stream", false).await;
    assert!(events.iter().any(|e| e["type"] == "error"));
    assert!(!events.iter().any(|e| e["type"] == "message_stop"));
    assert!(!events.iter().any(|e| e["type"] == "content_block_start"));
    upstream.abort();
    server.abort();
    let _ = std::fs::remove_dir_all(dir);
}

#[tokio::test]
async fn v2_sse_http_verified_over_budget_emits_error_not_fake_stop() {
    let (endpoint, upstream, server, dir) = start_streaming_fixture().await;
    let events = send_streaming_fixture(&endpoint, "over budget", false).await;
    assert!(events.iter().any(|e| e["type"] == "error"));
    assert!(!events.iter().any(|e| e["type"] == "message_stop"));
    upstream.abort();
    server.abort();
    let _ = std::fs::remove_dir_all(dir);
}

#[tokio::test]
async fn v2_sse_http_length_remains_truncated_without_hidden_continuation() {
    let (endpoint, upstream, server, dir) = start_streaming_fixture().await;
    let events = send_streaming_fixture(&endpoint, "truncated", false).await;
    let end = events
        .iter()
        .find(|e| e["type"] == "message_delta")
        .unwrap();
    assert_eq!(end["delta"]["stop_reason"], "max_tokens");
    assert!(events.iter().any(|e| e["type"] == "message_stop"));
    upstream.abort();
    server.abort();
    let _ = std::fs::remove_dir_all(dir);
}

#[tokio::test]
async fn v2_sse_http_upstream_disconnect_reports_error_instead_of_completion() {
    let (endpoint, upstream, server, dir) = start_streaming_fixture().await;
    let events = send_streaming_fixture(&endpoint, "disconnect", false).await;
    assert!(events.iter().any(|e| e["type"] == "error"));
    assert!(!events.iter().any(|e| e["type"] == "message_stop"));
    upstream.abort();
    server.abort();
    let _ = std::fs::remove_dir_all(dir);
}

#[tokio::test]
async fn v2_capability_gate_blocks_unverified_code_tools_before_upstream_io() {
    let (mut state, dir) = test_server_state(1, ContractMode::Enforce);
    state.v2_capabilities = CapabilitySettings::from_strings(
        "enforce",
        r#"{"mock-upstream":{"tools":false,"streaming":true}}"#,
    );
    let gateway = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = gateway.local_addr().unwrap().port();
    let server = tokio::spawn(async move {
        axum::serve(
            gateway,
            Router::new()
                .route("/v1/messages", post(anthropic_messages))
                .with_state(state),
        )
        .await
        .unwrap();
    });
    let response = reqwest::Client::new()
        .post(format!("http://127.0.0.1:{port}/v1/messages"))
        .bearer_auth("test-gateway-token")
        .json(&json!({
            "model": "claude-sonnet-5-5",
            "max_tokens": 512, "stream": false,
            "tools": [{"name": "Read", "input_schema": {"type": "object"}}],
            "messages": [{"role": "user", "content": "Inspect the repository."}]
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), reqwest::StatusCode::BAD_REQUEST);
    let response: Value = response.json().await.unwrap();
    assert!(response["error"]["message"]
        .as_str()
        .unwrap()
        .contains("not verified for tool calling"));
    server.abort();
    let _ = std::fs::remove_dir_all(dir);
}

async fn malformed_tool_provider(Json(_body): Json<Value>) -> Json<Value> {
    Json(json!({
        "id": "invalid-tool-response",
        "choices": [{"finish_reason": "tool_calls", "message": {
            "role": "assistant", "content": null,
            "tool_calls": [{"type": "function", "function": {
                "name": "Read", "arguments": "{}"
            }}]
        }}]
    }))
}

#[tokio::test]
async fn v2_strict_provider_response_rejects_missing_tool_id_over_real_http() {
    let provider = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let provider_port = provider.local_addr().unwrap().port();
    let upstream = tokio::spawn(async move {
        axum::serve(
            provider,
            Router::new().route("/v1/chat/completions", post(malformed_tool_provider)),
        )
        .await
        .unwrap();
    });
    let (state, dir) = test_server_state(provider_port, ContractMode::Enforce);
    let gateway = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = gateway.local_addr().unwrap().port();
    let server = tokio::spawn(async move {
        axum::serve(
            gateway,
            Router::new()
                .route("/v1/messages", post(anthropic_messages))
                .with_state(state),
        )
        .await
        .unwrap();
    });
    let response = reqwest::Client::new()
        .post(format!("http://127.0.0.1:{port}/v1/messages"))
        .bearer_auth("test-gateway-token")
        .json(&json!({"model": "claude-sonnet-5-5", "max_tokens": 256,
            "stream": false, "messages": [{"role": "user", "content": "Read file"}]}))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), reqwest::StatusCode::BAD_GATEWAY);
    let value: Value = response.json().await.unwrap();
    assert!(value["error"]["message"]
        .as_str()
        .unwrap()
        .contains("invalid tool response"));
    upstream.abort();
    server.abort();
    let _ = std::fs::remove_dir_all(dir);
}

async fn output_budget_mock_provider(Json(payload): Json<Value>) -> Json<Value> {
    let requested = payload["max_tokens"]
        .as_u64()
        .expect("output budget supplied");
    assert!(requested > 0 && requested <= 32_000);
    let scenario = payload
        .pointer("/messages/0/content")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let usage = match scenario {
        "exceeded" => Some(json!({"prompt_tokens":100,"completion_tokens":requested + 1})),
        "unreported" => None,
        _ => Some(json!({"prompt_tokens":100,"completion_tokens":requested.min(8)})),
    };
    let mut answer = json!({
        "id":"mock-output",
        "choices":[{"finish_reason":"stop","message":{
            "role":"assistant","content":"The model stopped at the negotiated limit."
        }}]
    });
    if let Some(usage) = usage {
        answer["usage"] = usage;
    }
    Json(answer)
}

#[tokio::test]
async fn v2_output_cap_http_rejects_unmeasured_and_overlimit_provider_replies() {
    let provider = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let provider_port = provider.local_addr().unwrap().port();
    let upstream = tokio::spawn(async move {
        axum::serve(
            provider,
            Router::new().route("/v1/chat/completions", post(output_budget_mock_provider)),
        )
        .await
        .unwrap();
    });
    let (mut state, dir) = test_server_state(provider_port, ContractMode::Enforce);
    state.v2_output_guard_enabled = true;
    state.config.write().await.routes[0].max_output_tokens = 64_000;
    let gateway = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = gateway.local_addr().unwrap().port();
    let server = tokio::spawn(async move {
        axum::serve(
            gateway,
            Router::new()
                .route("/v1/messages", post(anthropic_messages))
                .with_state(state),
        )
        .await
        .unwrap();
    });
    let client = reqwest::Client::new();
    let url = format!("http://127.0.0.1:{port}/v1/messages");
    for (scenario, status) in [
        ("within limit", reqwest::StatusCode::OK),
        ("exceeded", reqwest::StatusCode::BAD_GATEWAY),
        ("unreported", reqwest::StatusCode::BAD_GATEWAY),
    ] {
        let response = client
            .post(&url)
            .bearer_auth("test-gateway-token")
            .json(&json!({
                "model":"claude-sonnet-5-5",
                "stream":false,"max_tokens":64000,
                "messages":[{"role":"user","content":scenario}]
            }))
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), status, "scenario={scenario}");
        let value: Value = response.json().await.unwrap();
        if status == reqwest::StatusCode::OK {
            assert_eq!(value["stop_reason"], "end_turn");
        } else {
            assert_eq!(value["error"]["type"], "api_error");
        }
    }
    server.abort();
    upstream.abort();
    let _ = std::fs::remove_dir_all(dir);
}

#[tokio::test]
async fn v2_output_cap_requires_strict_sse_for_streamed_requests() {
    let (mut state, dir) = test_server_state(1, ContractMode::Enforce);
    state.v2_output_guard_enabled = true;
    state.v2_stream_enabled = false;
    let gateway = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = gateway.local_addr().unwrap().port();
    let server = tokio::spawn(async move {
        axum::serve(
            gateway,
            Router::new()
                .route("/v1/messages", post(anthropic_messages))
                .with_state(state),
        )
        .await
        .unwrap();
    });
    let response = reqwest::Client::new()
        .post(format!("http://127.0.0.1:{port}/v1/messages"))
        .bearer_auth("test-gateway-token")
        .json(&json!({
            "model":"claude-sonnet-5-5","stream":true,"max_tokens":64000,
            "messages":[{"role":"user","content":"write very long answer"}]
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), reqwest::StatusCode::BAD_REQUEST);
    let value: Value = response.json().await.unwrap();
    assert!(value["error"]["message"]
        .as_str()
        .unwrap()
        .contains("CLAUDE_PROXY_V2_STREAM"));
    server.abort();
    let _ = std::fs::remove_dir_all(dir);
}

#[tokio::test]
async fn v2_strict_sse_requires_terminal_done_even_after_finish_reason() {
    let (endpoint, upstream, server, dir) = start_streaming_fixture().await;
    let events = send_streaming_fixture(&endpoint, "finish without done", false).await;
    assert!(events.iter().any(|event| event["type"] == "error"));
    assert!(!events.iter().any(|event| event["type"] == "message_stop"));
    upstream.abort();
    server.abort();
    let _ = std::fs::remove_dir_all(dir);
}

#[tokio::test]
async fn v2_strict_desktop_sse_guard_fails_on_missing_measured_usage() {
    let (endpoint, upstream, server, dir) = start_streaming_fixture_with_guard(true).await;
    let events = send_streaming_fixture(&endpoint, "missing output usage", false).await;
    assert!(events.iter().any(|event| event["type"] == "error"));
    assert!(!events.iter().any(|event| event["type"] == "message_stop"));
    upstream.abort();
    server.abort();
    let _ = std::fs::remove_dir_all(dir);
}

#[tokio::test]
async fn v2_strict_desktop_sse_guard_detects_provider_overlimit() {
    let (endpoint, upstream, server, dir) = start_streaming_fixture_with_guard(true).await;
    let events = send_streaming_fixture(&endpoint, "over budget", false).await;
    assert!(events.iter().any(|event| event["type"] == "error"));
    assert!(!events.iter().any(|event| event["type"] == "message_stop"));
    upstream.abort();
    server.abort();
    let _ = std::fs::remove_dir_all(dir);
}

#[tokio::test]
async fn v2_strict_desktop_sse_guard_passes_with_measured_usage() {
    let (endpoint, upstream, server, dir) = start_streaming_fixture_with_guard(true).await;
    let events = send_streaming_fixture(&endpoint, "truncated", false).await;
    assert!(!events.iter().any(|event| event["type"] == "error"));
    assert!(events.iter().any(|event| event["type"] == "message_stop"));
    let completion = events
        .iter()
        .find(|e| e["type"] == "message_delta")
        .unwrap();
    assert_eq!(completion["delta"]["stop_reason"], "max_tokens");
    assert_eq!(completion["usage"]["output_tokens"], 30);
    upstream.abort();
    server.abort();
    let _ = std::fs::remove_dir_all(dir);
}

async fn session_echo_mock(Json(payload): Json<Value>) -> Json<Value> {
    let text = payload["messages"]
        .as_array()
        .expect("messages")
        .iter()
        .rev()
        .find(|item| item["role"] == "user")
        .and_then(|item| item["content"].as_str())
        .expect("user text");
    Json(json!({
        "id": "provider-echo",
        "choices": [{"finish_reason":"stop","message":{
            "role":"assistant","content":format!("Response for {text}")
        }}],
        "usage": {"prompt_tokens":40,"completion_tokens":8}
    }))
}

#[tokio::test]
async fn v2_concurrent_fresh_sessions_do_not_mix_user_messages() {
    let provider = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let upstream_port = provider.local_addr().unwrap().port();
    let upstream = tokio::spawn(async move {
        axum::serve(
            provider,
            Router::new().route("/v1/chat/completions", post(session_echo_mock)),
        )
        .await
        .unwrap();
    });
    let (state, dir) = test_server_state(upstream_port, ContractMode::Enforce);
    let gateway = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = gateway.local_addr().unwrap().port();
    let server = tokio::spawn(async move {
        axum::serve(
            gateway,
            Router::new()
                .route("/v1/messages", post(anthropic_messages))
                .with_state(state),
        )
        .await
        .unwrap();
    });
    let client = reqwest::Client::new();
    let mut tasks = Vec::new();
    for n in 0..12 {
        let client = client.clone();
        let url = format!("http://127.0.0.1:{port}/v1/messages");
        let prompt = if n % 2 == 0 {
            format!("جلسه فارسی {n}")
        } else {
            format!("中文会话 {n}")
        };
        tasks.push(tokio::spawn(async move {
            let response = client
                .post(url)
                .bearer_auth("test-gateway-token")
                .json(&json!({
                    "model":"claude-sonnet-5-5","stream":false,"max_tokens":128,
                    "messages":[{"role":"user","content":prompt}]
                }))
                .send()
                .await
                .unwrap();
            assert_eq!(response.status(), reqwest::StatusCode::OK);
            let value: Value = response.json().await.unwrap();
            assert_eq!(
                value["content"][0]["text"],
                format!("Response for {prompt}")
            );
        }));
    }
    for task in tasks {
        task.await.unwrap();
    }
    upstream.abort();
    server.abort();
    let _ = std::fs::remove_dir_all(dir);
}

async fn latest_turn_intent_mock(Json(payload): Json<Value>) -> Json<Value> {
    let messages = payload["messages"].as_array().expect("messages");
    assert_eq!(messages.len(), 3);
    assert_eq!(messages[0]["role"], "user");
    assert!(messages[0]["content"].as_str().unwrap().contains("GitHub"));
    assert_eq!(messages[1]["role"], "assistant");
    assert_eq!(messages[2]["role"], "user");
    // Even when the prior conversation named GitHub, a language-only followup
    // must not inject the local web bridge into the current provider request.
    assert!(payload.get("tool_choice").is_none());
    if let Some(tools) = payload.get("tools").and_then(Value::as_array) {
        assert_eq!(tools.len(), 1, "only explicit client tools remain");
        assert_eq!(tools[0]["function"]["name"], "Read");
    }
    let latest = messages[2]["content"].as_str().expect("latest user text");
    let marker = if latest.contains("FA-527") {
        "FA-527"
    } else {
        assert!(latest.contains("ZH-884"));
        "ZH-884"
    };
    Json(json!({
        "id": "no-unrelated-web-bridge",
        "choices": [{"finish_reason":"stop","message":{
            "role":"assistant","content": marker
        }}],
        "usage": {"prompt_tokens":40,"completion_tokens":10}
    }))
}

#[tokio::test]
async fn real_http_old_github_history_cannot_force_tools_into_language_only_turns() {
    let provider = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let provider_port = provider.local_addr().unwrap().port();
    let provider_task = tokio::spawn(async move {
        axum::serve(
            provider,
            Router::new().route("/v1/chat/completions", post(latest_turn_intent_mock)),
        )
        .await
        .unwrap();
    });
    let (state, dir) = test_server_state(provider_port, ContractMode::Enforce);
    state.config.write().await.web_access_mode = "required".into();
    let gateway = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = gateway.local_addr().unwrap().port();
    let gateway_task = tokio::spawn(async move {
        axum::serve(
            gateway,
            Router::new()
                .route("/v1/messages", post(anthropic_messages))
                .with_state(state),
        )
        .await
        .unwrap();
    });
    let url = format!("http://127.0.0.1:{port}/v1/messages");
    let client = reqwest::Client::new();
    for (text, marker, explicit_client_tool) in [
        (
            "فقط به فارسی پاسخ بده و عبارت FA-527 را در پایان بنویس.",
            "FA-527",
            false,
        ),
        ("请只用中文回答，并在结尾写 ZH-884", "ZH-884", true),
    ] {
        let mut body = json!({
            "model":"claude-sonnet-5-5","stream":false,"max_tokens":128,
            "messages":[
                {"role":"user","content":"Find a GitHub repo"},
                {"role":"assistant","content":"Which repo?"},
                {"role":"user","content":text}
            ]
        });
        if explicit_client_tool {
            body["tools"] = json!([{
                "name":"Read", "input_schema":{"type":"object"}
            }]);
        }
        let response = client
            .post(&url)
            .bearer_auth("test-gateway-token")
            .json(&body)
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), reqwest::StatusCode::OK);
        let value: Value = response.json().await.unwrap();
        assert_eq!(value["content"][0]["text"], marker);
    }
    gateway_task.abort();
    provider_task.abort();
    let _ = std::fs::remove_dir_all(dir);
}

async fn client_tool_priority_mock(Json(payload): Json<Value>) -> Json<Value> {
    let tools = payload["tools"]
        .as_array()
        .expect("client tool definitions");
    assert_eq!(tools.len(), 1, "no local bridge tools may be injected");
    assert_eq!(tools[0]["function"]["name"], "Read");
    assert!(payload.get("tool_choice").is_none(), "no forced web search");
    assert_eq!(payload["messages"].as_array().unwrap().len(), 1);
    assert_eq!(payload["messages"][0]["role"], "user");
    Json(json!({
        "id": "native-client-read",
        "choices": [{"finish_reason":"tool_calls","message":{
            "role":"assistant","content":null,
            "tool_calls":[{"id":"call-read-42","type":"function",
                "function":{"name":"Read","arguments":"{\"path\":\"README.md\"}"}}]
        }}],
        "usage": {"prompt_tokens":70,"completion_tokens":15}
    }))
}

#[tokio::test]
async fn real_http_client_read_is_not_hijacked_by_required_web_or_host_shell() {
    let provider = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let provider_port = provider.local_addr().unwrap().port();
    let upstream = tokio::spawn(async move {
        axum::serve(
            provider,
            Router::new().route("/v1/chat/completions", post(client_tool_priority_mock)),
        )
        .await
        .unwrap();
    });
    let (state, dir) = test_server_state(provider_port, ContractMode::Enforce);
    {
        let mut config = state.config.write().await;
        config.web_access_mode = "required".into();
        config.shell_access_mode = "auto".into();
    }
    let gateway = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = gateway.local_addr().unwrap().port();
    let server = tokio::spawn(async move {
        axum::serve(
            gateway,
            Router::new()
                .route("/v1/messages", post(anthropic_messages))
                .with_state(state),
        )
        .await
        .unwrap();
    });
    let reply = reqwest::Client::new()
        .post(format!("http://127.0.0.1:{port}/v1/messages"))
        .bearer_auth("test-gateway-token")
        .json(&json!({
            "model":"claude-sonnet-5-5", "stream":false, "max_tokens":256,
            "messages":[{"role":"user","content":"Read my local GitHub repo README"}],
            "tools":[{"name":"Read", "input_schema":{"type":"object"}}]
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(reply.status(), reqwest::StatusCode::OK);
    let result: Value = reply.json().await.unwrap();
    assert_eq!(result["stop_reason"], "tool_use");
    assert_eq!(result["content"][0]["name"], "Read");
    assert_eq!(result["content"][0]["id"], "call-read-42");
    server.abort();
    upstream.abort();
    let _ = std::fs::remove_dir_all(dir);
}

// This HTTP test proves payload fidelity, not the live Qwen model's language
// quality. It captures the exact translated request on a mock upstream.
async fn qwen38_greeting_capture(Json(payload): Json<Value>) -> Json<Value> {
    assert_eq!(payload["model"], "qwen38");
    assert_eq!(payload["chat_template_kwargs"]["enable_thinking"], false);
    assert!(payload.get("reasoning_effort").is_none());
    assert!(payload["chat_template_kwargs"].get("reasoning_effort").is_none());
    let messages = payload["messages"].as_array().expect("messages forwarded");
    assert_eq!(messages.len(), 2);
    assert_eq!(messages[0]["role"], "system");
    assert_eq!(messages[0]["content"], "You are a helpful assistant.");
    assert_eq!(messages[1]["role"], "user");
    assert_eq!(messages[1]["content"], "سلام");
    assert!(payload.get("tools").is_none());
    assert!(payload.get("tool_choice").is_none());
    Json(json!({
        "id":"mock-greeting",
        "model":"qwen38",
        "choices":[{"finish_reason":"stop", "message":{"role":"assistant", "content":"سلام!"}}],
        "usage":{"prompt_tokens":20,"completion_tokens":3}
    }))
}

#[tokio::test]
async fn real_http_qwen38_greeting_disables_thinking_without_injecting_web_context() {
    let provider = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let provider_port = provider.local_addr().unwrap().port();
    let upstream = tokio::spawn(async move {
        axum::serve(provider,
            Router::new().route("/v1/chat/completions", post(qwen38_greeting_capture)))
            .await.unwrap();
    });
    let (state, dir) = test_server_state(provider_port, ContractMode::Enforce);
    {
        let mut config = state.config.write().await;
        config.web_access_mode = "required".into();
        config.shell_access_mode = "disabled".into();
        config.routes[0].upstream_model = "qwen38".into();
        config.routes[0].thinking_mode = String::new(); // Old stored route.
    }
    let gateway = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = gateway.local_addr().unwrap().port();
    let server = tokio::spawn(async move {
        axum::serve(gateway,
            Router::new().route("/v1/messages", post(anthropic_messages)).with_state(state))
            .await.unwrap();
    });
    let reply = reqwest::Client::new()
        .post(format!("http://127.0.0.1:{port}/v1/messages"))
        .bearer_auth("test-gateway-token")
        .json(&json!({
            "model":"claude-sonnet-5-5",
            "stream":false,
            "max_tokens":256,
            "system":[{"type":"text","text":"You are a helpful assistant."}],
            "thinking":{"type":"adaptive"},
            "output_config":{"effort":"high"},
            "messages":[{"role":"user","content":"سلام"}]
        }))
        .send().await.unwrap();
    assert_eq!(reply.status(), reqwest::StatusCode::OK);
    let body: Value = reply.json().await.unwrap();
    assert_eq!(body["content"][0]["text"], "سلام!");
    server.abort();
    upstream.abort();
    let _ = std::fs::remove_dir_all(dir);
}

async fn provider_attempts_disabled_shell(Json(payload): Json<Value>) -> Json<Value> {
    let tools = payload["tools"].as_array().expect("web tools enabled");
    assert!(tools
        .iter()
        .any(|tool| tool["function"]["name"] == "claude_proxy_web_search"));
    assert!(!tools
        .iter()
        .any(|tool| tool["function"]["name"] == "claude_proxy_shell"));
    Json(json!({
        "id": "injected-local-shell",
        "choices": [{"finish_reason": "tool_calls", "message": {
            "role": "assistant",
            "content": null,
            "tool_calls": [{
                "id": "attempted-shell",
                "type": "function",
                "function": {"name": "terminal", "arguments": json!({"command": "echo must-not-run"}).to_string()}
            }]
        }}],
        "usage": {"prompt_tokens": 50, "completion_tokens": 12}
    }))
}

#[tokio::test]
async fn real_http_web_request_rejects_provider_shell_call_when_shell_disabled() {
    let provider = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let provider_port = provider.local_addr().unwrap().port();
    let upstream = tokio::spawn(async move {
        axum::serve(
            provider,
            Router::new().route(
                "/v1/chat/completions",
                post(provider_attempts_disabled_shell),
            ),
        )
        .await
        .unwrap();
    });
    let (state, dir) = test_server_state(provider_port, ContractMode::Enforce);
    {
        let mut config = state.config.write().await;
        config.web_access_mode = "required".into();
        config.shell_access_mode = "disabled".into();
    }
    let gateway = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = gateway.local_addr().unwrap().port();
    let server = tokio::spawn(async move {
        axum::serve(
            gateway,
            Router::new()
                .route("/v1/messages", post(anthropic_messages))
                .with_state(state),
        )
        .await
        .unwrap();
    });
    let response = reqwest::Client::new()
        .post(format!("http://127.0.0.1:{port}/v1/messages"))
        .bearer_auth("test-gateway-token")
        .json(&json!({
            "model": "claude-sonnet-5-5",
            "stream": false,
            "max_tokens": 256,
            "messages": [{"role": "user", "content": "Search for current news"}]
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), reqwest::StatusCode::BAD_GATEWAY);
    let body: Value = response.json().await.unwrap();
    assert!(body["error"]["message"]
        .as_str()
        .unwrap()
        .contains("disabled for this request"));
    server.abort();
    upstream.abort();
    let _ = std::fs::remove_dir_all(dir);
}

async fn provider_recovery_echo(Json(body): Json<Value>) -> (StatusCode, Json<Value>) {
    // Successful local mock represents upstream response *after* direct failover.
    (
        StatusCode::OK,
        Json(json!({
            "choices":[{"message":{"role":"assistant","content":body["marker"]},"finish_reason":"stop"}],
            "usage":{"prompt_tokens":1,"completion_tokens":2}
        })),
    )
}

#[tokio::test]
async fn broken_system_proxy_falls_back_to_direct_and_preserves_post_body() {
    let provider = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = provider.local_addr().unwrap().port();
    let server = tokio::spawn(async move {
        axum::serve(
            provider,
            Router::new().route("/v1/chat/completions", post(provider_recovery_echo)),
        )
        .await
        .unwrap();
    });
    let disconnected = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let bad_port = disconnected.local_addr().unwrap().port();
    drop(disconnected);

    let system = Client::builder()
        .proxy(Proxy::all(format!("http://127.0.0.1:{bad_port}")).unwrap())
        .build()
        .unwrap();
    let direct = build_direct_provider_client().unwrap();
    let dir = std::env::temp_dir().join(format!("claude-proxy-recovery-{}", uuid::Uuid::new_v4()));
    let diagnostics = ObservabilityStore::new(dir.join("usage.jsonl"), dir.join("diag.jsonl"));
    let response = send_provider_with_mode(
        system
            .post(format!("http://127.0.0.1:{port}/v1/chat/completions"))
            .json(&json!({"marker":"FA-527","stream":false})),
        &direct,
        &diagnostics,
        ProviderProxyMode::Auto,
    )
    .await
    .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body: Value = response.json().await.unwrap();
    assert_eq!(body["choices"][0]["message"]["content"], "FA-527");
    server.abort();
    let _ = std::fs::remove_dir_all(dir);
}

#[tokio::test]
async fn explicit_system_proxy_mode_never_bypasses_a_broken_proxy() {
    let disconnected = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let bad_port = disconnected.local_addr().unwrap().port();
    drop(disconnected);
    let system = Client::builder()
        .proxy(Proxy::all(format!("http://127.0.0.1:{bad_port}")).unwrap())
        .build()
        .unwrap();
    let direct = build_direct_provider_client().unwrap();
    let dir = std::env::temp_dir().join(format!(
        "claude-proxy-strict-system-{}",
        uuid::Uuid::new_v4()
    ));
    let diagnostics = ObservabilityStore::new(dir.join("usage.jsonl"), dir.join("diag.jsonl"));
    let result = send_provider_with_mode(
        system.get("http://127.0.0.1:18183/healthz"),
        &direct,
        &diagnostics,
        ProviderProxyMode::System,
    )
    .await;
    assert!(result.is_err());
    serverless_cleanup(&dir);
}

fn serverless_cleanup(dir: &std::path::Path) {
    let _ = std::fs::remove_dir_all(dir);
}

async fn provider_http_failure_mock() -> StatusCode {
    StatusCode::UNAUTHORIZED
}

#[tokio::test]
async fn provider_http_failure_is_not_replayed_as_direct_retry() {
    let provider = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = provider.local_addr().unwrap().port();
    let server = tokio::spawn(async move {
        axum::serve(
            provider,
            Router::new().route("/v1/models", get(provider_http_failure_mock)),
        )
        .await
        .unwrap();
    });
    let system = Client::new();
    let direct = build_direct_provider_client().unwrap();
    let dir = std::env::temp_dir().join(format!(
        "claude-proxy-no-http-retry-{}",
        uuid::Uuid::new_v4()
    ));
    let diagnostics = ObservabilityStore::new(dir.join("usage.jsonl"), dir.join("diag.jsonl"));
    let response = send_provider_with_mode(
        system.get(format!("http://127.0.0.1:{port}/v1/models")),
        &direct,
        &diagnostics,
        ProviderProxyMode::Auto,
    )
    .await
    .unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    server.abort();
    serverless_cleanup(&dir);
}

#[test]
fn provider_proxy_mode_defaults_direct_and_requires_explicit_system_opt_in() {
    assert_eq!(
        ProviderProxyMode::from_setting(""),
        ProviderProxyMode::Direct
    );
    assert_eq!(
        ProviderProxyMode::from_setting("bad"),
        ProviderProxyMode::Direct
    );
    assert_eq!(
        ProviderProxyMode::from_setting("auto"),
        ProviderProxyMode::Auto
    );
    assert_eq!(
        ProviderProxyMode::from_setting("SYSTEM"),
        ProviderProxyMode::System
    );
    assert_eq!(
        ProviderProxyMode::from_setting(" direct "),
        ProviderProxyMode::Direct
    );
}

#[tokio::test]
async fn provider_direct_mode_never_dials_a_configured_dead_system_proxy() {
    let upstream = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let upstream_port = upstream.local_addr().unwrap().port();
    let server = tokio::spawn(async move {
        axum::serve(
            upstream,
            Router::new().route("/v1/chat/completions", post(provider_recovery_echo)),
        )
        .await
        .unwrap();
    });
    let dead = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let dead_port = dead.local_addr().unwrap().port();
    drop(dead);
    let inherited_proxy_client = Client::builder()
        .proxy(Proxy::all(format!("http://127.0.0.1:{dead_port}")).unwrap())
        .build()
        .unwrap();
    let direct = build_direct_provider_client().unwrap();
    let dir = std::env::temp_dir().join(format!(
        "claude-proxy-direct-default-{}",
        uuid::Uuid::new_v4()
    ));
    let diag = ObservabilityStore::new(dir.join("usage.jsonl"), dir.join("diag.jsonl"));
    let response = send_provider_with_mode(
        inherited_proxy_client
            .post(format!(
                "http://127.0.0.1:{upstream_port}/v1/chat/completions"
            ))
            .json(&json!({"marker":"DIRECT-209"})),
        &direct,
        &diag,
        ProviderProxyMode::from_setting(""),
    )
    .await
    .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let result: Value = response.json().await.unwrap();
    assert_eq!(result["choices"][0]["message"]["content"], "DIRECT-209");
    server.abort();
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn isolated_probe_is_single_turn_and_has_no_client_or_web_tools() {
    let probe = isolated_provider_probe_payload(
        "mock-upstream",
        "فقط به فارسی پاسخ بده و عبارت FA-527 را در پایان بنویس.",
    );
    assert_eq!(probe["model"], "mock-upstream");
    assert_eq!(probe["stream"], false);
    assert_eq!(probe["max_tokens"], 512);
    let messages = probe["messages"].as_array().unwrap();
    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0]["role"], "user");
    assert!(messages[0]["content"].as_str().unwrap().contains("FA-527"));
    assert!(probe.get("tools").is_none());
    assert!(probe.get("tool_choice").is_none());
    assert!(probe.get("system").is_none());
    assert!(probe.get("metadata").is_none());
}
