# V2 Release-gate replays and real Windows acceptance

This PR is stacked on V2 output/caching PR #16, then integration PR #15,
then foundation PR #6. No part of it changes the installed app or main.

## CI coverage: mocked backend (does not prove Desktop compatibility)

Windows Rust loopback tests now exercise:
1. A provider SSE stream that sends finish_reason=stop but disconnects
   without terminal [DONE]. Under strict V2 mode this is an **error**, not
   a successful message_stop. An unreceived usage/error frame cannot be
   silently ignored.
2. Strict Cowork output-limit SSE: missing measured completion_tokens is
   an error, over-budget completion_tokens is an error; measured usage within
   limit succeeds without synthesizing a continuation.
3. Twelve concurrently submitted fresh requests with distinguishable
   Persian and Chinese contents. The loopback fake provider returns exactly
   the content of the corresponding request; cross-request mixing fails.
   This tests proxy routing, not the internal state of real Claude Desktop.
4. Existing Code simulated Read tool_use -> tool_result -> grounded answer,
   parallel tool calls, partial tool JSON, context and cache coverage.
5. TypeScript/Vite build, Rust format and all Rust tests.

## Local Windows smoke script (opt-in, safe dry-run)

scripts/verify-local-code.ps1 runs only against an HTTP loopback proxy and
does **not** access files or execute the model's proposed command. By default,
the script prints a test plan and exits without network requests.

Dry-run in PowerShell from a checkout that contains this branch:

    .\scripts\verify-local-code.ps1

To perform a *real* two-request provider round-trip, start the updated proxy,
select a verified tool-capable upstream model, and provide the local gateway
token via an environment variable in the same PowerShell session. Do not
paste the token into this chat or commit it to Git:

    $env:CLAUDE_PROXY_GATEWAY_TOKEN = Read-Host "Local gateway token"
    .\scripts\verify-local-code.ps1 -Run -GatewayUrl http://127.0.0.1:18181 -Model claude-sonnet-5-5

The script sends an artificial small Read tool-call request, verifies that the
provider returns a structured Read action with a stable ID and README.md path,
then supplies a **synthetic** README result with a unique validation marker.
It checks that the model includes the marker in its final answer. If the model
greets the user instead of calling a tool, the smoke test fails explicitly.

This optional -Run mode makes two real provider API calls, may incur model
usage charges and sends synthetic prompts to the configured provider. It
never sends a local file, executes shell, or enables guest VM host access.
It does not log request bodies or gateway tokens, and it never auto-executes
on CI. CI invokes only the no-network dry-run.

The gateway proxy must match this branch or later for all V2 guards. The
configured route, provider tool-calling support, correct port, gateway token,
and optional feature flags are the operator's responsibility. Do not assume
an already installed old binary contains this PR.

## Mandatory real Claude Desktop Code/Cowork acceptance

1. Note Windows version, app installer/build version, client Code/Cowork
   version, upstream provider and **exact upstream model**.
2. Run the local smoke script to establish the basic structured API path.
3. In actual Claude Desktop Code, open a disposable **read-only fixture**
   repository with two source files. Ask it to inspect the repository and
   identify a defect; verify via client trace that an actual Read invocation
   took place. Generic assistant introductions are a failure.
4. In Cowork, attempt a long continuation that approaches 32k output tokens.
   Verify the client does not abort or fabricate success after partial output.
5. Create fresh simultaneous Persian and Chinese sessions, checking that
   the app's *actual UI/history* does not conflate them.
6. Confirm guest VM tools do not gain implicit access to Windows CMD,
   arbitrary host paths, or Internet. Deny requested host approvals and
   verify they cannot execute.
7. Compare cache usage fields with provider-recorded usage. "Unreported"
   and "reported zero" must remain distinct.
8. Verify interruption/cancellation and rerun; there should be no orphaned
   host subprocess or unexpected tool invocation.
9. Document **PASS/FAIL** and concrete failing build/model pairs before
   releasing. CI green alone does not establish product readiness.

No automated GitHub release or merge happens as part of this increment.
