# V2 experimental Desktop output limit and truthful cache usage

This branch is stacked on integration PR #15 (which depends on core PR #6).
Default application behavior remains unchanged.

## Optional 32,000 output-token safety contract

In Claude Desktop/Cowork, an assistant message exceeding a 32k output limit
may break a session even when the provider advertises higher output capacity.

Enable strictly for the proxy process, then restart:

    CLAUDE_PROXY_V2_CONTRACTS=enforce
    CLAUDE_PROXY_V2_STREAM=on
    CLAUDE_PROXY_V2_DESKTOP_OUTPUT_CAP=on

The proxy negotiates the minimum of the client request, 32k hard cap,
positive route output limit, and any lower budget calculated for context.
It reapplies this limit before every provider round, including local tool loops.

In nonstream mode, a provider response missing measured completion usage or
claiming more output than the bound fails before forwarding any tool call.
In stream mode, strict V2 SSE is mandatory and a missing or over-limit final
measured usage causes an error, never a fabricated message_stop success.
**Limitation:** text may already have streamed before final usage is received.
This cannot prove a true token-by-token bound without an upstream-compatible
tokenizer or provider-reported timely counts; a dishonest provider is out of
scope. Do not promise real Cowork compatibility on simulated CI alone.

## Honest cache read/write reporting

New usage records separately indicate whether the provider actually reported
cache *reads* and cache *writes*. Daily usage aggregates add two counters:
cacheReadReportedRequests and cacheWriteReportedRequests. Coverage below total
requests means that token sums are incomplete/unknown, not proven zero.

Old persisted records load with unknown new-field coverage; they are not
retroactively assumed to have reported zeros. This enables future dashboard
copy that distinguishes measured, estimated and unreported figures. Until that
UI work lands, do not display incomplete read/write totals as exact hit rates.

## Acceptance

- Provider output bound capped to 32000 even on a 64000 client request.
- Smaller model/request limits retained; overshoots and missing usage rejected.
- Strict streaming required for hard-cap mode.
- Cache read/write independently reported, older records read safely.
- Mock HTTP tests plus all old Rust tests green.
- Real Windows Code/Cowork and actual provider usage tests are release gates.
