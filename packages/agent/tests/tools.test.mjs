import test from "node:test";
import assert from "node:assert/strict";
import { createAgentTools, parseSearchResults, searchWeb, selectAgentTools, SAFE_DEFAULT_TOOLS } from "../src/tools.mjs";

const sample = "1.[Phoebe (given name) - Wikipedia](https://duckduckgo.com/l/?uddg=https%3A%2F%2Fen.wikipedia.org%2Fwiki%2FPhoebe_(given_name)&rut=x)\nA name.\n";

test("search extracts direct HTTPS citations and rejects challenge pages", () => {
  assert.deepEqual(parseSearchResults(sample), [{ title: "Phoebe (given name) - Wikipedia",
    url: "https://en.wikipedia.org/wiki/Phoebe_(given_name)", snippet: "A name." }]);
  assert.deepEqual(parseSearchResults("Warning: This page maybe requiring CAPTCHA"), []);
  const sponsored = "1.[Ad](https://duckduckgo.com/l/?uddg=https%3A%2F%2Fduckduckgo.com%2Fy.js%3Fad_domain%3Dexample.com) (Sponsored link - [more info](https://duckduckgo.com/))";
  assert.deepEqual(parseSearchResults(sponsored), []);
  assert.deepEqual(parseSearchResults("1.[Ad](https://duckduckgo.com/l/?uddg=https%3A%2F%2Fduckduckgo.com%2Fy.js%3Fad_domain%3Dexample.com)"), []);
});

test("search uses the fixed provider URL and bounded query", async () => {
  let requested = "";
  const data = await searchWeb("菲比", { fetchImpl: async url => {
    requested = url;
    return { ok: true, text: async () => sample };
  } });
  assert.match(requested, /^https:\/\/r\.jina\.ai\/https:\/\/lite\.duckduckgo\.com\/lite\/\?q=/);
  assert.equal(data.results.length, 1);
  await assert.rejects(searchWeb("x".repeat(201)), /搜索词/);
  await assert.rejects(searchWeb("hello", { fetchImpl: async () => { throw new Error("fetch failed"); } }), /配置已信任的本机搜索代理/);
});

test("file and location tools only expose the current authorized attachments", async () => {
  let context = { file: null, location: null };
  const tools = createAgentTools({ getContext: () => context, fetchImpl: async () => ({ ok: true, text: async () => sample }) });
  assert.deepEqual(tools.map(tool => tool.name),
    ["web_search", "read_selected_file", "get_device_location", "get_current_time", "get_system_status", "open_url"]);
  await assert.rejects(tools[1].execute("x", {}), /尚未/);
  await assert.rejects(tools[2].execute("x", {}), /尚未/);
  context = { file: { name: "x.txt", content: "hello" },
    location: { latitude: 30.1, longitude: 120.2, capturedAt: "2026-09-16T00:00:00Z", accuracyMeters: 500 } };
  assert.match((await tools[1].execute("x", {})).content[0].text, /hello/);
  assert.match((await tools[2].execute("x", {})).content[0].text, /30\.1/);
  assert.equal((await tools[0].execute("x", { query: "菲比" })).details.count, 1);
});

test("operating-system tools forward to the Rust broker and fail closed", async () => {
  const calls = [];
  let context = { requestTool: async (tool, args) => {
    calls.push({ tool, args });
    return { content: [{ type: "text", text: `${tool}:ok` }], details: { via: "broker" } };
  } };
  const tools = createAgentTools({ getContext: () => context });
  const openUrl = tools.find(tool => tool.name === "open_url");
  const result = await openUrl.execute("id", { url: "https://example.com" });
  assert.deepEqual(calls, [{ tool: "open_url", args: { url: "https://example.com" } }]);
  assert.equal(result.content[0].text, "open_url:ok");
  assert.deepEqual(result.details, { via: "broker" });

  assert.deepEqual(selectAgentTools(tools, ["web_search", "open_url"]).map(tool => tool.name),
    ["web_search", "open_url"]);
  assert.deepEqual(selectAgentTools(tools, SAFE_DEFAULT_TOOLS).map(tool => tool.name),
    SAFE_DEFAULT_TOOLS);

  const ungranted = createAgentTools({ getContext: () => ({}) });
  await assert.rejects(ungranted.find(tool => tool.name === "open_url").execute("id", { url: "https://example.com" }),
    /未向 Agent 注册/);
});
