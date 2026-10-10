"use strict";
const test = require("node:test");
const assert = require("node:assert/strict");
const { getConfig, relay } = require("./index.cjs");
const config = { port: 18183, token: "secret-not-for-output" };
const json = (id, result) => new Response(JSON.stringify({ jsonrpc: "2.0", id, result }), { status: 200, headers: { "content-type": "application/json" } });

test("only a numeric local port and nonempty header-safe token are accepted", () => {
  assert.deepEqual(getConfig({ CLAUDE_PROXY_PORT: "18183", CLAUDE_PROXY_GATEWAY_TOKEN: "xyz" }), { port: 18183, token: "xyz" });
  assert.throws(() => getConfig({ CLAUDE_PROXY_PORT: "80/path", CLAUDE_PROXY_GATEWAY_TOKEN: "xyz" }));
  assert.throws(() => getConfig({ CLAUDE_PROXY_PORT: "18183", CLAUDE_PROXY_GATEWAY_TOKEN: "a\nb" }));
  assert.throws(() => getConfig({ CLAUDE_PROXY_PORT: "18183" }));
});
test("MCP initialize is forwarded exclusively to the local authenticated endpoint", async () => {
  let count = 0;
  const req = { jsonrpc: "2.0", id: 1, method: "initialize", params: { protocolVersion: "2025-11-25", capabilities: {}, clientInfo: { name: "test", version: "1" } } };
  const reply = await relay(req, config, async (url, init) => {
    count++;
    assert.equal(url, "http://127.0.0.1:18183/mcp");
    assert.equal(init.headers.authorization, "Bearer secret-not-for-output");
    assert.equal(init.headers["mcp-protocol-version"], "2025-11-25");
    assert.deepEqual(JSON.parse(init.body), req);
    return json(1, { protocolVersion: "2025-11-25" });
  });
  assert.equal(count, 1);
  assert.equal(reply.result.protocolVersion, "2025-11-25");
});
test("unknown and write-capable tools are rejected before any network call", async () => {
  let called = false;
  const reply = await relay({ jsonrpc: "2.0", id: 2, method: "tools/call", params: { name: "shell" } }, config, async () => { called = true; });
  assert.equal(reply.error.code, -32602);
  assert.equal(called, false);
});
test("only read-only web fetch result is relayed, including source metadata", async () => {
  const req = { jsonrpc: "2.0", id: 3, method: "tools/call", params: { name: "claude_proxy_web_fetch", arguments: { url: "https://github.com/thedotmack/claude-mem" } } };
  const reply = await relay(req, config, async () => json(3, { content: [{ type: "text", text: '{"source_url":"https://github.com/thedotmack/claude-mem"}' }] }));
  assert.match(reply.result.content[0].text, /source_url/);
});
test("a denied gateway request returns a safe diagnostic without leaking credentials", async () => {
  const req = { jsonrpc: "2.0", id: 4, method: "tools/list" };
  const reply = await relay(req, config, async () => new Response("secret-not-for-output", { status: 403 }));
  assert.equal(reply.error.code, -32000);
  assert.doesNotMatch(JSON.stringify(reply), /secret-not-for-output/);
});
test("connection failure is explicit, never fabricated search content", async () => {
  const req = { jsonrpc: "2.0", id: "e", method: "tools/call", params: { name: "claude_proxy_web_search", arguments: { query: "test" } } };
  const reply = await relay(req, config, async () => { throw new Error("bad network"); });
  assert.equal(reply.error.code, -32000);
  assert.doesNotMatch(JSON.stringify(reply), /results/);
});
test("initialized notification produces no stdout response", async () => {
  const req = { jsonrpc: "2.0", method: "notifications/initialized" };
  const reply = await relay(req, config, async () => new Response(null, { status: 202 }));
  assert.equal(reply, null);
});
