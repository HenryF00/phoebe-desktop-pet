import test from "node:test";
import assert from "node:assert/strict";
import { compactIfNeeded } from "../src/summary.mjs";

function turns(count, size) {
  const list = [];
  for (let i = 0; i < count; i++) {
    list.push({ role: "user", content: `问题${i} ${"x".repeat(size)}`, timestamp: Date.now() });
    list.push({ role: "assistant", content: [{ type: "text", text: `回答${i} ${"y".repeat(size)}` }], timestamp: Date.now() });
  }
  return list;
}

test("no compaction below the threshold", async () => {
  const messages = turns(3, 10);
  const models = { completeSimple: async () => { throw new Error("should not call"); } };
  const result = await compactIfNeeded(messages, models, {});
  assert.equal(result, messages);
});

test("compaction summarizes the oldest and keeps the recent messages", async () => {
  const messages = turns(20, 2500);
  const models = {
    completeSimple: async (_model, ctx, _options) => ({
      stopReason: "stop",
      content: [{ type: "text", text: "## 目标\n总结内容" }],
      usage: {},
    }),
  };
  const result = await compactIfNeeded(messages, models, {});
  assert.ok(result.length < messages.length);
  assert.match(result[0].content, /自动摘要/);
  assert.match(result[0].content, /总结内容/);
  // The most recent assistant message is still present at the end.
  assert.deepEqual(result[result.length - 1].content, messages[messages.length - 1].content);
});

test("summarization failure falls back to the original messages", async () => {
  const messages = turns(20, 2500);
  const models = { completeSimple: async () => { throw new Error("network"); } };
  const result = await compactIfNeeded(messages, models, {});
  assert.equal(result, messages);
});

test("an existing summary is excluded and updated rather than re-summarized", async () => {
  const previous = "（以下为此前对话的自动摘要，用于节省上下文，并非用户本次输入）\n\n## 目标\n旧目标";
  const messages = [{ role: "user", content: previous, timestamp: Date.now() }, ...turns(20, 2500)];
  let capturedPrompt = "";
  const models = {
    completeSimple: async (_model, ctx, _options) => {
      capturedPrompt = ctx.messages[0].content;
      return { stopReason: "stop", content: [{ type: "text", text: "更新后摘要" }], usage: {} };
    },
  };
  const result = await compactIfNeeded(messages, models, {});
  assert.match(capturedPrompt, /previous-summary/);
  assert.match(capturedPrompt, /旧目标/);
  const summaryCount = result.filter(
    (m) => m.role === "user" && typeof m.content === "string" && m.content.startsWith("（以下为此前对话"),
  ).length;
  assert.equal(summaryCount, 1);
});
