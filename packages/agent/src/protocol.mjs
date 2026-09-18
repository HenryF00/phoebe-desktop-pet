import { buildSystemPrompt } from "./prompt-builder.mjs";
import { ALL_TOOL_NAMES } from "./tools.mjs";

const KNOWN_TOOLS = new Set(ALL_TOOL_NAMES);

/** Largest command line. Images (base64) make the prompt line much bigger than text. */
const MAX_COMMAND_BYTES = 8 * 1024 * 1024;

export function parseCommand(line) {
  if (typeof line !== "string" || line.length > MAX_COMMAND_BYTES) throw new Error("invalid command size");
  let value;
  try { value = JSON.parse(line); } catch { throw new Error("invalid JSON"); }
  if (!value || typeof value !== "object" || Array.isArray(value)) throw new Error("invalid command");
  if (value.type === "status") return { type: "status" };
  if (value.type === "cancel" && typeof value.runId === "string" && value.runId.length <= 100)
    return { type: "cancel", runId: value.runId };
  if (value.type === "tool_result" && typeof value.requestId === "string"
      && value.requestId.length > 0 && value.requestId.length <= 100) {
    if (value.status !== "success" && value.status !== "denied" && value.status !== "failed")
      throw new Error("invalid tool result status");
    let text = "";
    if (value.content !== undefined) {
      if (!Array.isArray(value.content) || value.content.length > 8) throw new Error("invalid tool result content");
      let totalBytes = 0;
      for (const part of value.content) {
        if (!part || typeof part !== "object" || part.type !== "text" || typeof part.text !== "string")
          throw new Error("invalid tool result content");
        totalBytes += Buffer.byteLength(part.text);
      }
      if (totalBytes > 200_000) throw new Error("tool result too large");
      text = value.content.map(part => part.text).join("\n");
    }
    return { type: "tool_result", requestId: value.requestId, status: value.status, text,
      isError: value.isError === true || value.status !== "success",
      details: value.details && typeof value.details === "object" ? value.details : null };
  }
  if (value.type === "prompt" && typeof value.runId === "string" && value.runId.length > 0 && value.runId.length <= 100
      && typeof value.text === "string" && value.text.trim().length > 0 && value.text.length <= 10000) {
    const interactionMode = value.interactionMode === undefined ? "assistant" : value.interactionMode;
    if (interactionMode !== "assistant" && interactionMode !== "chat") throw new Error("invalid interaction mode");
    const memories = value.memories === undefined ? [] : value.memories;
    if (!Array.isArray(memories) || memories.length > 20) throw new Error("invalid memory context");
    let totalBytes = 0;
    const checked = memories.map(memory => {
      if (!memory || typeof memory !== "object" || Array.isArray(memory)
          || typeof memory.title !== "string" || typeof memory.content !== "string")
        throw new Error("invalid memory entry");
      const titleBytes = Buffer.byteLength(memory.title);
      const contentBytes = Buffer.byteLength(memory.content);
      totalBytes += titleBytes + contentBytes;
      if (!memory.title.trim() || !memory.content.trim() || titleBytes > 120 || contentBytes > 600 || totalBytes > 16_000)
        throw new Error("invalid memory size");
      return { title: memory.title, content: memory.content };
    });
    let file = null;
    if (value.file !== undefined && value.file !== null) {
      if (!value.file || typeof value.file !== "object" || Array.isArray(value.file)
          || typeof value.file.name !== "string" || !value.file.name.trim()
          || Buffer.byteLength(value.file.name) > 240 || typeof value.file.content !== "string"
          || Buffer.byteLength(value.file.content) > 20_000) throw new Error("invalid attached file");
      file = { name: value.file.name, content: value.file.content };
    }
    let image = null;
    if (value.image !== undefined && value.image !== null) {
      const img = value.image;
      if (!img || typeof img !== "object" || Array.isArray(img)
          || typeof img.name !== "string" || !img.name.trim()
          || Buffer.byteLength(img.name) > 240
          || !Array.isArray(img.frames) || img.frames.length === 0 || img.frames.length > 6)
        throw new Error("invalid attached image");
      let total = 0;
      const frames = img.frames.map(frame => {
        if (!frame || typeof frame !== "object" || Array.isArray(frame)
            || typeof frame.mimeType !== "string"
            || !["image/png", "image/jpeg", "image/webp"].includes(frame.mimeType)
            || typeof frame.data !== "string" || frame.data.length === 0
            || frame.data.length > 3_000_000
            || !/^[A-Za-z0-9+/]+=*$/.test(frame.data))
          throw new Error("invalid attached image frame");
        total += frame.data.length;
        return { mimeType: frame.mimeType, data: frame.data };
      });
      if (total > 7_000_000) throw new Error("attached image too large");
      image = { name: img.name, frames };
    }
    let tools = null;
    const occasion = value.occasion === "birthday" ? "birthday" : null;
    let userAddress = "";
    if (value.userAddress !== undefined && value.userAddress !== null) {
      if (typeof value.userAddress !== "string") throw new Error("invalid user address");
      const trimmed = value.userAddress.trim();
      if (trimmed.length > 24 || Array.from(trimmed).some(ch => ch.charCodeAt(0) < 32))
        throw new Error("invalid user address");
      userAddress = trimmed;
    }
    if (value.tools !== undefined && value.tools !== null) {
      if (!Array.isArray(value.tools) || value.tools.length > 64)
        throw new Error("invalid tool list");
      const names = new Set();
      for (const name of value.tools) {
        if (typeof name !== "string" || !/^[a-z_]{1,48}$/.test(name)) throw new Error("invalid tool name");
        // Only tools this sidecar actually defines can ever be registered; an
        // unknown name is dropped rather than trusted.
        if (KNOWN_TOOLS.has(name)) names.add(name);
      }
      if (names.size) tools = [...names];
    }
    let folderGrants = null;
    if (value.folderGrants !== undefined && value.folderGrants !== null) {
      if (!Array.isArray(value.folderGrants) || value.folderGrants.length > 100)
        throw new Error("invalid folder grants");
      folderGrants = value.folderGrants.map(grant => {
        if (!grant || typeof grant !== "object" || Array.isArray(grant)
            || typeof grant.grantId !== "string" || !/^[0-9a-f]{32}$/.test(grant.grantId)
            || typeof grant.label !== "string" || !grant.label.trim() || Buffer.byteLength(grant.label) > 120
            || typeof grant.read !== "boolean" || typeof grant.write !== "boolean")
          throw new Error("invalid folder grant");
        return { grantId: grant.grantId, label: grant.label, read: grant.read, write: grant.write };
      });
    }
    const command = { type: "prompt", runId: value.runId, text: value.text, interactionMode, memories: checked, file, image, occasion, userAddress };
    if (tools) command.tools = tools;
    if (folderGrants) command.folderGrants = folderGrants;
    return command;
  }
  if (value.type === "conversation" && typeof value.conversationId === "string"
      && value.conversationId.length > 0 && value.conversationId.length <= 64
      && /^[A-Za-z0-9-]+$/.test(value.conversationId) && Array.isArray(value.history)
      && value.history.length <= 200) {
    const history = [];
    let totalBytes = 0;
    for (const entry of value.history) {
      if (!entry || typeof entry !== "object" || Array.isArray(entry)
          || (entry.role !== "user" && entry.role !== "assistant")
          || typeof entry.content !== "string")
        throw new Error("invalid history entry");
      const bytes = Buffer.byteLength(entry.content);
      totalBytes += bytes;
      if (bytes > 1_000_000 || totalBytes > 8_000_000) throw new Error("history too large");
      history.push({ role: entry.role, content: entry.content });
    }
    return { type: "conversation", conversationId: value.conversationId, history };
  }
  throw new Error("unsupported command");
}

export function memorySystemPrompt(memories, interactionMode = "assistant", tools = [
  "web_search", "read_selected_file", "get_device_location", "get_current_time", "get_system_status"]) {
  return buildSystemPrompt({
    memories,
    interactionMode,
    capabilities: tools,
  });
}

export function encodeEvent(event) {
  return JSON.stringify(event) + "\n";
}
