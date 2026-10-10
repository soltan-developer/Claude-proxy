"use strict";
// Installed explicitly via Claude Desktop Extensions, never by the proxy app.
// This stdio shim can contact ONLY the loopback /mcp endpoint.
const readline = require("node:readline");
const MAX_RPC_BYTES = 256 * 1024;
const REQUEST_TIMEOUT_MS = 30000;
const METHODS = new Set(["initialize", "ping", "tools/list", "tools/call"]);
const TOOLS = new Set(["claude_proxy_web_search", "claude_proxy_web_fetch"]);
const error = (id, code, message) => ({ jsonrpc: "2.0", id, error: { code, message } });

function getConfig(env = process.env) {
  const portText = String(env.CLAUDE_PROXY_PORT ?? "18181");
  const port = Number(portText);
  if (!/^\\d{1,5}$/.test(portText) || !Number.isInteger(port) || port < 1 || port > 65535) {
    throw new Error("Invalid Claude Proxy loopback port.");
  }
  const token = env.CLAUDE_PROXY_GATEWAY_TOKEN;
  if (typeof token !== "string" || token.length < 1 || token.length > 4096 || /[\\r\\n]/.test(token)) {
    throw new Error("Configure the local Claude Proxy MCP token in this extension's settings.");
  }
  return { port, token };
}

async function relay(request, config, fetchFn = globalThis.fetch) {
  const id = request && (typeof request.id === "string" || typeof request.id === "number") ? request.id : null;
  if (!request || typeof request !== "object" || Array.isArray(request) || request.jsonrpc !== "2.0") {
    return error(id, -32600, "Invalid JSON-RPC request.");
  }
  if (request.method === "notifications/cancelled") return null;
  if (request.method === "notifications/initialized") {
    // The loopback server acknowledges this notification with HTTP 202.
    if (id !== null) return error(id, -32600, "A notification cannot have an id.");
  } else if (id === null || !METHODS.has(request.method)) {
    return error(id, -32601, "Unsupported MCP method.");
  }
  if (request.method === "tools/call" && (!TOOLS.has(request.params?.name) ||
      (request.params?.arguments !== undefined && (typeof request.params.arguments !== "object" ||
       request.params.arguments === null || Array.isArray(request.params.arguments))))) {
    return error(id, -32602, "Only Claude Proxy's two read-only web tools are available.");
  }
  const body = JSON.stringify(request);
  if (Buffer.byteLength(body, "utf8") > MAX_RPC_BYTES) return error(id, -32600, "MCP request too large.");
  const url = `http://127.0.0.1:${config.port}/mcp`;
  try {
    const response = await fetchFn(url, {
      method: "POST",
      headers: {
        "content-type": "application/json",
        "accept": "application/json, text/event-stream",
        "mcp-protocol-version": "2025-11-25",
        "authorization": `Bearer ${config.token}`
      },
      body,
      signal: AbortSignal.timeout(REQUEST_TIMEOUT_MS)
    });
    if (request.method === "notifications/initialized" && response.status === 202) return null;
    if (!response.ok) {
      return error(id, -32000,
        `Local Claude Proxy returned HTTP ${response.status}. Check that the app is running, the token matches and Web access is enabled.`);
    }
    if (!String(response.headers.get("content-type") || "").toLowerCase().includes("application/json")) {
      return error(id, -32000, "Local MCP returned an unsupported content type.");
    }
    const resultText = await response.text();
    if (Buffer.byteLength(resultText, "utf8") > MAX_RPC_BYTES) {
      return error(id, -32000, "Local MCP response exceeds the safe size limit.");
    }
    const result = JSON.parse(resultText);
    if (!result || result.jsonrpc !== "2.0" || result.id !== id) {
      return error(id, -32000, "Local MCP returned an invalid JSON-RPC response.");
    }
    return result;
  } catch {
    return error(id, -32000,
      "Local Claude Proxy MCP is unavailable. Start Claude Proxy and check the loopback port; this extension cannot change organization policies.");
  }
}

function runStdio() {
  const input = readline.createInterface({ input: process.stdin, crlfDelay: Infinity });
  input.on("line", (line) => {
    void (async () => {
      let request;
      try {
        if (Buffer.byteLength(line, "utf8") > MAX_RPC_BYTES) throw new Error("too large");
        request = JSON.parse(line);
      } catch {
        process.stdout.write(JSON.stringify(error(null, -32700, "Invalid or oversized JSON input.")) + "\n");
        return;
      }
      let reply;
      try { reply = await relay(request, getConfig()); }
      catch { reply = error(request?.id ?? null, -32000, "Local MCP configuration missing or invalid."); }
      if (reply !== null) process.stdout.write(JSON.stringify(reply) + "\n");
    })();
  });
}
if (require.main === module) runStdio();
module.exports = { getConfig, relay };
