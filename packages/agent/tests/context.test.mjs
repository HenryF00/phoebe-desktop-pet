import test from "node:test";
import assert from "node:assert/strict";
import { estimateMessagesTokens, estimateTextTokens, pruneMessages, sanitizeMessages } from "../src/context.mjs";

test("CJK text is not under-counted like the latin heuristic", () => {
  // Four Chinese characters are roughly four tokens, not one.
  assert.ok(estimateTextTokens("你好世界") >= 4);
  assert.ok(estimateTextTokens("hello world") >= 2);
  assert.equal(estimateTextTokens(""), 0);
});

test("prune keeps the newest messages within a token budget", () => {
  const messages = [];
  for (let index = 0; index < 40; index += 1) {
    messages.push({ role: "user", content: "x".repeat(4000) });
  }
  const kept = pruneMessages(messages, 8000);
  assert.ok(kept.length >= 2);
  assert.ok(kept.length < 40);
  assert.equal(kept[kept.length - 1].role, "user");
  assert.ok(estimateMessagesTokens(kept) <= 8000 + 4000);
});

test("prune truncates oversized tool results and never starts on a tool result", () => {
  const messages = [
    { role: "user", content: "hi" },
    { role: "assistant", content: [{ type: "toolCall", name: "read_text_file", arguments: {} }] },
    { role: "toolResult", content: [{ type: "text", text: "y".repeat(100_000) }] },
    { role: "user", content: "next" },
  ];
  const kept = pruneMessages(messages, 50_000);
  assert.equal(kept[0].role, "user");
  const toolResult = kept.find(message => message.role === "toolResult");
  assert.ok(toolResult.content[0].text.length < 30_000);
  assert.match(toolResult.content[0].text, /已截断/);
  assert.notEqual(kept[0].role, "toolResult");
});

test("tiered pruning drops old tool output before conversation text", () => {
  const messages = [
    { role: "user", content: "old question" },
    { role: "assistant", content: [{ type: "text", text: "old answer" }, { type: "toolCall", name: "read_text_file", arguments: {}, id: "t1" }] },
    { role: "toolResult", toolCallId: "t1", content: [{ type: "text", text: "y".repeat(30_000) }] },
    { role: "user", content: "recent question" },
    { role: "assistant", content: [{ type: "text", text: "recent answer" }] },
  ];
  const kept = pruneMessages(messages, 4_000);
  assert.ok(!kept.some(message => message.role === "toolResult"));
  assert.ok(kept.some(message => message.content === "old question"));
  assert.ok(kept.some(message => Array.isArray(message.content)
    && message.content.some(block => block.text === "old answer")));
  assert.ok(kept.some(message => message.content === "recent question"));
});

test("prune always keeps at least the last two messages", () => {
  const messages = [
    { role: "user", content: "a".repeat(100_000) },
    { role: "assistant", content: [{ type: "text", text: "b".repeat(100_000) }] },
  ];
  const kept = pruneMessages(messages, 100);
  assert.equal(kept.length, 2);
});

test("sanitize only rewrites tool results", () => {
  const messages = [
    { role: "user", content: "keep me" },
    { role: "toolResult", content: "z".repeat(20_000) },
  ];
  const sanitized = sanitizeMessages(messages, 5_000);
  assert.equal(sanitized[0].content, "keep me");
  assert.ok(sanitized[1].content.length < 6_000);
});
