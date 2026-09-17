const fencedCodePattern = /```[\s\S]*?```/g;
const markdownImagePattern = /!\[[^\]]*\]\([^)]*\)/g;
const markdownLinkPattern = /\[([^\]]+)\]\([^)]*\)/g;
const bareUrlPattern = /https?:\/\/[^\s)）\]}]+/gi;
const listMarkerPattern = /^\s*(?:#{1,6}\s+|[-*+]\s+|\d+[.)、]\s+)/gm;

export function normalizeForSpeech(value) {
  return String(value ?? "")
    .replace(fencedCodePattern, "代码已显示在聊天窗口。")
    .replace(markdownImagePattern, "")
    .replace(markdownLinkPattern, "$1")
    .replace(bareUrlPattern, "")
    .replace(listMarkerPattern, "")
    .replace(/[`*_~>|]/g, "")
    .replace(/\s+/g, " ")
    .trim();
}

function clampSummary(value, maximum = 60) {
  const characters = Array.from(value);
  if (characters.length <= maximum) return value;
  const prefix = characters.slice(0, maximum).join("");
  const punctuation = Math.max(prefix.lastIndexOf("。"), prefix.lastIndexOf("！"), prefix.lastIndexOf("？"));
  if (punctuation >= 18) return prefix.slice(0, punctuation + 1);
  return `${characters.slice(0, maximum - 1).join("")}…`;
}

export function buildSpeechText(displayText, interactionMode) {
  const text = String(displayText ?? "").trim();
  if (interactionMode === "chat") return normalizeForSpeech(text);
  const firstParagraph = text.split(/\n\s*\n/).map(part => part.trim()).find(Boolean) ?? text;
  return clampSummary(normalizeForSpeech(firstParagraph));
}
