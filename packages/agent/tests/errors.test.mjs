import test from "node:test";
import assert from "node:assert/strict";
import { describeFailure, lastAssistantMessage } from "../src/errors.mjs";

test("classifies context overflow from the raw provider message", () => {
  assert.match(describeFailure({ errorMessage: "This model's maximum context length is 32768 tokens" }), /上下文/);
  assert.match(describeFailure({ errorMessage: "prompt is too long: 40000 tokens > 32768 maximum" }), /上下文/);
});

test("classifies auth, balance, rate limit and network failures", () => {
  assert.match(describeFailure({ errorMessage: "401 Unauthorized" }), /鉴权/);
  assert.match(describeFailure({ errorMessage: "Insufficient Balance" }), /余额/);
  assert.match(describeFailure({ errorMessage: "429 Too Many Requests" }), /频繁/);
  assert.match(describeFailure({ errorMessage: "fetch failed" }), /网络/);
});

test("prefers the assistant error message and falls back to the raw text", () => {
  assert.match(describeFailure({ lastAssistant: { errorMessage: "429 rate limit exceeded" }, errorMessage: "generic" }), /频繁/);
  assert.match(describeFailure({ errorMessage: "weird provider error" }), /weird provider error/);
  assert.match(describeFailure({}), /稍后重试/);
});

test("lastAssistantMessage finds the newest assistant turn", () => {
  const agent = { state: { messages: [
    { role: "user", content: "a" },
    { role: "assistant", content: [] },
    { role: "toolResult", content: [] },
    { role: "assistant", content: [] },
  ] } };
  assert.equal(lastAssistantMessage(agent), agent.state.messages[3]);
  assert.equal(lastAssistantMessage({ state: { messages: [] } }), null);
  assert.equal(lastAssistantMessage(null), null);
});
