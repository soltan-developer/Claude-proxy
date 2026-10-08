# V2 Integration Acceptance — Stacked PR #15

This branch combines the independently tested feature contracts of PRs #12,
#13 and #14. It is **stacked on** foundation PR #6, NOT a stable release.

## What can be automated (Windows CI)

- 62 original V2 and legacy Rust tests.
- Capability declarations reject Code tool calls for a text-only provider.
- Valid parallel tool calls are accepted only with stable IDs and JSON arguments.
- Invalid provider tool-call IDs fail before the Claude client executes them.
- OpenAI Chat Completions and Responses cache reads are not counted twice.
- Cache metadata missing is **unknown**, not silently zero.
- Per-request system/tool/history byte attribution never logs prompt contents.
- Host approvals are exact-intent, short-lived, single-use; directory traversal
  and filesystem paths outside the approved workspace are rejected.
- Strict SSE recognizes stream truncation, malformed tool arguments and
  verified output-limit overruns without fabricated success.
- All experimental flags default to off in the running gateway.

## Manual Windows acceptance (release blocker)

Environment matrix: Windows 10/11 versions; installed Claude Desktop build
(MSIX vs legacy), Code/Cowork client version, upstream provider/model and
explicitly verified feature capability declaration. Save exact version IDs.

1. **Code agent reads repository**: Give it a disposable, read-only fixture
   project with README, two source files and an intentionally failing test.
   Ask it to inspect the project and identify concrete defects. Verify a real
   client `Read` call and tool_result. Generic greetings are a **failure**.
2. **Fresh session isolation**: Run simultaneous Chinese SDK research in
   session A and Persian greeting in B. Capture hashed request correlation,
   tool counts and sanitized preflight metadata. Never include raw prompts in
   logs unless separately approved. Confirm no cross-request mix-up.
3. **Output cap**: Send long-running 32,000 output-token boundary workloads.
   Verify no synthetic continuations that bypass client cap, and truthful
   `max_tokens` on truncated outputs.
4. **Context accounting**: Distinguish per-request current context from billed
   cumulative tokens; compare cache read/write values to the raw provider usage
   response. Unknown values must be labeled *unreported*.
5. **Cowork environment**: Confirm the real guest VM lacks host Rust tools if
   applicable; do not fabricate access to host CMD, files or Internet. Verify
   host bridge explicitly asks for permission and is unavailable when denied.
6. **Security**: Prompt-injection attempt to open an unrelated file, fetch
   localhost/private addresses, run a destructive shell command or replay an
   approval ID. Block and audit all forbidden operations.
7. **Cancellation / resilience**: Cancel multiple concurrent SSE streams,
   retry after rate limits, provider disconnect, and invalid JSON fragments.
   No running background command or orphaned tool execution is allowed.
8. **Install/rollback**: Upgrade and downgrade from last good release, with
   settings backed up and no DPAPI secret leakage.

Do not label the product generally compatible with Cowork/Code without
manual evidence and vendor/model capability verification.

## Runtime flags (off by default)

- `CLAUDE_PROXY_V2_CONTRACTS=observe|enforce`: protocol preflight.
- `CLAUDE_PROXY_V2_STREAM=on`: strict streaming when contracts are enforced.
- `CLAUDE_PROXY_V2_CAPABILITIES=observe|enforce`: explicit capability checks.
- `CLAUDE_PROXY_V2_MODEL_CAPABILITIES`: JSON keyed by upstream model ID.

**No flag currently makes the host-permission library a CMD/MCP tool server.**
Its approval lifecycle is still security infrastructure under development.

## Merge strategy

After this integration PR is green, keep PRs #12–14 available as evidence;
merge core PR #6 first only after its review, then land a reconciled
integration change into the target branch through a reviewed PR. Rebase/retarget
as necessary. No automatic force merges into `main`.
