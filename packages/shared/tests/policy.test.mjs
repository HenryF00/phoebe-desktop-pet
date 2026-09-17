import test from "node:test";
import assert from "node:assert/strict";
import { isAutoAllowedTool, isAllowedHttpsUrl } from "../src/index.ts";

test("only the two read-only tools are auto-allowed", () => {
  assert.equal(isAutoAllowedTool("get_current_time"), true);
  assert.equal(isAutoAllowedTool("get_system_status"), true);
  assert.equal(isAutoAllowedTool("launch_wuthering_waves"), false);
  assert.equal(isAutoAllowedTool("bash"), false);
});

test("external destinations must be HTTPS without embedded credentials", () => {
  assert.equal(isAllowedHttpsUrl("https://example.com/path"), true);
  assert.equal(isAllowedHttpsUrl("http://example.com"), false);
  assert.equal(isAllowedHttpsUrl("https://user:secret@example.com"), false);
  assert.equal(isAllowedHttpsUrl("file:///tmp/a"), false);
});
