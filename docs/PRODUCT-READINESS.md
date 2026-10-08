# Claude Proxy — Product Readiness & Compatibility Plan

> Status: Engineering plan; **not** a certification that Cowork/Code work end-to-end.
> Baseline audited: v2.0.8 on `main` (2026-10-08). Review this document at every release.

## Product definition

Claude Proxy is a Windows **local inference gateway** and configuration helper. It
translates Anthropic Messages API requests from supported Claude clients into
OpenAI-compatible Chat Completions. It does **not** reproduce Claude Desktop's
agent runtime, sandbox, account services, connector permissions, or every model
capability. Compatibility is a property of a **specific Claude Desktop build +
proxy version + upstream provider/model**, not just an API label.

Do not advertise any provider/model as "full Cowork support" until the
corresponding end-to-end tests below pass on the actual Windows desktop build.

## Current confirmed implementation facts

- Tauri 2 + React UI; Rust/Axum listener on `127.0.0.1`.
- Message and model routes: `/v1/messages`, `/v1/models`, with `/claude-desktop` prefix aliases.
- Upstream uses `/v1/chat/completions` and converts messages and streaming SSE.
- Credentials protected using Windows DPAPI; client-side gateway token appears
  in the client config as needed by the client.
- Local optional web and shell tools, usage records and diagnostic events.
- Claude Desktop third-party-inference profile and Claude Code CLI settings are
  written to different locations; changes may require complete client restart.
- A Windows portable build workflow exists. Release signing, updates, and a
  complete multi-provider test matrix are not established in this audit.
- `bridge_v2.rs` contains ~5.7k lines; isolate protocol, providers, client
  integrations, and security-sensitive tools behind testable modules.

## P0 — Block general availability (GA) until complete

1. **Desktop 3P deployment contract:** verify config/profile schema and actual
   file discovery on each supported Claude Desktop build; test clean install,
   upgrade, profile switching, refusal of malformed config, and rollback.
2. **Separate Code surfaces:** test Claude Code in **Desktop** and standalone
   `claude` CLI independently. CLI `settings.json` is not proof that the
   Desktop Code tab reads the same settings.
3. **Protocol compliance:** record sanitized fixtures for non-stream and
   streaming text, tool calls (parallel, partial arguments, mixed content),
   `tool_result`, images, system messages, reasoning, JSON outputs, cancellation,
   provider errors, rate limits, and malformed upstream data. Preserve client
   tool ownership and correct Anthropic SSE event ordering. Test with mock
   upstream before every release.
4. **Provider qualification:** maintain per-provider/model capability flags:
   tool calling, streamed tool arguments, vision, reasoning, structured outputs,
   token usage, max input/output, and retry behavior. Gate advanced features
   instead of silently pretending a model supports them.
5. **Agent safety:** disabled-by-default local shell; when enabled require
   visible consent before commands, restrict working directories, audit without
   logging secrets, reject untrusted prompt instructions, and minimize
   privilege. A natural-language tool description is not a security control.
6. **No silent task corruption:** define a safe context-overflow policy.
   Dropping old turns may also drop facts or tool results an agent needs.
   Make trimming visible and test tool-call/result pairing after trimming.
7. **Release gates:** Rust formatting/tests, frontend type-check/build,
   end-to-end Windows/Claude Desktop smoke tests, security review, and rollback
   rehearsal must pass before a release is promoted.
8. **Client version matrix:** keep at least one pinned, actually installed
   Windows Claude Desktop version in CI/manual QA; alert on upstream breaking
   changes and mark unverified versions as experimental.

## P1 — Reliability & maintainability

- Split the Rust monolith into `protocol/anthropic`, `protocol/openai`,
  `providers`, `clients/desktop`, `clients/cli`, `tools`, `security`
  and `observability` while keeping fixtures stable.
- Add request IDs, explicit timeouts, cancellation propagation,
  bounded concurrency, exponential backoff with jitter, and
  retry-only-if-safe rules; never auto-retry arbitrary side-effecting tools.
- Adopt provider adapters: prefer native Anthropic Messages when available;
  otherwise translate through provider-specific APIs. Chat Completions is a
  lowest-common-denominator mode, not a feature-complete guarantee.
- Add Diagnostics -> `Run compatibility check` for (a) local listener and
  gateway auth, (b) provider auth/models, (c) streamed reply, (d) tool call,
  (e) Desktop profile detected, (f) CLI settings detected. Never test by
  executing a shell command without explicit approval.
- Add deterministic integration tests using a mock provider that can inject
  401/403/404/429/500, malformed JSON, slow SSE and connection drops.
- Test crash/restart, port conflict, sleeping laptop, proxy reassignment,
  preserved settings, backup and restore, RTL layout and WebView2 missing.

## P2 — Commercial product

- Trademark/branding review: position as an independent gateway for supported
  Claude clients, not an official Anthropic desktop replacement.
- Clarify commercialization rights: `LICENSE.txt` grants third parties only
  non-commercial use; confirm the seller controls the underlying source rights,
  and audit third-party dependency and provider terms.
- Provide a signed Windows installer, reproducible tagged builds, SHA-256
  checksums, secure update channel, offline/uninstall/restore behavior and
  Windows SmartScreen guidance. Portable remains an optional delivery format.
- BYOK first: providers' keys stay on device; do not pool/redistribute third
  party subscriptions without permission. Later: optional commercial licensing
  tied to the product, not hidden changes to the provider's access controls.
- Publish a compatibility matrix, clear limits, changelog, support SLAs,
  incident process, privacy notice and refund terms before selling.
- Add opt-in crash telemetry with redaction, never collect prompts or keys
  by default; keep diagnostic exports sanitized.

## Acceptance matrix (run against actual client & upstream)

| Scenario | Desktop Cowork | Desktop Code | Claude Code CLI |
|---|---|---|---|
| Startup / restart / profile selection | Required | Required | Required |
| Text streaming, SSE and cancellation | Required | Required | Required |
| Client-side multi-tool loop | Required | Required | Required |
| Large task / context overflow | Required | Required | Required |
| Provider error / rate limit / timeout | Required | Required | Required |
| Multi-session model isolation | Required | Required | Required |
| File task creates correct output | Required | N/A | Required |
| Repo inspect → edit → tests → diff | N/A | Required | Required |
| Upgrade and rollback without lost settings | Required | Required | Required |

For each row capture: Windows version, Desktop/CLI version, proxy commit SHA,
provider API implementation, model ID, pass/fail, sanitized logs, and a replay
fixture. Any failed P0 row blocks a "fully supported" claim for that combination.

## First engineering increment

- Fix Claude Code CLI default model being removed during settings generation.
- Send SSE heartbeat for both local shell and web tool workflows.
- Add regression tests for these cases.
- Make Rust tests and TypeScript build required on PRs, while keeping portable
  executable packaging on `main` / manual workflow dispatch.

**Exit condition:** CI green **and** at least one real Windows Claude Desktop
Cowork and Code smoke test on a named compatible model. CI green alone does not
prove upstream Desktop integration.
