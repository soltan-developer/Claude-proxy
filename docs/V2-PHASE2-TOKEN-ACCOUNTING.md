# V2 Phase 2 — Accurate token / cache foundations

This is a **separate stacked branch** on V2 foundation PR #6. Default
application behavior is unchanged; the module is compiled and unit tested
but has not yet been wired to persisted usage records or the dashboard.

## Fixed accounting semantics

- For OpenAI Chat Completions, `prompt_tokens` **already includes**
  `prompt_tokens_details.cached_tokens`; do not add cached reads a second time.
- For OpenAI Responses, `input_tokens` includes
  `input_tokens_details.cached_tokens`. The normalized values are distinct
  from Anthropic Messages API.
- For Anthropic Messages, `input_tokens` excludes separately reported
  `cache_read_input_tokens` and `cache_creation_input_tokens` where present.
- `cache_read_input_tokens: null` means **unreported**, NOT verified zero.
  Reporting no cache metadata does not justify claiming 100% uncached input.
  A real reported `cached_tokens: 0` remains numeric zero.
- `ContextShape` records only byte sizes of system, tool schemas, preceding
  messages and latest message. It does **not** pretend that JSON byte count is
  an exact model tokenizer result or a billing counter.

## Why Code appears to consume so many context tokens

A tiny human message can travel with long hidden system instructions,
tool definitions and prior agent-tool results. The context used for *one*
request is not the cumulative number of input tokens billed across requests.
Cache hits may reduce cost without removing the context from a request.

## Remaining phase-2 work

1. Wire normalized usage to observability and UI, preserving field provenance.
2. Record verified provider counts and explicit unknown/estimated flags.
3. Obtain exact token counts via provider-supported count-tokens APIs, when
   available; never use character estimates as verified token counts.
4. Build explicit, user-visible compaction that maintains tool use/result
   pairing, rather than silently dropping history.
5. Test actual 32k output ceilings against real Desktop/Code agent flows.
6. Gate rollout behind opt-in flags, keep backup and rollback.

## Quality
Windows `cargo fmt --all -- --check` and `cargo test --locked` must pass.
Real-provider token usage and cache semantics require later qualification.
