# V2 gateway contracts: opt-in activation & integration proof

The legacy gateway remains the default. Changes in PR #6 add a server-side
flag that is read at proxy startup from the *proxy process* environment:

| \`CLAUDE_PROXY_V2_CONTRACTS\` | Behavior |
|---|---|
| unset (default), \`off\` | Legacy behavior; no v2 validation or logging |
| \`shadow\` / \`observe\` | Validate the parsed Anthropic request and record numeric shape + compatibility result; do **not** block it |
| \`enforce\` | Same checks; reject unsupported content with a safe 400 error instead of silently dropping it |

To set this experimentally on Windows PowerShell *before launching the proxy*:
\`\`\`powershell
$env:CLAUDE_PROXY_V2_CONTRACTS = "shadow"
# Launch Claude Proxy from this same process/environment.
\`\`\`

Restart the proxy application to change the mode. Do not assume an environment
variable set in a different terminal applies to an already-running app.

**Important:** Enforce validates only the request shape against the existing
Chat Completions adapter. It does not certify the provider's tool-calling quality,
fix VM limitations, or turn the legacy translator into a native Anthropic adapter.
The error is intentionally generic and never echoes prompt content, tool data,
or secrets. Diagnostics contain counts and sizes rather than message text.

## Integration test proof vs real-client proof

The Rust integration fixture runs two loopback HTTP servers: a mock upstream
\`/v1/chat/completions\` and an Axum gateway \`/v1/messages\`. It simulates
the complete **wire-level** exchange:

1. Client sends a real HTTP request with a \`Read\` tool and project-audit prompt.
2. Proxy authorizes, validates, maps the model and forwards converted messages.
3. Fake provider replies with a structured \`Read({"path":"README.md"})\` tool call.
4. Proxy returns an Anthropic \`tool_use\` block with the same call ID/name/input.
5. Simulated client supplies a deterministic \`tool_result\`.
6. Proxy forwards it to the fake provider and returns a grounded final response.
7. A document that would be dropped by Chat Completions is rejected with 400.

This proves the HTTP bridge and client-tool round trip **with a mock provider**,
not that Claude Desktop's Code panel has access to a real workspace. Actual
Windows Claude Desktop + Cowork/Code compatibility remains a separate release
gate. The test requires no subscription, live internet or user files.

## Next steps

- Port the v2 streaming tool-call accumulator into the live SSE pipeline,
  preserving event order and client-side tool ownership.
- Expand mock fixtures for SSE fragmentation, parallel tools, client cancellation,
  429/500 retries, long context and the 32k client output cap.
- Support native Anthropic as an adapter and typed usage provenance without
  silently inferring missing cache metadata.
- Add client app preflight diagnostics for actual Cowork and Code contexts;
  never bypass guest VM isolation or user consent.
