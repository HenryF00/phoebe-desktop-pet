// Context summarization for long conversations.
//
// When a conversation's retained history exceeds the token budget, instead of
// silently dropping the oldest messages we ask the model for a compact
// checkpoint summary, then keep that summary plus the most recent messages.
// The summary is a normal user message prefixed with a marker, so the default
// convertToLlm keeps it and the model can read it. A failed summarization
// falls back to the existing pruneMessages dropping behaviour.

import { contentText } from "@earendil-works/pi-ai";
import { estimateMessagesTokens } from "./context.mjs";

const SUMMARY_THRESHOLD_TOKENS = Number(process.env.PHOEBE_SUMMARY_THRESHOLD || 18_000);
const SUMMARY_MIN_MESSAGES = 12;
const SUMMARY_MAX_TOKENS = 2000;
const SUMMARY_MARKER = "（以下为此前对话的自动摘要";

const SUMMARY_SYSTEM_PROMPT = `You are a context summarization assistant. Read a conversation between a user and an AI assistant and produce a concise structured summary so the assistant can continue without the original messages. Do NOT continue the conversation. Do NOT answer any question. ONLY output the summary.`;

const SUMMARY_PROMPT = `总结上面的对话，作为给同一个 AI 助手继续任务用的上下文检查点。用简洁中文，保持以下格式：

## 目标
[用户想做什么]

## 约束与偏好
- [用户提到的约束/偏好，没有就写“无”]

## 已完成
- [已完成的事项]

## 进行中
- [当前在做的事]

## 关键决定
- [决定：理由]

## 下一步
1. [接下来要做什么]

## 关键上下文
- [需要保留的数据、文件名、路径、错误信息，没有就写“无”]`;

const UPDATE_SUMMARY_PROMPT = `下面是新的对话消息，需要并入 <previous-summary> 里的已有摘要。规则：保留旧摘要的全部信息，新增新进度/决定/上下文，更新“进行中/已完成/下一步”。保持同样的格式。`;

function textOf(message) {
  if (message.role === "user") {
    if (typeof message.content === "string") return message.content;
    return message.content
      .filter((block) => block.type === "text")
      .map((block) => block.text)
      .join("");
  }
  if (message.role === "assistant") {
    return message.content
      .filter((block) => block.type === "text")
      .map((block) => block.text)
      .join("");
  }
  if (message.role === "toolResult") {
    if (typeof message.content === "string") return message.content;
    return message.content
      .filter((block) => block.type === "text")
      .map((block) => block.text)
      .join("");
  }
  return "";
}

function serializeForSummary(messages) {
  const lines = [];
  for (const message of messages) {
    const text = textOf(message).trim();
    if (!text) continue;
    if (message.role === "user") lines.push(`用户：${text.slice(0, 4000)}`);
    else if (message.role === "assistant") lines.push(`助手：${text.slice(0, 4000)}`);
    else if (message.role === "toolResult") {
      lines.push(`[工具 ${message.toolName} 结果]：${text.slice(0, 2000)}`);
    }
  }
  return lines.join("\n\n");
}

function summaryMessage(summary) {
  return {
    role: "user",
    content: `${SUMMARY_MARKER}，用于节省上下文，并非用户本次输入）\n\n${summary}`,
    timestamp: Date.now(),
  };
}

async function generateSummary(toSummarize, models, model, previousSummary) {
  const conversationText = serializeForSummary(toSummarize);
  if (!conversationText.trim()) return null;
  const prompt = previousSummary
    ? `<previous-summary>\n${previousSummary}\n</previous-summary>\n\n<conversation>\n${conversationText}\n</conversation>\n\n${UPDATE_SUMMARY_PROMPT}`
    : `<conversation>\n${conversationText}\n</conversation>\n\n${SUMMARY_PROMPT}`;
  const response = await models.completeSimple(
    model,
    {
      systemPrompt: SUMMARY_SYSTEM_PROMPT,
      messages: [{ role: "user", content: prompt, timestamp: Date.now() }],
      tools: [],
    },
    { maxTokens: SUMMARY_MAX_TOKENS },
  );
  if (response.stopReason === "error" || response.stopReason === "aborted") return null;
  const summary = contentText(response.content).trim();
  return summary || null;
}

/**
 * Compacts a transcript when it exceeds the budget. Returns the original array
 * when compaction is unnecessary or the summary call fails (so pruning keeps
 * its drop-oldest behaviour as a fallback).
 */
export async function compactIfNeeded(messages, models, model) {
  if (!Array.isArray(messages)) return messages;
  let previousSummary = null;
  const rest = [];
  for (const message of messages) {
    if (
      message.role === "user" &&
      typeof message.content === "string" &&
      message.content.startsWith(SUMMARY_MARKER)
    ) {
      previousSummary = message.content;
      continue;
    }
    rest.push(message);
  }
  if (rest.length < SUMMARY_MIN_MESSAGES) return messages;
  if (estimateMessagesTokens(rest) <= SUMMARY_THRESHOLD_TOKENS) return messages;

  const keepCount = Math.max(4, Math.floor(rest.length * 0.4));
  const toSummarize = rest.slice(0, rest.length - keepCount);
  const kept = rest.slice(rest.length - keepCount);
  try {
    const summary = await generateSummary(toSummarize, models, model, previousSummary);
    if (!summary) return messages;
    return [summaryMessage(summary), ...kept];
  } catch {
    return messages;
  }
}
