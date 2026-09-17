import readline from "node:readline";
import { Agent } from "@earendil-works/pi-agent-core";
import { createModels } from "@earendil-works/pi-ai";
import { deepseekProvider } from "@earendil-works/pi-ai/providers/deepseek";
import { encodeEvent, parseCommand } from "./protocol.mjs";
import { buildSystemPrompt } from "./prompt-builder.mjs";
import { createAgentTools } from "./tools.mjs";
import { inferReplyMotion } from "./motion.mjs";
import { buildSpeechText } from "./speech.mjs";
import { addTokenUsage, emptyTokenUsage } from "./usage.mjs";

// Only the Rust supervisor should launch this in production. No frontend IPC or local port.
const models = createModels();
models.setProvider(deepseekProvider());
const modelId = process.env.PHOEBE_DEEPSEEK_MODEL || "deepseek-v4-flash";
const model = models.getModel("deepseek", modelId);
if (!model) throw new Error("DeepSeek model is unavailable in the pinned Pi catalog");

let agent = null;
let agentTools = null;
let active = null;
let toolContext = null;

function send(event) { process.stdout.write(encodeEvent(event)); }

function getAgentTools() {
  if (!agentTools) agentTools = createAgentTools(() => toolContext);
  return agentTools;
}

function systemPromptFor(memories = [], interactionMode = "assistant") {
  return buildSystemPrompt({
    memories,
    interactionMode,
    capabilities: getAgentTools().map(({ name, description }) => ({ name, description })),
  });
}

function getAgent() {
  if (agent) return agent;
  agent = new Agent({
    initialState: {
      systemPrompt: systemPromptFor(),
      model,
      tools: getAgentTools(),
    },
    streamFn: models.streamSimple.bind(models),
  });
  agent.subscribe(event => {
    if (!active) return;
    if (event.type === "message_end" && event.message?.role === "assistant") {
      active.usage = addTokenUsage(active.usage, event.message.usage);
      send({ type: "usage", runId: active.runId, usage: active.usage });
      return;
    }
    if (event.type === "tool_execution_start") {
      active.usedTool = true;
      send({ type: "tool_started", runId: active.runId, tool: event.toolName });
      return;
    }
    if (event.type === "tool_execution_end") {
      send({ type: "tool_finished", runId: active.runId, result: {
        status: event.isError ? "failed" : "success", message: event.isError ? "工具执行失败" : "工具已返回结果",
      } });
      return;
    }
    if (event.type === "message_update" && event.assistantMessageEvent?.type === "text_delta") {
      const delta = event.assistantMessageEvent.delta;
      active.text += delta;
      send({ type: "text_delta", runId: active.runId, delta });
    }
  });
  return agent;
}

async function handle(command) {
  if (command.type === "status") {
    send({ type: "status", agent: process.env.DEEPSEEK_API_KEY ? "ready" : "not_configured", model: model.id });
    return;
  }
  if (command.type === "cancel") {
    if (active?.runId === command.runId) {
      active.cancelled = true;
      getAgent().abort();
    }
    return;
  }
  if (active) { send({ type: "error", runId: command.runId, message: "另一轮对话仍在进行" }); return; }
  if (!process.env.DEEPSEEK_API_KEY) {
    send({ type: "error", runId: command.runId, message: "DeepSeek API Key 尚未由桌面核心配置" });
    return;
  }
  const run = { runId: command.runId, text: "", userText: command.text, interactionMode: command.interactionMode,
    usedTool: false, cancelled: false, timedOut: false, usage: emptyTokenUsage() };
  active = run;
  toolContext = { file: command.file, location: command.location };
  send({ type: "state", state: "thinking", runId: run.runId });
  const timeout = setTimeout(() => {
    run.timedOut = true;
    getAgent().abort();
  }, 120_000);
  try {
    const currentAgent = getAgent();
    currentAgent.state.systemPrompt = systemPromptFor(command.memories, command.interactionMode);
    currentAgent.state.messages = currentAgent.state.messages.slice(-16);
    await currentAgent.prompt(command.text);
    if (run.timedOut) send({ type: "error", runId: run.runId, message: "模型回复超时；请重试" });
    else if (run.cancelled) send({ type: "cancelled", runId: run.runId });
    else if (agent.state.errorMessage) send({ type: "error", runId: run.runId, message: "模型请求失败，请检查连接、余额和模型配置" });
    else {
      const motion = inferReplyMotion({ replyText: run.text, userText: run.userText, usedTool: run.usedTool });
      const displayText = run.text.trim();
      send({ type: "completed", runId: run.runId, reply: {
        display_text: displayText,
        speech_text: buildSpeechText(displayText, run.interactionMode),
        ...motion,
      } });
    }
  } catch {
    send(run.timedOut
      ? { type: "error", runId: run.runId, message: "模型回复超时；请重试" }
      : run.cancelled
      ? { type: "cancelled", runId: run.runId }
      : { type: "error", runId: run.runId, message: "模型请求失败，请稍后重试" });
  } finally {
    clearTimeout(timeout);
    if (agent && !agent.state.isStreaming) {
      agent.state.messages = command.file || command.location ? [] : agent.state.messages.slice(-16);
    }
    active = null;
    toolContext = null;
    send({ type: "state", state: "idle", runId: run.runId });
  }
}

const lines = readline.createInterface({ input: process.stdin, crlfDelay: Infinity });
lines.on("line", line => {
  try { void handle(parseCommand(line)); }
  catch { send({ type: "error", message: "无效的 Sidecar 命令" }); }
});
lines.on("close", () => { if (active) getAgent().abort(); });
