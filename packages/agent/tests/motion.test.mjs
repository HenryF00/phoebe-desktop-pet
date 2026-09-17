import test from "node:test";
import assert from "node:assert/strict";
import { inferReplyMotion } from "../src/motion.mjs";

test("friendly short replies become a happy nod", () => {
  assert.deepEqual(inferReplyMotion({ replyText: "当然可以呀！很高兴陪你。" }), {
    emotion: "happy", gesture: "nod",
  });
});

test("tool-backed or structured answers use the presenting gesture", () => {
  assert.deepEqual(inferReplyMotion({ replyText: "搜索结果如下。", usedTool: true }), {
    emotion: "calm", gesture: "point",
  });
  assert.equal(inferReplyMotion({ replyText: "1. 打开设置\n2. 保存修改" }).gesture, "point");
});

test("failures take the concerned expression even when explaining", () => {
  assert.deepEqual(inferReplyMotion({ replyText: "抱歉，搜索服务连接失败。", usedTool: true }), {
    emotion: "concerned", gesture: "point",
  });
});

test("neutral conversation remains calm and still", () => {
  assert.deepEqual(inferReplyMotion({ replyText: "窗外的光线正在慢慢变柔和。" }), {
    emotion: "calm", gesture: "idle",
  });
});

test("affection and compliments use the shy chat reaction", () => {
  assert.deepEqual(inferReplyMotion({ userText: "菲比，你今天真的很可爱", replyText: "谢谢你这样说。" }), {
    emotion: "shy", gesture: "idle",
  });
});
