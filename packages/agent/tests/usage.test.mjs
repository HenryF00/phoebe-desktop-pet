import test from "node:test";
import assert from "node:assert/strict";
import { addTokenUsage, emptyTokenUsage, normalizeTokenUsage } from "../src/usage.mjs";

test("normalizes provider usage without trusting invalid values", () => {
  assert.deepEqual(normalizeTokenUsage({
    input: 12.9, output: 4, cacheRead: -3, cacheWrite: "2", totalTokens: Number.NaN,
  }), {
    input: 12, output: 4, cacheRead: 0, cacheWrite: 2, totalTokens: 0,
  });
});

test("accumulates all assistant messages in one agent run", () => {
  const first = addTokenUsage(emptyTokenUsage(), {
    input: 100, output: 20, cacheRead: 10, cacheWrite: 0, totalTokens: 130,
  });
  assert.deepEqual(addTokenUsage(first, {
    input: 140, output: 30, cacheRead: 20, cacheWrite: 5, totalTokens: 195,
  }), {
    input: 240, output: 50, cacheRead: 30, cacheWrite: 5, totalTokens: 325,
  });
});
