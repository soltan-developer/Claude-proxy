# Claude Proxy V2 — Architecture Decision & Executable Delivery Plan
Version: 0.1 (2026-10-08). Status: implementation commenced; NOT release-certified.

## Decision
**Rewrite the core, keep the proven shell/UI.** Do not rewrite the whole Tauri
Windows app, backup logic, themes, and installer. Make a new modular Rust gateway
beside the legacy `bridge_v2.rs`, migrate features only behind explicit flags
after mock-provider tests and actual Desktop Cowork/Code validation. Preserve old
settings with schema migration, backup and rollback. NO broad GA launch yet.

**Product promise**: a BYOK, local, privacy-preserving and honest compatibility
gateway for *supported* versions of Claude Desktop/Cowork/Code and model providers.
It cannot replace Anthropic's agent runtime, override Cowork's VM security policy,
or make models lacking reliable tool calling function as autonomous coders.

## Confirmed baseline risks (source review)
- `src-tauri/src/bridge_v2.rs` is a ~5.7k-line monolith doing HTTP transport,
  all provider conversion, local command execution, token estimates, config and
  streaming. Changes routinely entangle unrelated functionality.
- The legacy formatter estimates tokens using ASCII chars / 3 and non-ASCII
  chars / 1. This is **not** a tokenizer and must not be the authoritative
  context-size / cache / billing basis.
- `anthropic_to_openai` filters unsupported tool types; `assistant_content`,
  `user_content`, and `anthropic_text` silently ignore unknown content blocks,
  potentially changing Code/Cowork agent instructions or outputs.
- `trim_oldest_turn` deletes serialized messages between user turns, without
  proving semantic independence of tool calls/results or preservation of required
  repository context.
- Auto-continuation synthesizes a user Continue prompt, appends partial assistant
  output and may rewrite `finish_reason=length` into `stop`. Character-count
  budgeting may violate the Desktop's output ceiling. Disable by default in v2;
  implement explicit client-approved continuation only after exact token accounting.
- `AnthropicStreamState` assumes the first chunk for every tool index has the
  final tool id and name, and uses fallback `tool_call` / `tool` if absent.
  Some OpenAI-compatible providers split tool metadata across chunks. A client
  tool call with the wrong identity can fail even though the text streams.
- Browser/agent source context is large even when user text is tiny. A reported
  25% of 128k context does not prove 32k **new** tokens were consumed; trace
  upstream request composition and caching separately from billed usage.
- The application cannot directly run commands from within Cowork's VM merely
  because the gateway is on Windows. Its built-in optional shell is a separate
  proxy-origin tool, and must not bypass Claude's tool permission system.
- Authored logs never include prompts by design; add opt-in request **shape**
  metrics to explain payload size, tool count and context history without secrets.
- Provider cache accounting currently mixes real provider usage and heuristic
  fallback. Never silently display missing cache metadata as a verified 0.
- The Desk/CLI client profiles use distinct config paths; verify active profile,
  managed Windows registry policy and the actual Desktop build independently.
- Client limitation "response exceeded 32000 output tokens" must be dealt with at
  the protocol boundary, not by blindly raising a process environment variable.

## Symptom -> reproduction -> proof
1. **Code ignores project audit and replies "I'm ready"**: on a fixture repository
   with read-only permissions, verify actual requested tools reach the provider,
   provider returns tool calls with stable names/IDs, Desktop performs tool calls,
   results are translated losslessly, and the assistant cites files actually read.
   Flag unsupported tool-calling upstream instead of hallucinating success.
2. **Fresh Cowork greeting returns old Chinese SDK answer**: run two concurrent
   isolated sessions, one Chinese research and one Persian "سلام". Use request
   IDs, per-session correlators, counts and HMACed (ephemeral locally stored)
   message digests, never payload contents. Verify client input and provider
   response boundaries, not just the visible UI. Diagnose provider/session mixing
   without asserting memory leakage on insufficient evidence.
3. **32000-token crash**: inject model streams with provider `length`,
   client output cap 32000, 40000 tokens emitted, slow partial chunks and
   tool_use. Never produce > client cap, never silently change `max_tokens`
   to `end_turn`, and never auto-append synthetic `Continue` on Code tasks.
4. **Context appears 25% full immediately**: itemize system, tool schemas,
   message history, images/documents, repo context, client reserves, provider
   measured input and cached input, and UI estimates. Compare raw request shape
   to provider usage for each turn.
5. **No Windows cargo / no internet inside Cowork**: distinguish VM and Windows
   host. Expose approved host capabilities via standard client-approved MCP,
   or user-granted folders. If a cloud session cannot reach the host bridge or
   policy denies network, show a precise actionable limitation. Never pretend a
   local gateway changes Cowork's sandbox or network egress rules.
6. **Cache totals wrong**: test OpenAI Chat Completions cached_tokens subset of
   prompt_tokens and Anthropic Messages additive input/cache counts. Preserve
   `unknown` vs actual reported 0 and per-provider provenance.

