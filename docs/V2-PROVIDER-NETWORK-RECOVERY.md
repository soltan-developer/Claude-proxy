# Automatic provider connection recovery (V2)

## Background

A Windows installation can keep HTTPS_PROXY pointing at 127.0.0.1:10808 even after
v2rayN changes ports or stops listening. Starting a Portable EXE with double-click
inherits the broken Windows environment; clearing variables in a separate
PowerShell session does not repair a process already running.

The local proxy on port 18183 is healthy; only the outgoing provider
requests fail. Direct curl to ai.ismc.ir returned HTTP 401 without an API
key, proving that direct HTTPS connectivity is available from that host.

## New default behavior

Provider API Ping, model discovery, chat/completions POST and continuation
rounds all share one connection recovery policy.

CLAUDE_PROXY_PROVIDER_PROXY_MODE:
- auto (default): use the normal inherited system proxy. If and only if
  request connection establishment fails, retry once using an independent
  HTTP client with all proxies disabled.
- system: never bypass system proxy, including on connection failures.
- direct: always connect without a proxy; never use system proxy.

Only reqwest connection failures (is_connect) trigger retry. HTTP 401/403/429,
HTTP 5xx, transport failures after connection, response decoding failures
and arbitrary read timeouts are never retried. POST request cloning preserves
the original body/headers, and retries are attempted only before successful
connection establishment to avoid duplicating already-submitted work.
No credentials or prompts are logged; a diagnostic warning records the
bypass category. Direct connections can reveal the user's public IP to the
upstream provider, so operators requiring a VPN-only connection should
choose system mode.

This is a provider-only connection change; Claude Desktop and Claude Code
still use the configured local gateway port, and the Web tool network path
retains its separate policy.

## Windows acceptance

1. Quit all Claude Proxy instances from the tray and back up app settings.
2. Leave your old HTTPS_PROXY pointing at an *unreachable* local proxy.
3. Double-click the newly built Portable EXE from this branch. Do not clear
   environment variables manually. Confirm that Ping provider API and a short
   Claude Desktop answer both succeed.
4. Confirm that model discovery and long-response continuation work.
5. Confirm that the local gateway port remains the user's configured 18183.
6. In an explicit VPN-only environment set the mode to system before launch;
   confirm that an unavailable proxy remains a visible connection error.
7. Check Diagnostics for the network failover warning (no token or prompt).

Automated tests use an isolated fake provider and a deliberately dead proxy.
They cannot validate the real ai.ismc.ir certificate, actual v2rayN routing,
or installed Claude Desktop behavior.

This PR is draft, stacked on PR #18, not merged into main.
