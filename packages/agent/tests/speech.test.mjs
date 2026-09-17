import test from "node:test";
import assert from "node:assert/strict";
import { buildSpeechText, normalizeForSpeech } from "../src/speech.mjs";

test("assistant mode speaks only the opening summary paragraph", () => {
  const text = "上海今天有小雨，出门记得带伞。\n\n详细天气与来源：[天气服务](https://weather.example/test)。";
  assert.equal(buildSpeechText(text, "assistant"), "上海今天有小雨，出门记得带伞。");
});

test("assistant summaries are bounded even when the model violates the prompt", () => {
  const speech = buildSpeechText("这是一段非常长的语音摘要，".repeat(12), "assistant");
  assert.ok(Array.from(speech).length <= 60);
});

test("chat mode reads the full natural-language reply without markup or URLs", () => {
  const text = "今天有小雨，详情见[天气页面](https://weather.example)。\n\n```js\nalert(1)\n```";
  assert.equal(buildSpeechText(text, "chat"), "今天有小雨，详情见天气页面。 代码已显示在聊天窗口。");
});

test("speech normalization removes list and emphasis syntax", () => {
  assert.equal(normalizeForSpeech("## 建议\n- **带伞**\n- 穿外套"), "建议 带伞 穿外套");
});
