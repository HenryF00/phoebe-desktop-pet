// Conservative, CJK-aware context budgeting for the Pi Agent.
//
// Pi ships `estimateTokens` but it uses `chars / 4`, which under-counts Chinese
// by roughly four times. This app is Chinese-first, so we estimate per-script
// and keep history well below the provider's real context window.


/** Token budget for the retained conversation history (not system prompt/tools). */
const DEFAULT_BUDGET_TOKENS = Number(process.env.PHOEBE_CONTEXT_BUDGET || 24_000);
/** Hard cap for a single historical tool result kept in the transcript. */
const MAX_TOOL_RESULT_CHARS = 24_000;
const TRUNCATION_NOTE = "\n\n…（较早的工具输出已截断，仅保留开头片段）";
const IMAGE_TOKENS = 1_200;

export function historyBudgetTokens() {
  return Number.isFinite(DEFAULT_BUDGET_TOKENS) && DEFAULT_BUDGET_TOKENS > 0
    ? DEFAULT_BUDGET_TOKENS
    : 24_000;
}

function isWideChar(character) {
  const code = character.codePointAt(0);
  return (
    (code >= 0x1100 && code <= 0x115f) ||
    code === 0x2329 ||
    code === 0x232a ||
    (code >= 0x2e80 && code <= 0xa4cf && code !== 0x303f) ||
    (code >= 0xac00 && code <= 0xd7a3) ||
    (code >= 0xf900 && code <= 0xfaff) ||
    (code >= 0xfe30 && code <= 0xfe6f) ||
    (code >= 0xff00 && code <= 0xff60) ||
    (code >= 0xffe0 && code <= 0xffe6) ||
    (code >= 0x20000 && code <= 0x3fffd)
  );
}

/** CJK counts about one token per character; latin about four characters per token. */
export function estimateTextTokens(text) {
  if (typeof text !== "string" || !text) return 0;
  let wide = 0;
  let narrow = 0;
  for (const character of text) {
    if (isWideChar(character)) wide += 1;
    else narrow += 1;
  }
  return Math.ceil(wide * 1.05 + narrow / 4);
}

export function estimateMessageTokens(message) {
  if (!message || typeof message !== "object") return 0;
  const content = message.content;
  if (message.role === "user" || message.role === "toolResult") {
    if (typeof content === "string") return estimateTextTokens(content);
    if (!Array.isArray(content)) return 0;
    return content.reduce((sum, block) => {
      if (block?.type === "text") return sum + estimateTextTokens(block.text);
      if (block?.type === "image") return sum + IMAGE_TOKENS;
      return sum;
    }, 0);
  }
  if (!Array.isArray(content)) return 0;
  return content.reduce((sum, block) => {
    if (block?.type === "text") return sum + estimateTextTokens(block.text);
    if (block?.type === "thinking") return sum + estimateTextTokens(block.thinking);
    if (block?.type === "toolCall") {
      let argumentsText = "";
      try {
        argumentsText = JSON.stringify(block.arguments ?? {});
      } catch {
        argumentsText = "";
      }
      return sum + estimateTextTokens(block.name ?? "") + estimateTextTokens(argumentsText);
    }
    return sum;
  }, 0);
}

export function estimateMessagesTokens(messages) {
  if (!Array.isArray(messages)) return 0;
  return messages.reduce((sum, message) => sum + estimateMessageTokens(message), 0);
}

function truncateText(text, maxChars) {
  if (typeof text !== "string" || text.length <= maxChars) return text;
  return text.slice(0, maxChars) + TRUNCATION_NOTE;
}

function truncateContent(content, maxChars) {
  if (typeof content === "string") return truncateText(content, maxChars);
  if (!Array.isArray(content)) return content;
  let remaining = maxChars;
  return content.map(block => {
    if (block?.type !== "text" || typeof block.text !== "string") return block;
    const kept = block.text.slice(0, Math.max(0, remaining));
    remaining -= kept.length;
    return kept.length === block.text.length ? block : { ...block, text: kept + TRUNCATION_NOTE };
  });
}

/** Replaces very large historical tool outputs with a bounded excerpt. */
export function sanitizeMessages(messages, maxToolResultChars = MAX_TOOL_RESULT_CHARS) {
  if (!Array.isArray(messages)) return [];
  return messages.map(message => {
    if (message?.role !== "toolResult") return message;
    const content = message.content;
    if (typeof content === "string") {
      return content.length <= maxToolResultChars
        ? message
        : { ...message, content: truncateText(content, maxToolResultChars) };
    }
    if (Array.isArray(content)) {
      return { ...message, content: truncateContent(content, maxToolResultChars) };
    }
    return message;
  });
}

/**
 * Strips tool calls and their results from the oldest tool exchange, keeping
 * the assistant's own text. Removing a tool call without its result (or vice
 * versa) would produce an invalid transcript, so they are removed together.
 */
function compactOldestToolExchange(messages) {
  for (let index = 0; index < messages.length; index += 1) {
    const message = messages[index];
    if (message?.role !== "assistant" || !Array.isArray(message.content)) continue;
    if (!message.content.some(block => block?.type === "toolCall")) continue;
    const next = messages.slice();
    const keptBlocks = message.content.filter(block => block?.type !== "toolCall");
    next[index] = {
      ...message,
      content: keptBlocks.length ? keptBlocks : [{ type: "text", text: "（此前的工具操作已省略）" }],
    };
    let end = index + 1;
    while (end < next.length && next[end]?.role === "toolResult") end += 1;
    next.splice(index + 1, end - (index + 1));
    return next;
  }
  return null;
}

/**
 * Keeps the newest content that fits in `budgetTokens` in two tiers:
 *
 * 1. Drop the oldest tool exchanges first. Tool output is bulky and usually
 *    single-use, while the surrounding user/assistant text is what keeps a
 *    multi-step task coherent.
 * 2. Only if that is not enough, drop the oldest remaining messages, always
 *    retaining at least the last two and never starting on a tool result.
 */
export function pruneMessages(messages, budgetTokens = historyBudgetTokens()) {
  if (!Array.isArray(messages) || messages.length === 0) return [];
  let working = sanitizeMessages(messages);

  let guard = 0;
  while (estimateMessagesTokens(working) > budgetTokens && guard <= messages.length) {
    const compacted = compactOldestToolExchange(working);
    if (!compacted) break;
    working = compacted;
    guard += 1;
  }

  const minimumKeep = Math.min(2, working.length);
  let total = 0;
  let cut = working.length;
  for (let index = working.length - 1; index >= 0; index -= 1) {
    const cost = estimateMessageTokens(working[index]);
    if (working.length - index > minimumKeep && total + cost > budgetTokens) break;
    total += cost;
    cut = index;
  }
  let kept = working.slice(cut);
  // A tool result must never start the window without its producing assistant turn.
  while (kept.length > 0 && kept[0]?.role === "toolResult") kept = kept.slice(1);
  return kept;
}

/** Diagnostics helper used by tests and optional UI. */
export function messageCount(messages) {
  return Array.isArray(messages) ? messages.length : 0;
}
