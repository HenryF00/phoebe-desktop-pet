import readline from "node:readline";
import { randomUUID } from "node:crypto";
import { Agent } from "@earendil-works/pi-agent-core";
import { createModels } from "@earendil-works/pi-ai";
import { deepseekProvider } from "@earendil-works/pi-ai/providers/deepseek";
import { encodeEvent, parseCommand } from "./protocol.mjs";
import { buildSystemPrompt } from "./prompt-builder.mjs";
import { createAgentTools, selectAgentTools, SAFE_DEFAULT_TOOLS } from "./tools.mjs";
import { inferReplyMotion } from "./motion.mjs";
import { buildSpeechText } from "./speech.mjs";
import { addTokenUsage, emptyTokenUsage } from "./usage.mjs";
import { pruneMessages } from "./context.mjs";
import { describeFailure, lastAssistantMessage } from "./errors.mjs";

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

/** In-flight operating-system tool requests awaiting a Rust `tool_result`. */
const pendingTools = new Map();

function send(event) { process.stdout.write(encodeEvent(event)); }

function getAllAgentTools() {
  if (!agentTools) agentTools = createAgentTools({ getContext: () => toolContext });
  return agentTools;
}

function toolsForTurn(allowed) {
  return selectAgentTools(getAllAgentTools(), allowed);
}

function systemPromptFor(memories = [], interactionMode = "assistant", allowed = SAFE_DEFAULT_TOOLS, folderGrants = []) {
  return buildSystemPrompt({
    memories,
    interactionMode,
    capabilities: toolsForTurn(allowed).map(({ name, description }) => ({ name, description })),
    folderGrants,
  });
}

function getAgent() {
  if (agent) return agent;
  agent = new Agent({
    initialState: {
      systemPrompt: systemPromptFor(),
      model,
      tools: getAllAgentTools(),
    },
    streamFn: models.streamSimple.bind(models),
    // Runs before every provider request, including between tool calls inside a
    // single turn, so accumulated tool output can never blow the context.
    transformContext: async messages => pruneMessages(messages),
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

function abortPending(runId) {
  for (const [requestId, pending] of pendingTools) {
    if (runId && pending.runId !== runId) continue;
    pendingTools.delete(requestId);
    pending.reject(new Error("操作已取消"));
  }
}

/**
 * Forwards an operating-system tool call to Rust and waits for the gateway's
 * answer. Node never executes the action itself.
 */
function requestTool(tool, args, signal, runId) {
  const requestId = randomUUID();
  return new Promise((resolve, reject) => {
    pendingTools.set(requestId, { resolve, reject, runId });
    if (signal) {
      const onAbort = () => {
        if (pendingTools.delete(requestId)) reject(new Error("操作已取消"));
      };
      if (signal.aborted) { onAbort(); return; }
      signal.addEventListener("abort", onAbort, { once: true });
    }
    send({ type: "tool_request", requestId, runId, tool, arguments: args });
  });
}

async function handle(command) {
  if (command.type === "tool_result") {
    const pending = pendingTools.get(command.requestId);
    if (pending) {
      pendingTools.delete(command.requestId);
      if (command.isError) pending.reject(new Error(command.text || "工具执行失败"));
      else pending.resolve({ content: [{ type: "text", text: command.text }], details: command.details });
    }
    return;
  }
  if (command.type === "status") {
    send({ type: "status", agent: process.env.DEEPSEEK_API_KEY ? "ready" : "not_configured", model: model.id });
    return;
  }
  if (command.type === "cancel") {
    if (active?.runId === command.runId) {
      active.cancelled = true;
      abortPending(command.runId);
      getAgent().abort();
    }
    return;
  }
  if (active) { send({ type: "error", runId: command.runId, message: "另一轮对话仍在进行" }); return; }
  if (!process.env.DEEPSEEK_API_KEY) {
    send({ type: "error", runId: command.runId, message: "DeepSeek API Key 尚未由桌面核心配置" });
    return;
  }
  const allowed = Array.isArray(command.tools) && command.tools.length ? command.tools : SAFE_DEFAULT_TOOLS;
  const run = { runId: command.runId, text: "", userText: command.text, interactionMode: command.interactionMode,
    usedTool: false, cancelled: false, timedOut: false, usage: emptyTokenUsage() };
  active = run;
  toolContext = {
    file: command.file,
    location: command.location,
    requestTool: (tool, args, signal) => requestTool(tool, args, signal, run.runId),
  };
  send({ type: "state", state: "thinking", runId: run.runId });
  const timeout = setTimeout(() => {
    run.timedOut = true;
    abortPending(run.runId);
    getAgent().abort();
  }, 120_000);
  try {
    const currentAgent = getAgent();
    currentAgent.state.systemPrompt = systemPromptFor(command.memories, command.interactionMode, allowed, command.folderGrants || []);
    currentAgent.state.tools = toolsForTurn(allowed);
    currentAgent.state.messages = pruneMessages(currentAgent.state.messages);
    await currentAgent.prompt(command.text);
    if (run.timedOut) send({ type: "error", runId: run.runId, message: "模型回复超时；请重试" });
    else if (run.cancelled) send({ type: "cancelled", runId: run.runId });
    else if (agent.state.errorMessage) send({ type: "error", runId: run.runId, message: describeFailure({
      lastAssistant: lastAssistantMessage(agent), errorMessage: agent.state.errorMessage, contextWindow: model.contextWindow }) });
    else {
      const motion = inferReplyMotion({ replyText: run.text, userText: run.userText, usedTool: run.usedTool });
      const displayText = run.text.trim();
      send({ type: "completed", runId: run.runId, reply: {
        display_text: displayText,
        speech_text: buildSpeechText(displayText, run.interactionMode),
        ...motion,
      } });
    }
  } catch (error) {
    send(run.timedOut
      ? { type: "error", runId: run.runId, message: "模型回复超时；请重试" }
      : run.cancelled
      ? { type: "cancelled", runId: run.runId }
      : { type: "error", runId: run.runId, message: describeFailure({
          lastAssistant: lastAssistantMessage(agent), errorMessage: agent?.state?.errorMessage, error, contextWindow: model.contextWindow }) });
  } finally {
    clearTimeout(timeout);
    abortPending(run.runId);
    if (agent && !agent.state.isStreaming) {
      agent.state.messages = command.file || command.location ? [] : pruneMessages(agent.state.messages);
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
lines.on("close", () => {
  abortPending(null);
  if (active) getAgent().abort();
});
