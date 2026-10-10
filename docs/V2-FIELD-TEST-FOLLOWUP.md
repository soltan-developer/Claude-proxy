# V2 Code/Cowork field feedback — 2026-10-10

## Observed vs proven

User testing on Windows, using an actual running Claude Proxy preview and
ai.ismc.ir upstream:

- API connectivity succeeded after clearing a stale loopback proxy setting.
- A short Persian instruction FA-527 opened an unrelated GitHub questionnaire.
- Claude Code/Cowork executed Bash commands in some turns, but final model
  answers were frequently generic or did not reflect execution output.
- A successful working directory listing reported /d/Work/Software/Claude-proxy.
  A later tool attempted D:/Work/Software/Claude/proxy; the latter is a
  DIFFERENT path. "No such file" is not evidence of Windows ACL denial.
- A Cowork curl to the GitHub API exited with code 56; exact root cause
  (guest network/proxy/TLS/server) remains unverified.
- A Chinese-only instruction returned Chinese and ZH-884, but referenced a
  prior web search. This does NOT by itself prove cross-session message leakage.

These results do not meet real Code/Cowork release acceptance. Mock HTTP CI
remains necessary but insufficient.

## Found and fixed in this PR

The local web bridge previously used a historical GitHub mention as a
reason to enable web tools during a new greeting/language-only turn.
When web_access_mode=required, automatic mode could also force a web call.
A past GitHub turn must never authorize irrelevant web browsing on later
unrelated messages.

The current user turn now determines web eligibility. Explicit revisit,
search, opinion or fresh-information requests involving the old repo retain
their former behavior. Plain سلام, Persian format-only instructions and
Chinese format-only instructions with an old GitHub turn have no injected
web tools even in required mode.

HTTP regression tests check actual translated requests to an upstream
fake provider, including retention of explicit client Read tools.

Critical second path: in strict V2, when Claude Code/Cowork provides its own
tool definitions, the proxy must NOT inject a competing local web/shell tool
or force tool_choice=claude_proxy_web_search, even when web mode is required
and the user asks about a GitHub repository. Client-owned Read/Bash tools
retain their identities and receive normal tool_use responses. The legacy
bridge behavior remains unchanged for clients without declared tools. V2
diagnostics record web routing decision and counts but NO text, tool arguments,
file paths or secrets.

## Operator acceptance still required

1. In a NEW Cowork conversation (not the earlier thread) send
   "فقط به فارسی پاسخ بده و عبارت FA-527 را در پایان بنویس."
   Verify: Persian, exact marker, NO GitHub search/questionnaire.
2. In another NEW conversation send
   "请只用中文回答，并在结尾写 ZH-884".
   Verify the marker and no reference to earlier unrelated web tasks.
3. In Claude Code, explicitly open D:\Work\Software\Claude-proxy, or use a
   disposable fixture repository, and ask to run pwd before reading
   README.md. Confirm the exact path from the tool output. Do not use
   guessed D:\Work\Software\Claude\proxy.
4. Confirm the client actually invokes Read or Bash and receives contents
   via its tool_result, not just a fluent model claim.
5. If a response is generic/unrelated, export REDACTED Diagnostics.
   Compare V2 request shape (messages, tool_uses, tool_results) and
   web_intent to distinguish client conversation history from bridge tool
   injection. Never paste raw API keys or entire private prompts.
6. Diagnose Cowork guest network failures inside its actual VM separately.
   A Windows host-side API ping does not establish Cowork guest Internet.
7. Test individual provider/model tool-calling fidelity. Model name alone
   does not guarantee Anthropic tool semantics when proxied via Chat
   Completions.

No user filesystem permissions are changed. This PR must remain Draft
until the Windows Code/Cowork tests and security review pass; no merge to main.
