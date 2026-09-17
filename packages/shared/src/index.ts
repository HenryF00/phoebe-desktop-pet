export type AssistantState =
  | "idle" | "listening" | "transcribing" | "thinking"
  | "awaiting_approval" | "tool_running" | "speaking" | "failed";

export type AssistantEvent =
  | { type: "state"; runId: string; state: AssistantState }
  | { type: "usage"; runId: string; usage: TokenUsage }
  | { type: "text_delta"; runId: string; delta: string }
  | { type: "tool_started"; runId: string; tool: ToolName }
  | { type: "tool_progress"; runId: string; detail: string }
  | { type: "approval_required"; runId: string; request: ApprovalRequest }
  | { type: "tool_finished"; runId: string; result: ToolResult }
  | { type: "speech_segment"; runId: string; text: string }
  | { type: "completed"; runId: string; reply: AssistantReply }
  | { type: "cancelled"; runId: string }
  | { type: "error"; runId: string; message: string };

export interface TokenUsage {
  input: number;
  output: number;
  cacheRead: number;
  cacheWrite: number;
  totalTokens: number;
}

/** Estimated tokens of the retained conversation history against its budget. */
export interface ContextUsage {
  estimated: number;
  budget: number;
}

export type ToolName =
  | "get_current_time" | "get_system_status" | "web_search" | "read_selected_file" | "get_device_location" | "launch_wuthering_waves"
  | "launch_application" | "open_url" | "focus_application"
  | "list_granted_folders" | "list_directory" | "read_text_file" | "search_files"
  | "write_file" | "move_file" | "delete_file"
  | "list_installed_apps" | "reveal_file" | "open_file_with_application"
  | "browser_open" | "browser_snapshot" | "browser_click" | "browser_type" | "browser_select"
  | "browser_wait" | "browser_extract_text" | "browser_close"
  | "remember_preference" | "forget_preference";

export interface ToolCall {
  id: string;
  name: ToolName;
  arguments: Record<string, unknown>;
}

export interface ApprovalRequest {
  id: string;
  runId: string;
  tool: ToolName;
  target: string;
  impact: string;
  scope: "once" | "bound_target";
  rememberable: boolean;
  /** Optional bounded preview, such as a write diff. Shown to the user only. */
  preview?: string;
}

/** A folder the user granted through the native directory picker. */
export interface FolderGrantView {
  id: string;
  path: string;
  label: string;
  read: boolean;
  write: boolean;
  createdAt: string;
}

/** User answer to an {@link ApprovalRequest}. */
export type ApprovalDecision = "deny" | "allow_once" | "always";

/** Whether the Agent may request operating-system actions at all. */
export type ActionMode = "disabled" | "standard" | "trust";

/** One mediated operation recorded by the Rust tool gateway. */
export interface AuditEntry {
  timestamp: string;
  runId: string | null;
  tool: string;
  target: string;
  decision: "auto" | "allow_trusted" | "allow_once" | "allow_always" | "deny";
  outcome: "success" | "failed" | "denied";
  message: string;
}

export interface ToolResult {
  status: "success" | "already_running" | "unconfigured" | "unsupported" | "denied" | "failed";
  message: string;
}

export interface AssistantReply {
  display_text: string;
  speech_text: string;
  emotion: "calm" | "happy" | "shy" | "concerned";
  gesture: "idle" | "nod" | "point";
}

export interface VoiceTranscription {
  text: string;
  language: string;
  duration_ms: number;
  confidence: number | null;
}

export type VoiceRequest =
  | { type: "transcribe"; requestId: string; recordingId: string }
  | { type: "synthesize"; requestId: string; text: string; voiceId: string }
  | { type: "cancel"; requestId: string };

export interface SystemStatus {
  platform: string;
  agent: "not_configured" | "ready" | "failed";
  voice: "not_configured" | "starting" | "ready" | "failed";
  version: string;
}

const autoTools = new Set<ToolName>(["get_current_time", "get_system_status"]);

export function isAutoAllowedTool(name: string): boolean {
  return autoTools.has(name as ToolName);
}

export function isAllowedHttpsUrl(value: string): boolean {
  try {
    const url = new URL(value);
    return url.protocol === "https:" && Boolean(url.hostname) && !url.username && !url.password;
  } catch { return false; }
}
