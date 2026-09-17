// Turns provider failures into an actionable, user-facing message.
//
// Previously every failure was reported as "模型请求失败", which hid the real
// cause. This classifies the raw provider error so the user can tell a context
// overflow apart from auth, balance, rate limit or network problems.

import { isContextOverflow } from "@earendil-works/pi-ai/utils/overflow";

function shorten(value, limit = 300) {
  const text = typeof value === "string" ? value : value == null ? "" : String(value);
  const trimmed = text.trim();
  return trimmed.length > limit ? `${trimmed.slice(0, limit)}…` : trimmed;
}

function looksLikeOverflow(raw) {
  if (!raw) return false;
  return (
    /(context|token|prompt|input)[^\n]{0,48}(exceed|too long|maximum|length|window|limit)/i.test(raw) ||
    /(exceed|too long|maximum|length|window)[^\n]{0,48}(context|token|prompt|input)/i.test(raw)
  );
}

/**
 * @param {object} input
 * @param {{stopReason?: string, errorMessage?: string}|null} [input.lastAssistant]
 * @param {string} [input.errorMessage] Agent-level error message.
 * @param {unknown} [input.error] Thrown error, if any.
 * @param {number} [input.contextWindow] Model context window, for silent-overflow detection.
 */
export function describeFailure({ lastAssistant = null, errorMessage = "", error = null, contextWindow } = {}) {
  const raw = shorten(
    lastAssistant?.errorMessage || errorMessage || (error instanceof Error ? error.message : error) || "",
  );

  let overflow = looksLikeOverflow(raw);
  if (!overflow && lastAssistant) {
    try {
      overflow = isContextOverflow(lastAssistant, contextWindow);
    } catch {
      overflow = false;
    }
  }
  if (overflow) {
    return raw
      ? `本轮超出模型可用上下文，已自动裁剪较早内容；如仍失败请把问题拆小。原始错误：${raw}`
      : "本轮超出模型可用上下文，已自动裁剪较早内容；请重试或把问题拆小。";
  }

  if (/401|403|unauthor|invalid api key|authentication|api key/i.test(raw)) {
    return `DeepSeek 鉴权失败，请在设置中检查 API Key。原始错误：${raw}`;
  }
  if (/402|insufficient|balance|quota|欠费|余额|credit/i.test(raw)) {
    return `DeepSeek 余额或配额不足，请检查账户。原始错误：${raw}`;
  }
  if (/429|rate limit|too many requests|频繁|overload/i.test(raw)) {
    return `请求过于频繁或服务繁忙，请稍后重试。原始错误：${raw}`;
  }
  if (/timeout|timed out|ETIMEDOUT|ECONNRESET|ENOTFOUND|ECONNREFUSED|EAI_AGAIN|fetch failed|network|socket/i.test(raw)) {
    return `网络连接失败，请检查网络或搜索代理。原始错误：${raw}`;
  }
  return raw ? `模型请求失败：${raw}` : "模型请求失败，请稍后重试。";
}

/** Finds the most recent assistant message in a transcript, if any. */
export function lastAssistantMessage(agent) {
  const messages = agent?.state?.messages;
  if (!Array.isArray(messages)) return null;
  for (let index = messages.length - 1; index >= 0; index -= 1) {
    if (messages[index]?.role === "assistant") return messages[index];
  }
  return null;
}
