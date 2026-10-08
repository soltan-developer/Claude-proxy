# V2 Phase 1 — Capability-aware provider contracts

This phase extends V2-03 while the current stable desktop gateway remains the
default. This PR is **stacked on PR #6** and should not merge before its base.

## Why this is needed

An endpoint claiming OpenAI Chat Completions support does **not** prove that
a model supports real client-owned `Read`, `Edit`, `Bash` calls, streamed
arguments, vision, parallel calls or reasoning controls. Without an explicit
contract Claude Desktop Code can fall back to a generic "I'm ready" response.

## Opt-in runtime configuration

Set the environment on the **proxy process**, then restart it. These are
not provider credentials and must not contain API tokens or prompts.

```powershell
$env:CLAUDE_PROXY_V2_CONTRACTS = "enforce"
$env:CLAUDE_PROXY_V2_CAPABILITIES = "enforce"
$env:CLAUDE_PROXY_V2_MODEL_CAPABILITIES = '{"your-exact-upstream-model":{"tools":true,"streaming":true,"vision":false,"reasoning":false,"parallel_tools":true,"max_output_tokens":8192}}'
```

`CLAUDE_PROXY_V2_CAPABILITIES` accepts `off` (default), `observe` (diagnostics
only), or `enforce` (reject unsafe/unsupported requests). An unknown model,
malformed JSON declaration, or omitted capability in enforce mode **fails
closed**. Models are keyed by their exact upstream ID, not the Desktop display
name. The configuration is an operator assertion, **not** automated proof. Use
only verified results from a real model/tool compatibility test, and never
advertise a model as officially compatible based only on this switch.

`CLAUDE_PROXY_V2_STREAM=on` is a *separate* opt-in for V2-03 strict SSE and
requires strict request contracts. Feature flags are independent on purpose.

## Fail-closed behavior

- V2 request conversion checks all declared client tools, old tool results,
  images, requested stream/reasoning/parallel modes and provider output limits.
- Unsupported features return an explicit 400, not a plausibly successful
  chat-only answer. Observe mode only logs the failure category.
- Under `CLAUDE_PROXY_V2_CONTRACTS=enforce`, a **non-stream** provider response
  must carry complete unique tool IDs/names and valid JSON argument objects.
  Incomplete or contradictory results return 502 before client execution.
- Nonstream *synthetic continuation* is disabled under strict request contracts
  so a provider `length` stop cannot silently become `stop`.

## Next gates

1. Windows CI `cargo fmt` and `cargo test` (including mock provider HTTP).
2. Controlled compatibility probe per provider/model on client `Read` ->
   tool_result -> repository-grounded response, with sanitized traces.
3. Native Anthropic adapter and OpenAI Responses API adapter in separate
   increments; current implementation still uses Chat Completions.
4. Actual Windows Claude Desktop Code and Cowork tests on pinned builds.
5. CI replay of rate limits, partial SSE, JSON schema and cancellation.

Do not change `main`, publish an installer, or represent the product as
ready for general users until these gates pass.
