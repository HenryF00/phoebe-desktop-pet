import { buildSystemPrompt } from "./prompt-builder.mjs";

export function parseCommand(line) {
  if (typeof line !== "string" || line.length > 65536) throw new Error("invalid command size");
  let value;
  try { value = JSON.parse(line); } catch { throw new Error("invalid JSON"); }
  if (!value || typeof value !== "object" || Array.isArray(value)) throw new Error("invalid command");
  if (value.type === "status") return { type: "status" };
  if (value.type === "cancel" && typeof value.runId === "string" && value.runId.length <= 100)
    return { type: "cancel", runId: value.runId };
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
    let location = null;
    if (value.location !== undefined && value.location !== null) {
      const loc = value.location;
      if (!loc || typeof loc !== "object" || Array.isArray(loc)
          || typeof loc.latitude !== "number" || !Number.isFinite(loc.latitude) || Math.abs(loc.latitude) > 90
          || typeof loc.longitude !== "number" || !Number.isFinite(loc.longitude) || Math.abs(loc.longitude) > 180
          || typeof loc.capturedAt !== "string" || !Number.isFinite(Date.parse(loc.capturedAt))
          || typeof loc.accuracyMeters !== "number" || !Number.isFinite(loc.accuracyMeters)
          || loc.accuracyMeters < 0 || loc.accuracyMeters > 100_000) throw new Error("invalid attached location");
      location = { latitude: loc.latitude, longitude: loc.longitude,
        capturedAt: loc.capturedAt, accuracyMeters: loc.accuracyMeters };
    }
    return { type: "prompt", runId: value.runId, text: value.text, interactionMode, memories: checked, file, location };
  }
  throw new Error("unsupported command");
}

export function memorySystemPrompt(memories, interactionMode = "assistant") {
  return buildSystemPrompt({
    memories,
    interactionMode,
    capabilities: ["web_search", "read_selected_file", "get_device_location"],
  });
}

export function encodeEvent(event) {
  return JSON.stringify(event) + "\n";
}