## v2 target architecture
```text
Claude Desktop [Chat / Cowork / Code] | Claude Code CLI
                   |
       Anthropic Messages edge (Axum, 127.0.0.1, local auth)
                   |
         Client capability + session-aware trace
                   |
     Typed request IR and lossless content model
     /          |                 \
Native Anthropic  OpenAI Responses  Chat Completions (partial)
Adapter         Adapter            Adapter w/ capability checks
     \          |                 /
          Provider capability registry + model router
                   |
         SSE state machine, bounded output and cancellation
                   |
   Usage ledger (provider actual vs estimated) + diagnostics
                   |
   Optional approved MCP host tool server, NOT hidden shell
```

Modules: `v2/protocol` (typed content + SSE), `v2/adapters` (native,
responses, chat-completions), `v2/routing`, `v2/session`,
`v2/output_budget`, `v2/usage`, `v2/request_shape`, `v2/tools`,
`v2/security`, `v2/diagnostics`, and `v2/tests/fixtures`.
Use versioned settings with atomic migration, backup and restoration.
Old runtime remains untouched until a feature is gated on.

### Strict v2 invariants
- Unknown block types are preserved natively OR refused with a precise
  `unsupported_feature` error; never silently dropped.
- Client tool calls always go back to the client, not executed locally by the
  proxy; proxy-owned tools have a unique namespace and require permissions.
- Tool IDs and partial arguments remain stable across SSE chunk boundaries.
- Request cancellation propagates to provider and ends background tool work.
- No synthetic completion that silently rewrites output-limit to success.
- Max output < = client's and provider's accepted limits, based on measured
  counters. Refuse unknowable limits rather than guessing safety.
- No implicit cross-session memory. No request body, token or secret appears in
  normal logs, exports or crash telemetry. Explicit diagnostic opt-in for raw
  traffic must be off by default and time-limited.
- Cache/usage from unsupported providers displayed as `unknown` or explicitly
  `estimated` — never as a fabricated exact number.
- Internet and Windows terminal only through user-approved, audited,
  scoped capabilities. Independent Cowork VM/network policy is respected.

## Twelve delivery increments (gate before next step)
| Stage | Engineering increment | Exit tests / evidence |
|---|---|---|
| V2-00 | Freeze known-good 2.0.8; tag config migration baseline | Backup + rollback succeeds |
| V2-01 | Build typed v2 diagnostic, token/budget contracts | Rust unit tests, privacy-safe shape snapshots |
| V2-02 | Mock Anthropic API edge with deterministic replay | 400/401/429/500, malformed JSON, cancellation |
| V2-03 | Native Anthropic adapter for eligible providers | lossless block and SSE round-trip |
| V2-04 | Chat Completions strict adapter; optional Responses adapter | tool streaming fragments, image, JSON, reasoning |
| V2-05 | Provider capability probing and no-fake-tool-agent guard | full/safe-limited/unsupported badges with proof |
| V2-06 | Context budget with measurement, explicit compaction | no silent context loss or orphan tools |
| V2-07 | Client output limits, SSE, backpressure, retry policy | 32k regression, stalled streams, 10 parallel turns |
| V2-08 | Cowork VM diagnostics + approved Windows host MCP bridge | VM versus host clearly disclosed; consent tests |
| V2-09 | Usage/caching accurate billing ledger and charts | compare provider totals and cache provenance |
| V2-10 | UX: health doctor, profiles, secure updates & crash recovery | clean Win10/11 install, offline/online rollback |
| V2-11 | Security, compatibility lab, beta and commercial release | signed builds, acceptance matrix, zero P0 defects |

Workstreams can overlap in parallel under the same contracts. Do NOT merge a
partly working feature just to advance a progress percentage.

## Team roles / review
- Product owner: actual Cowork/Code user jobs and promised compatibility.
- Protocol engineer: SSE, tool sequencing, Anthropic/OpenAI API semantics.
- Rust/desktop engineer: Axum, Tauri, Windows, settings, MSIX, process lifecycle.
- Agent runtime engineer: tools, filesystem boundaries, permission UI, MCP.
- Applied ML/tokenization engineer: capability and context/prompt composition.
- Security/red-team engineer: shell abuse, prompt injection, SSRF, secret storage.
- SRE: failover, status metrics, crash recovery, update and rollback.
- QA/release engineer: deterministic fixtures + real Windows Desktop smoke test.
- Licensing/legal reviewer: source license, branding and provider terms.

## Launch criteria
- A fully specified model + client/build + OS + provider capability matrix.
- Run 100 audited end-to-end tasks per supported combination with >=95% task
  completion, 0 cross-session data contamination, 0 P0 errors; investigate
  all failures rather than using success rate as permission to ignore them.
- No user-visible "success" when tool wasn't run or output was silently clipped.
- 32k output cap and cache accounting tests pass across provider variants.
- Agent actions require explicit user scope and destructive approval.
- Signed Windows installer; rollback, support channel, privacy notice, change log.

## Scope of initial v2 branch
- Add independently compiled `src-tauri/src/v2` core contracts and tests.
- Add zero-content request diagnostics design.
- CI tests and this masterplan.
- No breaking changes to shipping default routes until contracts are validated.
