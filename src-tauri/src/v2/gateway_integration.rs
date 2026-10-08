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
            upstream_cursor: Arc::new(AtomicUsize::new(0)),
            upstream_slots: Arc::new(Semaphore::new(2)),
            web_slots: Arc::new(Semaphore::new(2)),
            shell_slots: Arc::new(Semaphore::new(2)),
            observability: ObservabilityStore::new(dir.join("usage.jsonl"), dir.join("logs.jsonl")),
            active_requests: Arc::new(AtomicUsize::new(0)),
            v2_contract_mode: mode,
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
