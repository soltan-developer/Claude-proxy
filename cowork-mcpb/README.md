# Claude Proxy — optional local Cowork MCP Bundle (v2.0.14)

This is a **local, read-only MCPB/Desktop Extension** for approved *local* Cowork sessions on Windows. It relays only `initialize`, `ping`, `tools/list` and the two existing `claude_proxy_web_search`/`claude_proxy_web_fetch` tools to `http://127.0.0.1:<port>/mcp`. It never executes a shell command, does not install itself, and cannot change Cowork's native network policy.

## Requirements and permission boundary
- Claude Proxy v2.0.14 must be running with **Web access explicitly enabled**.
- Only use when the account/device owner or organization **authorizes local MCP tools and the requested destinations**. An organization-denied website must remain denied; do not use this bundle as an alternate route around a denylist.
- Local MCP extensions work only in compatible Claude Desktop/locally supported Cowork contexts. **Cowork cloud sessions do not execute local MCP servers.** Enterprise/MDM can disable local MCP/desktop extensions.
- Claude Code native web support or its local HTTP MCP registration does **not** imply that Cowork's own web search/fetch is enabled.
- Remote MCP connectors execute from Anthropic's cloud, cannot reach your 127.0.0.1 gateway, and should not be pointed at a public unauthenticated tunnel.

## Explicit opt-in installation
1. Verify with your administrator, when applicable, that local MCP and the required domain access are permitted.
2. Download the **private CI artifact** `Claude-Proxy-v2.0.14-Cowork-Web.mcpb` after all tests pass.
3. In Claude Desktop, open Settings > Extensions and install the bundle **only with authorization**.
4. Provide the local port shown in Claude Proxy and the sensitive local MCP Bearer token from your own Claude Code `mcpServers.claude-proxy-web.headers.Authorization` entry (remove the literal `Bearer ` prefix). To create that entry first, click the existing **Connect web tools to Claude Code** button in Claude Proxy. Never share or paste the token into chat or diagnostics.
5. Test within a **new compatible local Cowork session**. Confirm that the tool shown is `claude_proxy_web_fetch`, not native Cowork web fetch, and that it is an authorized destination. A blocked native web URL is **not** an invitation to retry via MCP.
6. Remove or disable the extension from Claude Desktop at any time; all previous Claude Code/Proxy configuration is retained.

## Expected failure messages
- `HTTP 401`: configured MCP token does not match.
- `HTTP 403`: Web access disabled or the localhost MCP security policy refused the request.
- `unavailable`: Proxy stopped, wrong port, or localhost routing unavailable.
- No tools visible in Cowork: local extension unsupported/disabled, or the session is cloud-hosted.
- Native Cowork web denied: client/organization restriction is **separate** from MCP. Request authorization or use a permitted source; never tunnel around the restriction.

## Sources
- https://support.claude.com/en/articles/14479288-claude-cowork-architecture-overview
- https://support.claude.com/en/articles/11725091-when-to-use-desktop-and-web-connectors
- https://support.claude.com/en/articles/10949351-getting-started-with-local-mcp-servers-on-claude-desktop
- https://github.com/modelcontextprotocol/mcpb/blob/main/MANIFEST.md
