import test from "node:test";
import assert from "node:assert/strict";
import { createAgentTools, parseSearchResults, searchWeb } from "../src/tools.mjs";

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
  const tools = createAgentTools(() => context, { fetchImpl: async () => ({ ok: true, text: async () => sample }) });
  assert.deepEqual(tools.map(tool => tool.name), ["web_search", "read_selected_file", "get_device_location"]);
  await assert.rejects(tools[1].execute("x", {}), /尚未/);
  await assert.rejects(tools[2].execute("x", {}), /尚未/);
  context = { file: { name: "x.txt", content: "hello" },
    location: { latitude: 30.1, longitude: 120.2, capturedAt: "2026-09-16T00:00:00Z", accuracyMeters: 500 } };
  assert.match((await tools[1].execute("x", {})).content[0].text, /hello/);
  assert.match((await tools[2].execute("x", {})).content[0].text, /30\.1/);
  assert.equal((await tools[0].execute("x", { query: "菲比" })).details.count, 1);
});
