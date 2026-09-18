import React from "react";
import { createRoot } from "react-dom/client";
import Markdown from "react-markdown";
import remarkGfm from "remark-gfm";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import type { ActionMode, ApprovalDecision, ApprovalRequest, AssistantEvent, AssistantReply, AuditEntry, ContextUsage, FolderGrantView, SystemStatus, TokenUsage, ToolResult } from "@phoebe/shared";
import { MemorySection } from "./MemorySection";
import { selectPetClip, usePetAnimation, useReducedMotion } from "./petAnimation";
import "./style.css";

const inTauri = "__TAURI_INTERNALS__" in window;
const view = inTauri ? getCurrentWindow().label : new URLSearchParams(location.search).get("view") || "pet";

type ChatEntry = {
  runId: string;
  question: string;
  answer: string;
  speechText?: string;
  emotion?: AssistantReply["emotion"];
  gesture?: AssistantReply["gesture"];
  phase: "thinking" | "completed" | "cancelled" | "failed";
  error?: string;
  fromHistory?: boolean;
  tool?: {
    name: string;
    status: "running" | ToolResult["status"];
    message?: string;
  };
};

type AgentTextEvent = Extract<AssistantEvent, { type: "state" | "text_delta" | "tool_started" | "tool_finished" | "completed" | "cancelled" | "error" }>;
type HistoryTurn = { runId: string; question: string; answer: string };
type HistoryOutcome = { runId: string; status: "saved" | "private" | "failed" };
type Conversation = { id: string; title: string; updatedAt: string; turnCount: number };
type ActiveConversation = { conversation: Conversation; turns: HistoryTurn[] };

type InteractionMode = "assistant" | "chat";
type DesktopSettings = { version: number; model: string; privacy_mode: boolean; search_proxy: string; voice_enabled: boolean; interaction_mode: InteractionMode; action_mode: ActionMode; pet_scale_percent: number; birthday: string | null; user_address: string };
type QuickPreferences = Pick<DesktopSettings, "interaction_mode" | "voice_enabled" | "pet_scale_percent">;
type TokenUsageSnapshot = { current: TokenUsage; session: TokenUsage };
type AutostartStatus = { enabled: boolean; available: boolean };
type VoiceDiagnostic = { status: "ready" | "failed"; message: string; endpoint: string; model: string; reference: string };
type VoiceEvent = {
  generation: number;
  runId?: string;
  state: "preparing" | "synthesizing" | "speaking" | "idle" | "stopped" | "failed";
  message: string;
  emotion?: AssistantReply["emotion"];
  gesture?: AssistantReply["gesture"];
};

const defaultSettings: DesktopSettings = { version: 1, model: "deepseek-v4-flash", privacy_mode: false, search_proxy: "", voice_enabled: true, interaction_mode: "assistant", action_mode: "standard", pet_scale_percent: 100, birthday: null, user_address: "漂泊者" };
const defaultQuickPreferences: QuickPreferences = { interaction_mode: "assistant", voice_enabled: true, pet_scale_percent: 100 };
const emptyTokenUsage: TokenUsage = { input: 0, output: 0, cacheRead: 0, cacheWrite: 0, totalTokens: 0 };
const defaultTokenUsage: TokenUsageSnapshot = { current: emptyTokenUsage, session: emptyTokenUsage };
const defaultContextUsage: ContextUsage = { estimated: 0, budget: 24_000 };
const PET_SCALE_PRESETS = [
  { value: 0, label: "图标" },
  { value: 75, label: "75%" },
  { value: 100, label: "100%" },
  { value: 125, label: "125%" },
  { value: 150, label: "150%" },
] as const;
const AUTO_WAVE_IDLE_MS = 120_000;

function formatTokenCount(value: number) {
  return new Intl.NumberFormat("zh-CN", { notation: value >= 10_000 ? "compact" : "standard", maximumFractionDigits: 1 }).format(value);
}

function formatContextCount(value: number) {
  return value >= 1000 ? `${(value / 1000).toFixed(value % 1000 === 0 ? 0 : 1)}k` : String(value);
}

type IconProps = React.SVGProps<SVGSVGElement>;

function Icon({ children, ...props }: IconProps) {
  return <svg viewBox="0 0 24 24" width="20" height="20" fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true" {...props}>{children}</svg>;
}

const CloseIcon = () => <Icon><path d="m6 6 12 12M18 6 6 18" /></Icon>;
const MoreIcon = () => <Icon><circle cx="5" cy="12" r="1" fill="currentColor" stroke="none" /><circle cx="12" cy="12" r="1" fill="currentColor" stroke="none" /><circle cx="19" cy="12" r="1" fill="currentColor" stroke="none" /></Icon>;
const PlusIcon = () => <Icon><path d="M12 5v14M5 12h14" /></Icon>;
const SendIcon = () => <Icon><path d="m5 12 14-7-4 14-3-6-7-1Z" /><path d="m12 13 7-8" /></Icon>;
const StopIcon = () => <Icon><rect x="7" y="7" width="10" height="10" rx="2" fill="currentColor" stroke="none" /></Icon>;
const FileIcon = () => <Icon><path d="M7 3h7l4 4v14H7z" /><path d="M14 3v5h5M10 13h5M10 17h5" /></Icon>;
const ImageIcon = () => <Icon><rect x="3" y="4" width="18" height="16" rx="2" /><circle cx="8.5" cy="9.5" r="1.6" /><path d="m5 18 5-5 3.5 3.5L17 13l2 2" /></Icon>;
const InfoIcon = () => <Icon><circle cx="12" cy="12" r="9" /><path d="M12 11v6M12 7.5h.01" /></Icon>;
const FolderIcon = () => <Icon><path d="M3 6h6l2 2h10v10H3z" /></Icon>;
const ChevronDownIcon = () => <Icon><path d="m7 10 5 5 5-5" /></Icon>;
const ToolIcon = () => <Icon><path d="m14.7 6.3 3-3a4 4 0 0 1-5.2 5.2L6 15l-3 1 1-3 6.5-6.5a4 4 0 0 1 5.2-5.2l-3 3 2 2Z" /></Icon>;
const DownIcon = () => <Icon><path d="m7 9 5 5 5-5" /></Icon>;
const ChatMenuIcon = () => <Icon><path d="M5 5h14v11H9l-4 3Z" /><path d="M9 9h6M9 12h4" /></Icon>;
const WaveMenuIcon = () => <Icon><path d="M8.5 12V5.5a1.5 1.5 0 0 1 3 0V10M11.5 10V4.5a1.5 1.5 0 0 1 3 0V10M14.5 10V6a1.5 1.5 0 0 1 3 0v6.5M8.5 9.5 6.8 7.8a1.6 1.6 0 0 0-2.3 2.2l4.4 7A5.5 5.5 0 0 0 19 14v-2" /></Icon>;
const SettingsMenuIcon = () => <Icon><circle cx="12" cy="12" r="3" /><path d="M19.4 15a1.7 1.7 0 0 0 .3 1.9l.1.1-2.8 2.8-.1-.1a1.7 1.7 0 0 0-1.9-.3 1.7 1.7 0 0 0-1 1.5V21h-4v-.1a1.7 1.7 0 0 0-1-1.5 1.7 1.7 0 0 0-1.9.3l-.1.1L4.2 17l.1-.1a1.7 1.7 0 0 0 .3-1.9A1.7 1.7 0 0 0 3.1 14H3v-4h.1a1.7 1.7 0 0 0 1.5-1 1.7 1.7 0 0 0-.3-1.9L4.2 7 7 4.2l.1.1a1.7 1.7 0 0 0 1.9.3 1.7 1.7 0 0 0 1-1.5V3h4v.1a1.7 1.7 0 0 0 1 1.5 1.7 1.7 0 0 0 1.9-.3l.1-.1L19.8 7l-.1.1a1.7 1.7 0 0 0-.3 1.9 1.7 1.7 0 0 0 1.5 1h.1v4h-.1a1.7 1.7 0 0 0-1.5 1Z" /></Icon>;
const HideMenuIcon = () => <Icon><path d="M3 12s3.2-5 9-5 9 5 9 5-3.2 5-9 5-9-5-9-5Z" /><path d="m4 4 16 16" /></Icon>;
const PowerMenuIcon = () => <Icon><path d="M12 3v9" /><path d="M7.5 5.8a8 8 0 1 0 9 0" /></Icon>;
const AssistantModeIcon = () => <Icon><circle cx="12" cy="8" r="3" /><path d="M6.5 20v-2.5A5.5 5.5 0 0 1 12 12a5.5 5.5 0 0 1 5.5 5.5V20M4 9V7a3 3 0 0 1 3-3h1M20 9V7a3 3 0 0 0-3-3h-1" /></Icon>;
const ChatModeIcon = () => <Icon><path d="M4 5h16v11H9l-5 4Z" /><path d="M8 9h8M8 12h5" /></Icon>;
const VoiceMenuIcon = () => <Icon><path d="M5 10v4h3l4 4V6L8 10Z" /><path d="M16 9a4 4 0 0 1 0 6M18.5 6.5a7.5 7.5 0 0 1 0 11" /></Icon>;
const CheckIcon = () => <Icon><path d="m6.5 12.5 3.5 3.5 7.5-8" /></Icon>;
const PlayIcon = () => <Icon><path d="m9 7 8 5-8 5Z" fill="currentColor" stroke="none" /></Icon>;

const toolLabels: Record<string, string> = {
  get_current_time: "读取当前时间",
  get_system_status: "检查系统状态",
  web_search: "搜索网页",
  read_selected_file: "读取所选文件",
  get_device_location: "获取设备位置",
  list_granted_folders: "查看授权文件夹",
  list_directory: "列出目录",
  read_text_file: "读取文本文件",
  search_files: "搜索文件",
  write_file: "写入文件",
  move_file: "移动文件",
  delete_file: "删除到废纸篓",
  list_installed_apps: "查看已安装应用",
  reveal_file: "在文件管理器中显示",
  open_file_with_application: "用指定应用打开",
  launch_wuthering_waves: "启动应用",
  launch_application: "启动应用",
  open_url: "打开链接",
  focus_application: "切换应用",
  browser_open: "打开受控浏览器",
  browser_snapshot: "浏览器页面快照",
  browser_click: "点击页面元素",
  browser_type: "向页面输入",
  browser_select: "选择下拉项",
  browser_wait: "等待页面加载",
  browser_extract_text: "提取页面文本",
  browser_close: "关闭受控浏览器",
  read_system_file: "读取系统文件",
  write_system_file: "写入系统文件",
  remember_preference: "保存偏好",
  forget_preference: "移除偏好",
};

function readAsDataURL(file: File | Blob): Promise<string> {
  return new Promise((resolve, reject) => {
    const reader = new FileReader();
    reader.onload = () => resolve(String(reader.result));
    reader.onerror = () => reject(new Error("read failed"));
    reader.readAsDataURL(file);
  });
}

function loadImageElement(src: string): Promise<HTMLImageElement> {
  return new Promise((resolve, reject) => {
    const image = new Image();
    image.onload = () => resolve(image);
    image.onerror = () => reject(new Error("image decode failed"));
    image.src = src;
  });
}

/** Draw any image/video source into a downscaled JPEG and return its base64. */
function drawFrameJpeg(source: HTMLImageElement | HTMLVideoElement, maxEdge = 1536, quality = 0.9): string {
  const sourceWidth = (source as HTMLVideoElement).videoWidth || (source as HTMLImageElement).naturalWidth || 1;
  const sourceHeight = (source as HTMLVideoElement).videoHeight || (source as HTMLImageElement).naturalHeight || 1;
  const scale = Math.min(1, maxEdge / Math.max(sourceWidth, sourceHeight));
  const width = Math.max(1, Math.round(sourceWidth * scale));
  const height = Math.max(1, Math.round(sourceHeight * scale));
  const canvas = document.createElement("canvas");
  canvas.width = width;
  canvas.height = height;
  const context = canvas.getContext("2d");
  if (!context) throw new Error("canvas unavailable");
  context.drawImage(source, 0, 0, width, height);
  return canvas.toDataURL("image/jpeg", quality).split(",")[1] ?? "";
}

async function framesFromImageFile(file: File): Promise<{ mimeType: string; data: string }[]> {
  const image = await loadImageElement(await readAsDataURL(file));
  return [{ mimeType: "image/jpeg", data: drawFrameJpeg(image) }];
}

async function framesFromVideoFile(file: File, count = 4): Promise<{ mimeType: string; data: string }[]> {
  const url = URL.createObjectURL(file);
  try {
    const video = document.createElement("video");
    video.muted = true;
    video.preload = "auto";
    video.src = url;
    await new Promise<void>((resolve, reject) => {
      video.onloadedmetadata = () => resolve();
      video.onerror = () => reject(new Error("video load failed"));
    });
    const duration = Number.isFinite(video.duration) && video.duration > 0 ? video.duration : 0;
    const frames: { mimeType: string; data: string }[] = [];
    for (let index = 0; index < count; index++) {
      const target = duration > 0 ? (duration * (index + 0.5)) / count : 0;
      await new Promise<void>((resolve, reject) => {
        const onSeeked = () => { video.removeEventListener("seeked", onSeeked); resolve(); };
        video.addEventListener("seeked", onSeeked);
        video.onerror = () => reject(new Error("video seek failed"));
        video.currentTime = Math.min(Math.max(target, 0), Math.max(duration - 0.05, 0));
      });
      frames.push({ mimeType: "image/jpeg", data: drawFrameJpeg(video) });
    }
    return frames.length ? frames : [{ mimeType: "image/jpeg", data: drawFrameJpeg(video) }];
  } finally {
    URL.revokeObjectURL(url);
  }
}

function MessageMarkdown({ children }: { children: string }) {
  return <Markdown
    remarkPlugins={[remarkGfm]}
    components={{
      a: ({ children: label, href }) => <a href={href} target="_blank" rel="noreferrer">{label}</a>,
      img: ({ alt }) => <span className="markdown-image-note">{alt ? `图片：${alt}` : "图片链接"}</span>,
    }}
  >{children}</Markdown>;
}

function SettingsApp() {
  const [saved, setSaved] = React.useState<DesktopSettings>(defaultSettings);
  const [draft, setDraft] = React.useState<DesktopSettings>(defaultSettings);
  const [status, setStatus] = React.useState<SystemStatus | null>(null);
  const [autostart, setAutostart] = React.useState<AutostartStatus>({ enabled: false, available: false });
  const [loading, setLoading] = React.useState(inTauri);
  const [saving, setSaving] = React.useState(false);
  const [autostartBusy, setAutostartBusy] = React.useState(false);
  const [notice, setNotice] = React.useState("");
  const [loadError, setLoadError] = React.useState(false);
  const [memoryDirty, setMemoryDirty] = React.useState(false);
  const [keyDirty, setKeyDirty] = React.useState(false);
  const [keyBusy, setKeyBusy] = React.useState(false);
  const [keyNotice, setKeyNotice] = React.useState("");
  const [keyError, setKeyError] = React.useState("");
  const [voiceBusy, setVoiceBusy] = React.useState(false);
  const [voiceDiagnostic, setVoiceDiagnostic] = React.useState<VoiceDiagnostic | null>(null);
  const [memoryResetToken, setMemoryResetToken] = React.useState(0);
  const [audit, setAudit] = React.useState<AuditEntry[]>([]);
  const [auditNotice, setAuditNotice] = React.useState("");
  const [grants, setGrants] = React.useState<FolderGrantView[]>([]);
  const [grantNotice, setGrantNotice] = React.useState("");
  const headingRef = React.useRef<HTMLHeadingElement>(null);
  const closeRef = React.useRef<HTMLButtonElement>(null);
  const keyRef = React.useRef<HTMLInputElement>(null);
  const dirtyRef = React.useRef(false);
  const keyBusyRef = React.useRef(false);
  const savedRef = React.useRef(saved);

  const changed = draft.model !== saved.model || draft.privacy_mode !== saved.privacy_mode
    || draft.search_proxy !== saved.search_proxy || draft.voice_enabled !== saved.voice_enabled
    || draft.interaction_mode !== saved.interaction_mode || draft.action_mode !== saved.action_mode
    || (draft.birthday ?? "") !== (saved.birthday ?? "") || draft.user_address !== saved.user_address;
  dirtyRef.current = changed || memoryDirty || keyDirty;
  keyBusyRef.current = keyBusy;
  savedRef.current = saved;

  React.useEffect(() => {
    if (!inTauri) { headingRef.current?.focus(); return; }
    let disposed = false;
    async function refresh() {
      setLoading(true);
      try {
        const next = await invoke<DesktopSettings>("get_settings");
        if (disposed) return;
        setSaved(next); setDraft(next);
        setLoadError(false); setNotice("");
        void invoke<SystemStatus>("get_system_status").then(value => { if (!disposed) setStatus(value); }).catch(() => { if (!disposed) setStatus(null); });
        void invoke<AutostartStatus>("get_autostart_status").then(value => { if (!disposed) setAutostart(value); }).catch(() => { if (!disposed) setNotice("无法刷新系统登录项状态。"); });
        void invoke<AuditEntry[]>("get_audit_log", { limit: 10 }).then(value => { if (!disposed) setAudit(value); }).catch(() => { if (!disposed) setAudit([]); });
        void invoke<FolderGrantView[]>("list_folder_grants").then(value => { if (!disposed) setGrants(value); }).catch(() => { if (!disposed) setGrants([]); });
      } catch {
        if (disposed) return;
        setLoadError(true);
        setNotice("无法读取设置文件。可用下方默认选项重新保存以修复；若反复失败，请检查应用数据目录权限。");
      } finally { if (!disposed) { setLoading(false); headingRef.current?.focus(); } }
    }
    void refresh();
    let unlisten: (() => void) | null = null;
    void listen<boolean>("settings-visibility", event => {
      if (!event.payload) return;
      closeRef.current?.focus();
      void invoke<SystemStatus>("get_system_status").then(setStatus).catch(() => setStatus(null));
      void invoke<AutostartStatus>("get_autostart_status").then(setAutostart).catch(() => setNotice("无法刷新系统登录项状态。"));
    })
      .then(fn => { if (disposed) fn(); else unlisten = fn; });
    return () => { disposed = true; unlisten?.(); };
  }, []);

  React.useEffect(() => {
    if (!inTauri) return;
    let disposed = false;
    let unlisten: (() => void) | null = null;
    void listen("agent-key-status", () => {
      void invoke<SystemStatus>("get_system_status").then(next => {
        if (!disposed) setStatus(next);
      }).catch(() => { if (!disposed) setKeyError("无法刷新密钥状态。请重新打开设置。"); });
    }).then(fn => { if (disposed) fn(); else unlisten = fn; });
    return () => { disposed = true; unlisten?.(); };
  }, []);

  React.useEffect(() => {
    if (!inTauri) return;
    let disposed = false;
    let unlisten: (() => void) | null = null;
    void listen<QuickPreferences>("quick-preferences-changed", event => {
      if (disposed) return;
      setSaved(current => ({ ...current, ...event.payload }));
      setDraft(current => ({ ...current, ...event.payload }));
    }).then(fn => { if (disposed) fn(); else unlisten = fn; });
    return () => { disposed = true; unlisten?.(); };
  }, []);

  React.useEffect(() => {
    if (!inTauri) return;
    let disposed = false;
    let unlisten: (() => void) | null = null;
    void getCurrentWindow().onCloseRequested(event => {
      event.preventDefault();
      void closeSettings();
    }).then(fn => { if (disposed) fn(); else unlisten = fn; })
      .catch(() => setNotice("系统关闭键监听不可用；请用页面内的关闭按钮。"));
    return () => { disposed = true; unlisten?.(); };
  }, []);

  async function saveSettings(event: React.FormEvent<HTMLFormElement>) {
    event.preventDefault();
    if (!inTauri || saving || loading || (!changed && !loadError)) return;
    setSaving(true); setNotice("");
    try {
      const next = await invoke<DesktopSettings>("update_settings", {
        model: draft.model,
        privacyMode: draft.privacy_mode,
        searchProxy: draft.search_proxy.trim(),
        voiceEnabled: draft.voice_enabled,
        interactionMode: draft.interaction_mode,
        actionMode: draft.action_mode,
        birthday: (draft.birthday ?? "").trim() || null,
        userAddress: draft.user_address,
      });
      setSaved(next); setDraft(next); setLoadError(false);
      void refreshAudit();
      setNotice("设置已保存。新模型会从下一轮对话起使用；当前对话不会被打断。");
    } catch { setNotice("保存失败。请检查应用数据目录权限，然后重试。"); }
    finally { setSaving(false); }
  }

  async function changeAutostart(enabled: boolean) {
    if (!inTauri || !autostart.available || autostartBusy) return;
    setAutostartBusy(true); setNotice("");
    try {
      setAutostart(await invoke<AutostartStatus>("set_autostart", { enabled }));
      setNotice(enabled ? "已登记开机启动。登录后会恢复桌宠，并在语音开启时后台预热内置声音。" : "已关闭开机启动。");
    } catch { setNotice("无法更新系统登录项。请检查系统权限后重试，原状态保持不变。"); }
    finally { setAutostartBusy(false); }
  }

  async function revokeGrant(id: string) {
    if (!inTauri) return;
    try {
      await invoke("revoke_folder_grant", { grantId: id });
      setGrants(current => current.filter(grant => grant.id !== id));
      setGrantNotice("已撤销该文件夹授权。");
    } catch { setGrantNotice("撤销失败，请重试。"); }
  }

  async function refreshAudit() {
    if (!inTauri) return;
    try { setAudit(await invoke<AuditEntry[]>("get_audit_log", { limit: 10 })); setAuditNotice(""); }
    catch { setAudit([]); setAuditNotice("无法读取审计记录。"); }
  }

  async function panicStop() {
    if (!inTauri) return;
    try {
      await invoke("panic_stop_operations");
      setAuditNotice("已停止当前操作并拒绝所有待审批项。");
      void refreshAudit();
    } catch { setAuditNotice("停止请求失败，请从托盘退出后重启应用。"); }
  }

  async function saveKey(event: React.FormEvent<HTMLFormElement>) {
    event.preventDefault();
    if (!inTauri || keyBusy) return;
    const key = keyRef.current?.value.trim() || "";
    if (!key || new TextEncoder().encode(key).length > 4096) {
      setKeyError("请输入不超过 4096 字节的 API Key。");
      return;
    }
    setKeyBusy(true); setKeyError(""); setKeyNotice("");
    if (keyRef.current) keyRef.current.value = "";
    setKeyDirty(false);
    try {
      await invoke("set_deepseek_api_key", { key });
      setStatus(current => current ? { ...current, agent: "ready" } : current);
      void invoke<SystemStatus>("get_system_status").then(setStatus).catch(() => setKeyNotice("密钥已保存，但状态刷新失败；请重新打开设置。"));
      setKeyNotice("已保存在本机系统安全存储。下一轮对话会使用新密钥。");
    } catch { setKeyError("保存失败。请检查系统安全存储权限，并重新打开设置确认当前状态。"); }
    finally { setKeyBusy(false); }
  }

  async function checkVoice() {
    if (!inTauri || voiceBusy) return;
    setVoiceBusy(true);
    try {
      const diagnostic = await invoke<VoiceDiagnostic>("check_voice_service");
      setVoiceDiagnostic(diagnostic);
      setStatus(current => current ? { ...current, voice: diagnostic.status } : current);
    } catch {
      setVoiceDiagnostic({ status: "failed", message: "无法检查本机语音服务。", endpoint: "http://127.0.0.1:9880/tts", model: "GPT e15 · SoVITS e8", reference: "021_自我介绍.wav" });
    } finally { setVoiceBusy(false); }
  }

  async function closeSettings() {
    if (keyBusyRef.current) { setKeyNotice("请等待密钥保存完成后再关闭设置。"); return; }
    if (dirtyRef.current) {
      if (!window.confirm("有尚未保存的设置或记忆草稿，仍要关闭吗？")) return;
      setDraft(savedRef.current);
      setMemoryResetToken(current => current + 1);
      dirtyRef.current = false;
    }
    if (keyRef.current) keyRef.current.value = "";
    setKeyDirty(false); setKeyError(""); setKeyNotice("");
    if (!inTauri) { location.search = "?view=pet"; return; }
    try { await invoke("hide_settings"); }
    catch { setNotice("无法关闭设置窗；可以使用窗口标题栏的关闭按钮。"); }
  }

  const credentialTone = !inTauri ? "preview" : status?.agent === "ready" ? "ready" : status?.agent === "failed" ? "error" : "empty";
  const credentialLabel = !inTauri ? "浏览器预览" : status?.agent === "ready" ? "已配置" : status?.agent === "failed" ? "安全存储不可用" : "尚未配置";

  return <main className="settings-shell" onKeyDown={event => { if (event.key === "Escape") { event.stopPropagation(); void closeSettings(); } }}>
    <section className="settings-card" aria-label="菲比助手设置">
      <header className="settings-header"><div className="settings-brand"><span className="settings-avatar" aria-hidden="true"><img src="/pet/phoebe-avatar-v2.png" alt="" /></span><div><p className="eyebrow">PHOEBE ASSISTANT · LOCAL SETTINGS</p><h1 ref={headingRef} tabIndex={-1}>设置</h1><p>模型、隐私和启动行为由桌面核心管理。</p></div></div>
        <button ref={closeRef} className="settings-close" onClick={() => void closeSettings()} aria-label="关闭设置窗">完成</button></header>
      <div className="settings-body">
        {!inTauri && <p className="settings-callout">浏览器仅预览界面；保存与系统登录项需在桌面应用中操作。</p>}
        {loading && <p className="settings-callout" role="status">正在读取本机设置…</p>}
        <form id="settings-form" onSubmit={saveSettings}>
          <fieldset className="settings-group" disabled={loading || saving}>
            <legend>对话模型</legend>
            <label htmlFor="model">DeepSeek 模型</label>
            <select id="model" value={draft.model} onChange={event => setDraft(current => ({ ...current, model: event.target.value }))}>
              <option value="deepseek-v4-flash">DeepSeek V4 Flash · 默认</option>
              <option value="deepseek-v4-pro">DeepSeek V4 Pro</option>
              <option value="deepseek-v4-flash-vision-exp">DeepSeek V4 Flash Vision · 支持图像识别</option>
            </select>
            <p className="field-help">图像识别需选择 Vision 模型；文字对话从下一轮起使用所选模型。</p>
            <p className="field-help">API Key 可在下方单独配置，不随模型和隐私设置保存到文件。</p>
          </fieldset>
          <fieldset className="settings-group" disabled={loading || saving}>
            <legend>隐私</legend>
            <label className="setting-switch"><input type="checkbox" checked={draft.privacy_mode} onChange={event => setDraft(current => ({ ...current, privacy_mode: event.target.checked }))} /><span><strong>隐私模式</strong><small>开启后不保存新的已完成对话；明确修改的设置仍保留。</small></span></label>
            <p className="field-help">隐私模式不会自动清理此前已保存的历史；可在聊天窗手动清理。未保存的对话仍只在内存中。</p>
          </fieldset>
          <fieldset className="settings-group" disabled={loading || saving}>
            <legend>操作权限</legend>
            <label htmlFor="action-mode">本机操作策略</label>
            <select id="action-mode" value={draft.action_mode} onChange={event => setDraft(current => ({ ...current, action_mode: event.target.value as ActionMode }))}>
              <option value="disabled">关闭 · 不向 Agent 注册操作工具</option>
              <option value="standard">标准 · 敏感操作逐次确认（默认）</option>
              <option value="trust">信任 · 已授权网站免重复确认</option>
            </select>
            <p className="field-help">聊天模式始终不注册操作工具。操作策略只决定应用内是否逐次确认，不能绕过 macOS 辅助功能、屏幕录制、文件与钥匙串等系统权限。信任模式目前只对“始终允许”过的网站生效。</p>
          </fieldset>
          <fieldset className="settings-group"><legend>已授权文件夹</legend>
            <p className="field-help">这些文件夹由你通过系统选择器授权；菲比只能使用其中的相对路径，既看不到绝对路径，也无法访问未授权位置。当前仅供只读。</p>
            <div className="grant-list" role="list">
              {grants.length === 0 && <p className="field-help">尚未授权任何文件夹。可在聊天窗的“＋”菜单中选择“授权文件夹”。</p>}
              {grants.map(grant => <div className="grant-row" role="listitem" key={grant.id}>
                <span className="grant-label">{grant.label}</span>
                <span className="grant-path" title={grant.path}>{grant.path}</span>
                <span className="grant-perm">{grant.read ? "只读" : ""}{grant.write ? `${grant.read ? " · " : ""}可写` : ""}</span>
                <button type="button" className="is-danger" onClick={() => void revokeGrant(grant.id)}>撤销</button>
              </div>)}
            </div>
            {grantNotice && <p className="field-help" role="status">{grantNotice}</p>}
          </fieldset>
          <fieldset className="settings-group" disabled={loading || saving}>
            <legend>联网搜索</legend>
            <label htmlFor="search-proxy">本机搜索代理（可选）</label>
            <input id="search-proxy" type="url" value={draft.search_proxy} placeholder="http://127.0.0.1:7897" autoComplete="off" spellCheck={false}
              onChange={event => setDraft(current => ({ ...current, search_proxy: event.target.value }))} />
            <p className="field-help">直连可用时留空；若本机网络必须经过代理，填写已信任的本机 HTTP 代理地址。只允许 127.0.0.1 或 localhost，保存在本机应用数据目录，不上传到 GitHub。</p>
          </fieldset>
          <fieldset className="settings-group" disabled={loading || saving}>
            <legend>个人</legend>
            <label htmlFor="user-address">菲比对你的称呼</label>
            <input id="user-address" type="text" value={draft.user_address} placeholder="漂泊者" maxLength={24} autoComplete="off" spellCheck={false}
              onChange={event => setDraft(current => ({ ...current, user_address: event.target.value }))} />
            <p className="field-help">默认“漂泊者”；菲比会在合适的场合这样称呼你，留空则使用默认。</p>
            <label htmlFor="birthday">生日（可选）</label>
            <input id="birthday" type="text" value={draft.birthday ?? ""} placeholder="MM-DD，例如 03-15" maxLength={5} autoComplete="off" spellCheck={false}
              onChange={event => setDraft(current => ({ ...current, birthday: event.target.value }))} />
            <p className="field-help">填写后，生日当天首次对话菲比会主动送上祝福（每天一次）。格式 MM-DD，留空关闭。</p>
          </fieldset>
        </form>
        <form className="key-form" onSubmit={saveKey}>
          <fieldset className="settings-group" disabled={!inTauri || keyBusy}>
            <legend>DeepSeek API Key</legend>
            <div className="settings-detail key-status"><span>本机凭证</span><strong className={`status-pill is-${credentialTone}`}><span aria-hidden="true" />{credentialLabel}</strong></div>
            <label htmlFor="deepseek-key">输入新密钥{status?.agent === "ready" ? "（替换现有密钥）" : ""}</label>
            <div className="key-controls">
              <input id="deepseek-key" ref={keyRef} type="password" maxLength={4096} autoComplete="off" autoCapitalize="off" spellCheck={false}
                onChange={event => { setKeyDirty(Boolean(event.currentTarget.value)); setKeyError(""); setKeyNotice(""); }}
                aria-invalid={Boolean(keyError)} aria-describedby={keyError ? "key-error" : "key-help"} placeholder="粘贴 DeepSeek API Key" />
              <button type="submit" disabled={!keyDirty || keyBusy}>{keyBusy ? "保存中…" : "保存密钥"}</button>
            </div>
            <p id="key-help" className="field-help">只写入本机系统钥匙串／凭证管理器；不保存在项目文件、聊天历史或安装包中，且不会回显。</p>
            {keyError && <p id="key-error" className="key-error" role="alert">{keyError}</p>}
            {keyNotice && <p className="key-notice" role="status">{keyNotice}</p>}
          </fieldset>
        </form>
        <fieldset className="settings-group"><legend>启动</legend>
          <label className="setting-switch"><input type="checkbox" checked={autostart.enabled} onChange={event => void changeAutostart(event.target.checked)} disabled={!inTauri || !autostart.available || autostartBusy || loading} /><span><strong>开机启动</strong><small>{autostart.available ? "登录时恢复桌宠；语音开启时会在后台预热内置声音。" : "当前构建未生成正式安装包；登录项暂不可用。"}</small></span></label>
        </fieldset>
        <MemorySection onDirtyChange={setMemoryDirty} resetToken={memoryResetToken} />
        <fieldset className="settings-group"><legend>操作审计</legend>
          <p className="field-help">被网关执行或拒绝的操作都会记录在本机 audit.jsonl；这里显示最近 10 条。</p>
          <div className="audit-list" role="list">
            {audit.length === 0 && <p className="field-help">还没有本机操作记录。</p>}
            {audit.map((entry, index) => <div className="audit-row" role="listitem" key={`${entry.timestamp}-${index}`}>
              <span className={`audit-badge is-${entry.outcome}`}>{entry.outcome === "success" ? "成功" : entry.outcome === "denied" ? "拒绝" : "失败"}</span>
              <span className="audit-tool">{toolLabels[entry.tool] ?? entry.tool}</span>
              <span className="audit-target" title={entry.target}>{entry.target || "—"}</span>
            </div>)}
          </div>
          <div className="audit-actions">
            <button type="button" onClick={() => void refreshAudit()}>刷新记录</button>
            <button type="button" className="is-danger" onClick={() => void panicStop()} disabled={!inTauri}>立即停止所有操作</button>
          </div>
          {auditNotice && <p className="field-help" role="status">{auditNotice}</p>}
        </fieldset>
        <fieldset className="settings-group"><legend>菲比语音</legend>
          <label className="setting-switch"><input type="checkbox" checked={draft.voice_enabled}
            onChange={event => setDraft(current => ({ ...current, voice_enabled: event.target.checked }))} disabled={loading || saving} />
            <span><strong>自动播放菲比语音</strong><small>{draft.interaction_mode === "assistant" ? "助手模式只播报简短结论；详细内容保留在聊天窗口。" : "聊天模式保持简短回答，并完整朗读自然语言内容。"}</small></span></label>
          <div className="voice-baseline"><span><strong>基线</strong><small>GPT e15 · SoVITS e8 · 中文 · 凑四句一切</small></span>
            <button type="button" onClick={() => void checkVoice()} disabled={!inTauri || voiceBusy}>{voiceBusy ? "检测中…" : "检测服务"}</button></div>
          <p className={`voice-diagnostic ${voiceDiagnostic?.status === "ready" || (!voiceDiagnostic && status?.voice === "ready") ? "is-ready" : voiceDiagnostic?.status === "failed" ? "is-error" : ""}`} role="status">
            {voiceDiagnostic?.message || (status?.voice === "ready" ? "内置菲比语音已就绪。" : status?.voice === "starting" ? "正在后台准备内置菲比语音，首次启动需要稍候。" : "安装包会自动准备并启动内置菲比语音；首次使用需要解压模型。")}
          </p>
          <p className="field-help">不会朗读 Markdown 标记、网址、代码或文件路径。安装包包含 GPT-SoVITS 推理环境、GPT e15、SoVITS e8 和参考音频；首次启动会解压到本机应用数据目录。关闭此开关不影响文字聊天。</p>
        </fieldset>
        <fieldset className="settings-group is-pending" disabled><legend>角色模型</legend><p>本机角色图已加入眨眼、轻摆和挥手原型；Live2D 分层模型仍未接入。</p></fieldset>
      </div>
      <footer className="settings-footer" data-dirty={changed || memoryDirty || keyDirty}><p role="status" aria-live="polite">{notice || (changed ? "有尚未保存的更改" : "设置已同步")}</p>
        <button form="settings-form" type="submit" disabled={!inTauri || loading || saving || (!changed && !loadError)}>{saving ? "保存中…" : loadError ? "恢复默认设置" : "保存设置"}</button></footer>
    </section>
  </main>;
}

function ChatApp() {
  const [status, setStatus] = React.useState<SystemStatus | null>(null);
  const [interactionMode, setInteractionMode] = React.useState<InteractionMode>("assistant");
  const [attachmentBusy, setAttachmentBusy] = React.useState(false);
  const [attachedFile, setAttachedFile] = React.useState("");
  const [attachedImage, setAttachedImage] = React.useState("");
  const [attachmentNotice, setAttachmentNotice] = React.useState("");
  const [listenerReady, setListenerReady] = React.useState(false);
  const [draft, setDraft] = React.useState("");
  const [draftError, setDraftError] = React.useState("");
  const [messages, setMessages] = React.useState<ChatEntry[]>([]);
  const [activeRunId, setActiveRunId] = React.useState<string | null>(null);
  const [assistantState, setAssistantState] = React.useState("idle");
  const [historyLoading, setHistoryLoading] = React.useState(inTauri);
  const [savedHistoryCount, setSavedHistoryCount] = React.useState(0);
  const [historyNotice, setHistoryNotice] = React.useState("");
  const [historyBusy, setHistoryBusy] = React.useState(false);
  const [conversations, setConversations] = React.useState<Conversation[]>([]);
  const [activeConversationId, setActiveConversationId] = React.useState<string | null>(null);
  const [conversationBusy, setConversationBusy] = React.useState(false);
  const [renamingId, setRenamingId] = React.useState<string | null>(null);
  const [renameDraft, setRenameDraft] = React.useState("");
  const [headerMenuOpen, setHeaderMenuOpen] = React.useState(false);
  const [toolMenuOpen, setToolMenuOpen] = React.useState(false);
  const [privacyOpen, setPrivacyOpen] = React.useState(false);
  const [hasUnread, setHasUnread] = React.useState(false);
  const [voiceEvent, setVoiceEvent] = React.useState<VoiceEvent | null>(null);
  const [approval, setApproval] = React.useState<ApprovalRequest | null>(null);
  const [grants, setGrants] = React.useState<FolderGrantView[]>([]);
  const [grantBusy, setGrantBusy] = React.useState(false);
  const endRef = React.useRef<HTMLDivElement>(null);
  const bodyRef = React.useRef<HTMLDivElement>(null);
  const inputRef = React.useRef<HTMLTextAreaElement>(null);
  const imageInputRef = React.useRef<HTMLInputElement>(null);
  const closeRef = React.useRef<HTMLButtonElement>(null);
  const privacyAnchorRef = React.useRef<HTMLDivElement>(null);
  const privacyTriggerRef = React.useRef<HTMLButtonElement>(null);
  const privacyCloseRef = React.useRef<HTMLButtonElement>(null);
  const privacyRestoreFocusRef = React.useRef(false);
  const stickToBottomRef = React.useRef(true);

  React.useEffect(() => {
    if (!inTauri) return;
    invoke<SystemStatus>("get_system_status").then(setStatus).catch(() => setStatus(null));
    let disposed = false;
    void invoke<ApprovalRequest | null>("get_pending_approval").then(value => {
      if (!disposed && value) { setApproval(value); setAssistantState("awaiting_approval"); }
    }).catch(() => {});
    void invoke<FolderGrantView[]>("list_folder_grants").then(list => {
      if (!disposed) setGrants(list);
    }).catch(() => {});
    let unlisten: (() => void) | null = null;
    let unlistenHistory: (() => void) | null = null;
    let unlistenVoice: (() => void) | null = null;
    let unlistenApproval: (() => void) | null = null;
    let unlistenPrefs: (() => void) | null = null;
    void invoke<DesktopSettings>("get_settings").then(settings => {
      if (!disposed) setInteractionMode(settings.interaction_mode);
    }).catch(() => {});
    function reloadConversation() {
      void invoke<Conversation[]>("list_conversations").then(list => {
        if (!disposed) setConversations(list);
      }).catch(() => {});
      void invoke<ActiveConversation>("get_active_conversation").then(active => {
        if (disposed) return;
        setActiveConversationId(active.conversation.id);
        setSavedHistoryCount(active.turns.length);
        setMessages(active.turns.map(turn => ({
          runId: turn.runId, question: turn.question, answer: turn.answer,
          phase: "completed" as const, fromHistory: true,
        })));
        setHistoryLoading(false);
      }).catch(() => { if (!disposed) { setHistoryNotice("本机历史暂不可读取；请检查应用数据目录。对话仍可继续。"); setHistoryLoading(false); } });
    }
    reloadConversation();
    void listen<QuickPreferences>("quick-preferences-changed", event => {
      if (disposed) return;
      setInteractionMode(event.payload.interaction_mode);
      reloadConversation();
    }).then(fn => { if (disposed) fn(); else unlistenPrefs = fn; });
    void listen<HistoryOutcome>("history-event", event => {
      const outcome = event.payload;
      if (outcome.status === "saved") {
        setSavedHistoryCount(current => current + 1);
        setHistoryNotice("本轮文字对话已保存到本机历史。");
        void invoke<Conversation[]>("list_conversations").then(list => { if (!disposed) setConversations(list); }).catch(() => {});
      }
      else if (outcome.status === "private") setHistoryNotice("隐私模式：本轮对话只保留在内存中。");
      else setHistoryNotice("本轮对话未能保存到本机历史；请检查应用数据目录。");
    }).then(fn => { if (disposed) fn(); else unlistenHistory = fn; });
    let unlistenKey: (() => void) | null = null;
    void listen("agent-key-status", () => {
      void invoke<SystemStatus>("get_system_status").then(next => {
        if (!disposed) setStatus(next);
      }).catch(() => { if (!disposed) setStatus(null); });
    }).then(fn => { if (disposed) fn(); else unlistenKey = fn; });
    void listen<AgentTextEvent>("assistant-event", event => {
      const payload = event.payload;
      if (payload.type === "state") {
        setAssistantState(payload.state);
        return;
      }
      if (payload.type === "tool_started") {
        setAssistantState("tool_running");
        setMessages(current => current.map(entry => entry.runId === payload.runId
          ? { ...entry, tool: { name: payload.tool, status: "running" } } : entry));
        return;
      }
      if (payload.type === "tool_finished") {
        setAssistantState("thinking");
        setMessages(current => current.map(entry => entry.runId === payload.runId
          ? { ...entry, tool: { name: entry.tool?.name ?? "tool", status: payload.result.status, message: payload.result.message } } : entry));
        return;
      }
      setMessages(current => current.map(entry => {
        if (entry.runId !== payload.runId) return entry;
        if (payload.type === "text_delta") return { ...entry, answer: entry.answer + payload.delta };
        if (payload.type === "completed") return {
          ...entry,
          answer: payload.reply.display_text,
          speechText: payload.reply.speech_text,
          emotion: payload.reply.emotion,
          gesture: payload.reply.gesture,
          phase: "completed",
        };
        if (payload.type === "cancelled") return { ...entry, phase: "cancelled" };
        if (payload.type === "error") return { ...entry, phase: "failed", error: payload.message };
        return entry;
      }));
      if (payload.type === "completed" || payload.type === "cancelled" || payload.type === "error") {
        setActiveRunId(current => current === payload.runId ? null : current);
        setAssistantState("idle");
        setApproval(null);
      }
    }).then(fn => {
      if (disposed) fn();
      else { unlisten = fn; setListenerReady(true); }
    }).catch(() => setListenerReady(false));
    void listen<VoiceEvent>("voice-event", event => {
      if (!disposed) setVoiceEvent(event.payload);
    }).then(fn => { if (disposed) fn(); else unlistenVoice = fn; });
    void listen<ApprovalRequest>("approval-required", event => {
      if (!disposed) { setApproval(event.payload); setAssistantState("awaiting_approval"); }
    }).then(fn => { if (disposed) fn(); else unlistenApproval = fn; });
    return () => { disposed = true; unlisten?.(); unlistenHistory?.(); unlistenKey?.(); unlistenVoice?.(); unlistenApproval?.(); unlistenPrefs?.(); setListenerReady(false); };
  }, []);

  React.useEffect(() => {
    const frame = requestAnimationFrame(() => {
      if (stickToBottomRef.current) {
        endRef.current?.scrollIntoView({ block: "end" });
        setHasUnread(false);
      } else {
        setHasUnread(true);
      }
    });
    return () => cancelAnimationFrame(frame);
  }, [messages]);

  React.useEffect(() => {
    const textarea = inputRef.current;
    if (!textarea) return;
    textarea.style.height = "0";
    textarea.style.height = `${Math.min(textarea.scrollHeight, 112)}px`;
  }, [draft]);

  React.useEffect(() => {
    if (!historyNotice) return;
    const timeout = window.setTimeout(() => setHistoryNotice(""), 4500);
    return () => window.clearTimeout(timeout);
  }, [historyNotice]);

  React.useEffect(() => {
    const focusChat = () => {
      if (inputRef.current?.disabled) closeRef.current?.focus();
      else inputRef.current?.focus();
    };
    if (!inTauri) { focusChat(); return; }
    let disposed = false;
    let unlisten: (() => void) | null = null;
    void listen<boolean>("chat-visibility", event => {
      if (event.payload) focusChat();
    }).then(fn => { if (disposed) fn(); else unlisten = fn; });
    return () => { disposed = true; unlisten?.(); };
  }, []);

  React.useEffect(() => {
    if (!privacyOpen) return;
    const focusFrame = requestAnimationFrame(() => privacyCloseRef.current?.focus());
    const handlePointerDown = (event: PointerEvent) => {
      if (!privacyAnchorRef.current?.contains(event.target as Node)) setPrivacyOpen(false);
    };
    const handleEscape = (event: KeyboardEvent) => {
      if (event.key !== "Escape") return;
      event.preventDefault();
      event.stopPropagation();
      privacyRestoreFocusRef.current = true;
      setPrivacyOpen(false);
    };
    document.addEventListener("pointerdown", handlePointerDown);
    window.addEventListener("keydown", handleEscape, true);
    return () => {
      cancelAnimationFrame(focusFrame);
      document.removeEventListener("pointerdown", handlePointerDown);
      window.removeEventListener("keydown", handleEscape, true);
    };
  }, [privacyOpen]);

  React.useEffect(() => {
    if (privacyOpen || !privacyRestoreFocusRef.current) return;
    privacyRestoreFocusRef.current = false;
    const focusFrame = requestAnimationFrame(() => privacyTriggerRef.current?.focus());
    return () => cancelAnimationFrame(focusFrame);
  }, [privacyOpen]);

  const agentReady = status?.agent === "ready" && listenerReady;
  const agentNotice = agentReady ? "文字对话已就绪。可联网搜索；文件、图片/视频需你主动附加到本轮对话（图像需选用 Vision 模型），位置由系统原生定位按需获取。"
    : !inTauri ? "浏览器只显示界面预览；请启动桌面应用以连接 Agent。"
    : status?.agent === "failed" ? "系统安全存储暂不可用；请检查桌面环境。"
    : status?.agent === "ready" ? "Agent 事件连接未就绪；发送消息暂不可用。"
    : status ? "DeepSeek 密钥尚未由桌面核心配置；发送消息暂不可用。"
    : "正在检查桌面核心与 Agent 配置…";
  const agentStatusLabel = !inTauri ? "浏览器预览" : agentReady ? "就绪" : status?.agent === "failed" ? "连接失败"
    : status?.agent === "ready" ? "连接中" : status ? "待配置" : "检查中";

  async function chooseFile() {
    if (!inTauri || attachmentBusy || activeRunId) return;
    setToolMenuOpen(false);
    setAttachmentBusy(true); setAttachmentNotice("");
    try {
      const name = await invoke<string | null>("select_agent_file");
      if (name) { setAttachedFile(name); setAttachmentNotice("文本文件已附加到下一轮对话；只有调用文件工具时，内容才会交给 DeepSeek。"); }
    } catch { setAttachmentNotice("无法附加文件。请选择不超过 20 KB 的 UTF-8 文本文件。"); }
    finally { setAttachmentBusy(false); }
  }

  async function pickImage(event: React.ChangeEvent<HTMLInputElement>) {
    const file = event.target.files?.[0];
    event.target.value = "";
    if (!file || !inTauri || attachmentBusy || activeRunId) return;
    setToolMenuOpen(false);
    setAttachmentBusy(true); setAttachmentNotice("正在处理图片/视频…");
    try {
      const isVideo = file.type.startsWith("video/");
      const frames = isVideo ? await framesFromVideoFile(file) : await framesFromImageFile(file);
      const name = await invoke<string>("attach_agent_image", {
        name: file.name || (isVideo ? "video" : "image"), frames,
      });
      setAttachedImage(name);
      setAttachmentNotice(isVideo
        ? "已从视频提取关键帧附加到下一轮；发送后才会交给 DeepSeek（需选用支持图像的模型）。"
        : "图片已附加到下一轮对话；发送后才会交给 DeepSeek（需选用支持图像的模型）。");
    } catch {
      setAttachmentNotice("无法处理该图片/视频。请选择不超过 5 MB 的 PNG/JPEG/WebP，或常见格式的短视频。");
    } finally {
      setAttachmentBusy(false);
    }
  }

  async function clearAttachments() {
    if (!inTauri || activeRunId) return;
    try { await invoke("clear_agent_attachments");
      setAttachedFile(""); setAttachedImage(""); setAttachmentNotice("已移除本轮附加内容。");
    } catch { setAttachmentNotice("移除附加内容失败，请重试。"); }
  }

  async function clearHistory() {
    if (!inTauri || historyBusy || historyLoading || activeRunId || savedHistoryCount === 0) return;
    setHistoryBusy(true); setHistoryNotice("");
    try {
      const cleared = await invoke<boolean>("clear_chat_history");
      if (!cleared) { setHistoryNotice("已取消清理；本机历史保持不变。"); return; }
      setMessages([]);
      setSavedHistoryCount(0);
      await refreshConversations();
      setHistoryNotice("当前会话已保存的文字聊天历史已清理；此操作无法在应用内撤销。");
    } catch { setHistoryNotice("清理失败，原历史保持不变。请检查应用数据目录权限后重试。"); }
    finally { setHistoryBusy(false); }
  }

  function applyActiveConversation(active: ActiveConversation) {
    setActiveConversationId(active.conversation.id);
    setSavedHistoryCount(active.turns.length);
    setMessages(active.turns.map(turn => ({
      runId: turn.runId, question: turn.question, answer: turn.answer,
      phase: "completed" as const, fromHistory: true,
    })));
    setRenamingId(null);
  }

  async function refreshConversations() {
    try { setConversations(await invoke<Conversation[]>("list_conversations")); } catch { /* keep the previous list */ }
  }

  async function newConversation() {
    if (!inTauri || conversationBusy || activeRunId) return;
    setConversationBusy(true); setHistoryNotice("");
    setHeaderMenuOpen(false);
    try {
      const active = await invoke<ActiveConversation>("create_conversation");
      setDraft("");
      applyActiveConversation(active);
      await refreshConversations();
      setHistoryNotice("已新建会话；旧会话仍可在会话列表中切换。");
    } catch { setHistoryNotice("新建会话失败，请重试。"); }
    finally { setConversationBusy(false); }
  }

  async function switchConversation(id: string) {
    if (!inTauri || conversationBusy || activeRunId || id === activeConversationId) return;
    setConversationBusy(true); setHistoryNotice("");
    try {
      const active = await invoke<ActiveConversation>("switch_conversation", { id });
      setDraft("");
      applyActiveConversation(active);
      await refreshConversations();
      setHeaderMenuOpen(false);
    } catch { setHistoryNotice("切换会话失败，请重试。"); }
    finally { setConversationBusy(false); }
  }

  async function deleteConversation(conversation: Conversation) {
    if (!inTauri || conversationBusy || activeRunId) return;
    setConversationBusy(true); setHistoryNotice("");
    try {
      const active = await invoke<ActiveConversation>("delete_conversation", { id: conversation.id });
      applyActiveConversation(active);
      await refreshConversations();
      setHistoryNotice("会话已删除。");
    } catch { setHistoryNotice("删除会话失败；原会话保持不变。"); }
    finally { setConversationBusy(false); }
  }

  function startRename(conversation: Conversation) {
    setRenamingId(conversation.id);
    setRenameDraft(conversation.title);
  }

  async function saveRename(id: string) {
    const title = renameDraft.trim();
    if (!title) { setRenamingId(null); return; }
    if (!inTauri || conversationBusy) return;
    setConversationBusy(true); setHistoryNotice("");
    try {
      await invoke("rename_conversation", { id, title });
      await refreshConversations();
    } catch { setHistoryNotice("重命名会话失败，请重试。"); }
    finally { setConversationBusy(false); setRenamingId(null); }
  }

  function cancelRename() {
    setRenamingId(null);
    setRenameDraft("");
  }

  async function chooseFolder(writable: boolean) {
    if (!inTauri || grantBusy || activeRunId) return;
    setToolMenuOpen(false);
    setGrantBusy(true); setAttachmentNotice("");
    try {
      const grant = await invoke<FolderGrantView | null>("select_folder_grant", { writable });
      if (grant) {
        setGrants(current => [...current.filter(item => item.id !== grant.id), grant]);
        setAttachmentNotice(writable
          ? `已授权读写「${grant.label}」；每次写入或删除仍需你逐次确认，可随时在设置中撤销。`
          : `已授权只读访问「${grant.label}」；菲比只能使用相对路径，可随时在设置中撤销。`);
      }
    } catch { setAttachmentNotice("无法授权文件夹；请重试。"); }
    finally { setGrantBusy(false); }
  }

  async function resolveApproval(decision: ApprovalDecision) {
    if (!approval) return;
    const id = approval.id;
    setApproval(null);
    setAssistantState(activeRunId ? "thinking" : "idle");
    try { await invoke("resolve_tool_approval", { approvalId: id, decision }); }
    catch { setAttachmentNotice("审批提交失败；该操作会被自动拒绝。"); }
  }

  async function panicStop() {
    if (!inTauri) return;
    setApproval(null);
    setHistoryNotice("已请求停止当前操作并拒绝所有待审批项。");
    try { await invoke("panic_stop_operations"); }
    catch { setHistoryNotice("停止请求失败；请从系统托盘退出后重启应用。"); }
  }

  async function sendMessage(event: React.FormEvent<HTMLFormElement>) {
    event.preventDefault();
    const text = draft.trim();
    if (!agentReady || activeRunId || !text) return;
    if (new TextEncoder().encode(text).length > 10_000) {
      setDraftError("消息过长，请缩短到 10000 字节以内。");
      return;
    }
    setDraftError("");
    const runId = crypto.randomUUID();
    stickToBottomRef.current = true;
    setMessages(current => [...current, { runId, question: text, answer: "", phase: "thinking" }]);
    setActiveRunId(runId);
    setAssistantState("thinking");
    setDraft("");
    try { await invoke("prompt_agent", { runId, text }); setAttachedFile(""); setAttachedImage(""); setAttachmentNotice(""); }
    catch {
      setMessages(current => current.map(entry => entry.runId === runId
        ? { ...entry, phase: "failed", error: "无法开始对话；请检查 Agent 配置后重试。" } : entry));
      setActiveRunId(null);
      setAssistantState("idle");
      setDraft(text);
    }
  }

  async function stopMessage() {
    if (!activeRunId) return;
    try { await invoke("cancel_agent", { runId: activeRunId }); }
    catch { setMessages(current => current.map(entry => entry.runId === activeRunId
      ? { ...entry, error: "停止请求失败；请再次点击停止。" } : entry)); }
  }

  async function stopVoice() {
    if (!inTauri) return;
    try { await invoke("stop_voice"); }
    catch { setVoiceEvent(current => current ? { ...current, state: "failed", message: "停止语音失败，请重试。" } : current); }
  }

  async function replayVoice(entry: ChatEntry) {
    if (!inTauri || entry.phase !== "completed") return;
    setVoiceEvent(null);
    try {
      await invoke("replay_voice", {
        runId: entry.runId,
        text: entry.speechText || entry.answer,
        emotion: entry.emotion || "calm",
        gesture: entry.gesture || "idle",
      });
    } catch {
      setVoiceEvent({ generation: 0, runId: entry.runId, state: "failed", message: "重新播放失败，请确认本机语音服务正在运行。" });
    }
  }

  async function closeChat() {
    if (!inTauri) { location.search = "?view=pet"; return; }
    try { await invoke("clear_agent_attachments"); setAttachedFile(""); setAttachedImage(""); await invoke("hide_chat"); }
    catch { setDraftError("无法收起聊天窗；请从托盘恢复或重启桌面应用。"); }
  }

  function handleBodyScroll() {
    const body = bodyRef.current;
    if (!body) return;
    const nearBottom = body.scrollHeight - body.scrollTop - body.clientHeight < 72;
    stickToBottomRef.current = nearBottom;
    if (nearBottom) setHasUnread(false);
  }

  function scrollToLatest() {
    stickToBottomRef.current = true;
    endRef.current?.scrollIntoView({ block: "end", behavior: "smooth" });
    setHasUnread(false);
  }

  function useSuggestion(value: string) {
    setDraft(value);
    requestAnimationFrame(() => inputRef.current?.focus());
  }

  return <main className={`chat-shell${interactionMode === "chat" ? " is-chat" : " is-assistant"}`} onKeyDown={event => {
    if (event.key !== "Escape") return;
    if (approval) { event.preventDefault(); event.stopPropagation(); void resolveApproval("deny"); return; }
    if (headerMenuOpen || toolMenuOpen || privacyOpen) {
      setHeaderMenuOpen(false); setToolMenuOpen(false); setPrivacyOpen(false);
    } else void closeChat();
  }}>
    <section className="chat-card" aria-label="菲比聊天面板">
      <header className="chat-header">
        <span className="chat-avatar"><img src="/pet/phoebe-avatar-v2.png" alt="" /></span>
        <div className="chat-identity"><h1>菲比 <span className={`mode-badge${interactionMode === "chat" ? " is-chat" : ""}`}>{interactionMode === "chat" ? "聊天" : "助手"}</span></h1><p><span className={`status-dot ${agentReady ? "is-ready" : ""}`} />{agentStatusLabel}</p></div>
        <div className="chat-header-actions">
          <button className="icon-button" type="button" onClick={() => { setHeaderMenuOpen(value => !value); setToolMenuOpen(false); setPrivacyOpen(false); }} aria-label="更多聊天选项" aria-expanded={headerMenuOpen}><MoreIcon /></button>
          {headerMenuOpen && <div className="chat-popover header-menu">
            <div className="menu-summary"><span>会话</span><strong>{conversationBusy ? "处理中…" : `${conversations.length} 个`}</strong></div>
            <button type="button" className="menu-row" onClick={() => void newConversation()} disabled={!inTauri || conversationBusy || Boolean(activeRunId)}>新建对话</button>
            {conversations.length > 0 && <div className="conversation-list" role="list" aria-label="会话列表">
              {conversations.map(conversation => (
                <div key={conversation.id} className={`conversation-item${conversation.id === activeConversationId ? " is-active" : ""}`} role="listitem">
                  {renamingId === conversation.id ? (
                    <div className="conversation-rename">
                      <input
                        autoFocus
                        value={renameDraft}
                        maxLength={60}
                        placeholder="会话标题"
                        onChange={event => setRenameDraft(event.target.value)}
                        onKeyDown={event => {
                          if (event.key === "Enter") { event.preventDefault(); void saveRename(conversation.id); }
                          else if (event.key === "Escape") { event.preventDefault(); cancelRename(); }
                        }}
                        aria-label="重命名会话"
                      />
                      <button type="button" className="conversation-icon-button" onClick={() => void saveRename(conversation.id)} aria-label="保存标题">✓</button>
                    </div>
                  ) : (
                    <>
                      <button type="button" className="conversation-title" onClick={() => void switchConversation(conversation.id)}
                        disabled={conversationBusy || Boolean(activeRunId)}
                        title={conversation.title || "（未命名会话）"}>
                        <span className="conversation-title-text">{conversation.title || "（未命名会话）"}</span>
                        <span className="conversation-turn-count">{conversation.turnCount} 轮</span>
                      </button>
                      <button type="button" className="conversation-icon-button" onClick={() => startRename(conversation)} aria-label="重命名" title="重命名" disabled={conversationBusy || Boolean(activeRunId)}>✎</button>
                      <button type="button" className="conversation-icon-button is-danger" onClick={() => void deleteConversation(conversation)} aria-label="删除" title="删除" disabled={conversationBusy || Boolean(activeRunId)}>✕</button>
                    </>
                  )}
                </div>
              ))}
            </div>}
            <div className="menu-summary"><span>本机历史</span><strong>{historyLoading ? "读取中…" : `${savedHistoryCount} 条`}</strong></div>
            <button type="button" className="menu-row is-danger" onClick={() => { setHeaderMenuOpen(false); void clearHistory(); }} disabled={!inTauri || historyLoading || historyBusy || Boolean(activeRunId) || savedHistoryCount === 0}>{historyBusy ? "正在清理…" : "清理聊天历史"}</button>
            <button type="button" className="menu-row is-danger" onClick={() => { setHeaderMenuOpen(false); void panicStop(); }} disabled={!inTauri}>{approval ? "拒绝并停止所有操作" : "立即停止所有操作"}</button>
          </div>}
          <button ref={closeRef} className="icon-button" type="button" onClick={closeChat} aria-label="收起聊天窗"><CloseIcon /></button>
        </div>
      </header>

      <div ref={bodyRef} className="chat-body" onScroll={handleBodyScroll}>
        {messages.length === 0 && <div className="empty-chat">
          <div className="empty-avatar"><img src="/pet/phoebe-avatar-v2.png" alt="" /></div>
          <h2>{interactionMode === "chat" ? "想和我聊点什么？" : "今天想一起做什么？"}</h2>
          <p>{agentReady ? (interactionMode === "chat" ? "我在聊天模式：回答会更简短、更亲近，并完整朗读。" : "我可以聊天、联网搜索，也能在你授权后读取文本文件或获取一次设备位置。") : agentNotice}</p>
          {agentReady && <div className="suggestion-list" aria-label="建议问题">
            {interactionMode === "chat" ? <>
              <button type="button" onClick={() => useSuggestion("随便聊聊，今天过得怎么样？")}>随便聊聊今天</button>
              <button type="button" onClick={() => useSuggestion("给我讲个轻松的小故事")}>讲个轻松的小故事</button>
              <button type="button" onClick={() => useSuggestion("用一句话夸夸我")}>用一句话夸夸我</button>
            </> : <>
              <button type="button" onClick={() => useSuggestion("根据我的位置查询今天的天气")}>查询今天的天气</button>
              <button type="button" onClick={() => useSuggestion("搜索今天值得关注的科技新闻，并给出来源")}>搜索今日科技新闻</button>
              <button type="button" onClick={() => { setToolMenuOpen(true); requestAnimationFrame(() => inputRef.current?.focus()); }}>读取一个文本文件</button>
            </>}
          </div>}
        </div>}

        {messages.length > 0 && <div className="message-list">
          {messages.map((entry, index) => <React.Fragment key={entry.runId}>
            {entry.fromHistory && index === 0 && <div className="conversation-divider"><span>本机历史</span></div>}
            {!entry.fromHistory && index > 0 && messages[index - 1]?.fromHistory && <div className="conversation-divider"><span>本轮对话</span></div>}
            <article className="exchange">
              <div className="message-row is-user"><div className="message-user">{entry.question}</div></div>
              <div className="assistant-turn">
                <div className="speaker-label"><span className="speaker-avatar"><img src="/pet/phoebe-avatar-v2.png" alt="" /></span><span>菲比</span></div>
                {entry.tool && <details className={`tool-event is-${entry.tool.status}`} open={entry.tool.status === "running"}>
                  <summary><span className="tool-event-icon"><ToolIcon /></span><span>{toolLabels[entry.tool.name] ?? "使用工具"}</span><span className="tool-event-state">{entry.tool.status === "running" ? "进行中" : entry.tool.status === "success" || entry.tool.status === "already_running" ? "已完成" : "需注意"}</span><ChevronDownIcon /></summary>
                  {entry.tool.message && <p>{entry.tool.message}</p>}
                </details>}
                {entry.answer ? <>
                  <div className="message-assistant"><MessageMarkdown>{entry.answer}</MessageMarkdown></div>
                  {entry.phase === "completed" && <div className="message-actions">
                    <button type="button" className="message-replay" onClick={() => void replayVoice(entry)} disabled={!inTauri}
                      aria-label="重新播放菲比语音" title="重新播放语音"><PlayIcon /></button>
                  </div>}
                </> : entry.phase === "thinking" && <div className="thinking-indicator" role="status" aria-label="菲比正在思考"><span /><span /><span /></div>}
                {entry.phase === "cancelled" && <p className="message-meta">已停止本轮回复</p>}
                {entry.error && <p className="message-error" role="alert">{entry.error}</p>}
              </div>
            </article>
          </React.Fragment>)}
          <div ref={endRef} />
        </div>}
      </div>

      {hasUnread && <button type="button" className="latest-button" onClick={scrollToLatest}><DownIcon />查看新消息</button>}

      <form className="chat-footer" onSubmit={sendMessage}>
        {historyNotice && <p className="chat-toast" role="status">{historyNotice}</p>}
        {voiceEvent && voiceEvent.state !== "idle" && voiceEvent.state !== "stopped" && <div className={`voice-progress is-${voiceEvent.state}`} role="status">
          <span>{voiceEvent.message}</span>
          {(voiceEvent.state === "synthesizing" || voiceEvent.state === "speaking") && <button type="button" onClick={() => void stopVoice()}>停止语音</button>}
        </div>}
        {(attachedFile || attachedImage) && <div className="attachment-chips" aria-label="本轮已附加内容">
          {attachedFile && <span><FileIcon />{attachedFile}</span>}
          {attachedImage && <span><ImageIcon />{attachedImage}</span>}
          <button type="button" onClick={() => void clearAttachments()} disabled={Boolean(activeRunId)} aria-label="移除本轮附加内容"><CloseIcon /></button>
        </div>}
        {attachmentNotice && <p className="composer-notice" role="status" aria-live="polite">{attachmentNotice}</p>}
        {!agentReady && <p className="composer-notice is-warning">{agentNotice}</p>}
        <div className="composer">
          <div className="composer-menu-anchor">
            <input ref={imageInputRef} type="file" accept="image/png,image/jpeg,image/webp,video/mp4,video/quicktime,video/webm" onChange={pickImage} hidden />
            <button className="composer-icon-button" type="button" onClick={() => { setToolMenuOpen(value => !value); setHeaderMenuOpen(false); setPrivacyOpen(false); }} disabled={!inTauri || attachmentBusy || Boolean(activeRunId)} aria-label="添加文件、图片或授权文件夹" aria-expanded={toolMenuOpen}><PlusIcon /></button>
            {toolMenuOpen && <div className="chat-popover tool-menu">
              <button type="button" onClick={() => void chooseFile()}><FileIcon /><span><strong>选择文本文件</strong><small>仅授权本轮读取</small></span></button>
              <button type="button" onClick={() => imageInputRef.current?.click()} disabled={attachmentBusy}><ImageIcon /><span><strong>附加图片/视频</strong><small>PNG/JPEG/WebP 或短视频</small></span></button>
              <button type="button" onClick={() => void chooseFolder(false)} disabled={grantBusy}><FolderIcon /><span><strong>授权文件夹（只读）</strong><small>{grantBusy ? "正在选择…" : grants.length ? `已授权 ${grants.length} 个` : "可随时撤销"}</small></span></button>
              <button type="button" onClick={() => void chooseFolder(true)} disabled={grantBusy}><FolderIcon /><span><strong>授权文件夹（可读写）</strong><small>写入与删除仍需逐次确认</small></span></button>
            </div>}
          </div>
          <label className="sr-only" htmlFor="message">发送消息给菲比</label>
          <textarea id="message" ref={inputRef} rows={1} value={draft} onChange={event => { setDraft(event.target.value); setDraftError(""); }} onKeyDown={event => {
            if (event.key === "Enter" && !event.shiftKey && !event.nativeEvent.isComposing) {
              event.preventDefault(); event.currentTarget.form?.requestSubmit();
            }
          }} disabled={!agentReady} maxLength={10000} aria-invalid={Boolean(draftError)} aria-describedby={draftError ? "draft-error" : undefined} placeholder={agentReady ? "输入消息…" : "等待 Agent 连接…"} />
          <div ref={privacyAnchorRef} className="privacy-anchor">
            <button ref={privacyTriggerRef} className="composer-info-button" type="button"
              onClick={() => { setPrivacyOpen(value => !value); setToolMenuOpen(false); setHeaderMenuOpen(false); }}
              aria-label="查看工具与隐私说明" aria-expanded={privacyOpen} aria-haspopup="dialog" aria-controls="privacy-popover"><InfoIcon /></button>
            {privacyOpen && <div id="privacy-popover" className="chat-popover privacy-popover" role="dialog" aria-labelledby="privacy-popover-title">
              <div className="privacy-popover-heading"><strong id="privacy-popover-title">工具与隐私</strong>
                <button ref={privacyCloseRef} className="privacy-close" type="button" aria-label="关闭工具与隐私说明"
                  onClick={() => { privacyRestoreFocusRef.current = true; setPrivacyOpen(false); }}><CloseIcon /></button>
              </div>
              <p>搜索词会发送给搜索服务；文件、图片/视频仅在你主动附加并发送后才会交给 DeepSeek（图像需选用 Vision 模型；视频会抽取少量关键帧），本轮结束后图片不再随上下文重发。位置由系统原生定位一次性获取，仅在你请求时读取。</p>
            </div>}
          </div>
          {activeRunId ? <button type="button" className="composer-action is-stop" onClick={stopMessage} aria-label="停止生成"><StopIcon /></button>
            : <button type="submit" className="composer-action" disabled={!agentReady || !draft.trim()} aria-label="发送消息"><SendIcon /></button>}
        </div>
        <p className="composer-hint">Enter 发送 · Shift + Enter 换行</p>
        {draftError && <p id="draft-error" className="input-error" role="alert">{draftError}</p>}
        <div className="sr-only" aria-live="polite">{activeRunId ? assistantState === "tool_running" ? "菲比正在使用工具" : "菲比正在回复" : ""}</div>
      </form>
      {approval && <div className="approval-overlay" role="dialog" aria-modal="true" aria-labelledby="approval-title">
        <div className="approval-card">
          <p className="approval-eyebrow">操作确认</p>
          <h2 id="approval-title">菲比请求执行操作</h2>
          <dl className="approval-details">
            <div><dt>操作</dt><dd>{toolLabels[approval.tool] ?? approval.tool}</dd></div>
            <div><dt>目标</dt><dd>{approval.target || "—"}</dd></div>
            <div><dt>影响</dt><dd>{approval.impact}</dd></div>
          </dl>
          <p className="approval-note">{approval.rememberable ? "「始终允许」只对该目标生效，之后不再逐次确认；可在设置中查看审计记录。" : "标准模式下每次操作都需要你确认，信任模式不会绕过系统权限。"}</p>
          {approval.preview && <pre className="approval-preview">{approval.preview}</pre>}
          <div className="approval-actions">
            <button type="button" className="approval-deny" onClick={() => void resolveApproval("deny")}>拒绝</button>
            {approval.rememberable && <button type="button" className="approval-always" onClick={() => void resolveApproval("always")}>始终允许</button>}
            <button type="button" className="approval-allow" autoFocus onClick={() => void resolveApproval("allow_once")}>允许一次</button>
          </div>
        </div>
      </div>}
    </section>
  </main>;
}

type QuickMenuProps = {
  chatVisible: boolean;
  preferences: QuickPreferences;
  usage: TokenUsageSnapshot;
  contextUsage: ContextUsage;
  preferenceBusy?: boolean;
  firstItem: React.RefObject<HTMLButtonElement | null>;
  onMode: (mode: InteractionMode) => void;
  onVoice: () => void;
  onScale: (percent: number) => void;
  onChat: () => void;
  onWave: () => void;
  onSettings: () => void;
  onHide: () => void;
  onQuit: () => void;
  onClose: () => void;
};

function QuickMenu({ chatVisible, preferences, usage, contextUsage, preferenceBusy = false, firstItem, onMode, onVoice, onScale, onChat, onWave, onSettings, onHide, onQuit, onClose }: QuickMenuProps) {
  const voiceLabel = !preferences.voice_enabled
    ? "语音：关闭"
    : preferences.interaction_mode === "assistant" ? "语音：精简播报" : "语音：完整朗读";
  return <div className="pet-menu" role="menu" aria-label="菲比快捷菜单" onKeyDown={event => {
    if (event.key !== "ArrowDown" && event.key !== "ArrowUp" && event.key !== "Home" && event.key !== "End") return;
    const items = Array.from(event.currentTarget.querySelectorAll<HTMLButtonElement>('button[role^="menuitem"]:not(:disabled)'));
    const current = items.indexOf(document.activeElement as HTMLButtonElement);
    const next = current < 0 ? event.key === "ArrowUp" || event.key === "End" ? items.length - 1 : 0
      : event.key === "Home" ? 0 : event.key === "End" ? items.length - 1
      : (current + (event.key === "ArrowDown" ? 1 : -1) + items.length) % items.length;
    items[next]?.focus();
    event.preventDefault();
  }}>
    <span className="pet-menu-heading"><span>菲比</span><small>快捷操作</small></span>
    <span className="menu-section-label">交互模式</span>
    <button ref={firstItem} role="menuitemradio" aria-checked={preferences.interaction_mode === "assistant"}
      className={preferences.interaction_mode === "assistant" ? "is-selected" : ""} disabled={preferenceBusy} onClick={() => onMode("assistant")}>
      <span className="menu-icon"><AssistantModeIcon /></span><span>助手模式</span>{preferences.interaction_mode === "assistant" && <span className="menu-item-state"><CheckIcon /></span>}
    </button>
    <button role="menuitemradio" aria-checked={preferences.interaction_mode === "chat"}
      className={preferences.interaction_mode === "chat" ? "is-selected" : ""} disabled={preferenceBusy} onClick={() => onMode("chat")}>
      <span className="menu-icon"><ChatModeIcon /></span><span>聊天模式</span>{preferences.interaction_mode === "chat" && <span className="menu-item-state"><CheckIcon /></span>}
    </button>
    <button role="menuitemcheckbox" aria-checked={preferences.voice_enabled} disabled={preferenceBusy} onClick={onVoice}>
      <span className="menu-icon"><VoiceMenuIcon /></span><span>{voiceLabel}</span>{preferences.voice_enabled && <span className="menu-item-state"><CheckIcon /></span>}
    </button>
    <span className="menu-section-label">Token 用量</span>
    <div className="menu-usage-card" role="status" aria-live="polite" aria-label={`本轮 ${usage.current.totalTokens} Token，本次启动 ${usage.session.totalTokens} Token`}>
      <span><small>本轮</small><strong>{formatTokenCount(usage.current.totalTokens)}</strong></span>
      <span><small>本次启动</small><strong>{formatTokenCount(usage.session.totalTokens)}</strong></span>
      <em>输入 {formatTokenCount(usage.current.input)} · 输出 {formatTokenCount(usage.current.output)}</em>
      <div className={`menu-context${contextUsage.estimated >= contextUsage.budget ? " is-over" : ""}`} role="status" aria-label={`上下文占用 ${contextUsage.estimated} / ${contextUsage.budget} Token`}>
        <span className="menu-context-head"><small>上下文</small><strong>{formatContextCount(contextUsage.estimated)} / {formatContextCount(contextUsage.budget)}</strong></span>
        <span className="menu-context-bar" aria-hidden="true"><i style={{ width: `${Math.min(100, Math.round(contextUsage.estimated / Math.max(1, contextUsage.budget) * 100))}%` }} /></span>
      </div>
    </div>
    <span className="menu-section-label">桌宠大小</span>
    <div className="menu-scale-grid" role="group" aria-label="桌宠显示大小">
      {PET_SCALE_PRESETS.map(preset => <button type="button" role="menuitemradio" aria-checked={preferences.pet_scale_percent === preset.value}
        className={`${preferences.pet_scale_percent === preset.value ? "is-selected" : ""}${preset.value === 0 ? " is-iconized-option" : ""}`} disabled={preferenceBusy} key={preset.value} onClick={() => onScale(preset.value)}
        title={preset.value === 0 ? "收起全身立绘，显示为圆形头像" : `桌宠缩放到 ${preset.label}`}>
        {preset.value === 0 && <img src="/pet/phoebe-avatar-v2.png" alt="" aria-hidden="true" />}
        <span>{preset.label}</span>
      </button>)}
    </div>
    <span className="menu-divider" />
    <button role="menuitem" onClick={onChat}><span className="menu-icon"><ChatMenuIcon /></span><span>{chatVisible ? "收起聊天" : "打开聊天"}</span></button>
    <button role="menuitem" onClick={onWave}><span className="menu-icon"><WaveMenuIcon /></span><span>挥手</span></button>
    <button role="menuitem" onClick={onSettings}><span className="menu-icon"><SettingsMenuIcon /></span><span>打开设置</span></button>
    <button role="menuitem" onClick={onHide}><span className="menu-icon"><HideMenuIcon /></span><span>隐藏至菜单栏</span></button>
    <button className="menu-danger" role="menuitem" onClick={onQuit}><span className="menu-icon"><PowerMenuIcon /></span><span>退出应用</span></button>
    <button className="menu-cancel" role="menuitem" onClick={onClose}><span className="menu-icon"><CloseIcon /></span><span>取消</span></button>
  </div>;
}

function MenuApp() {
  const [chatVisible, setChatVisible] = React.useState(false);
  const [preferences, setPreferences] = React.useState<QuickPreferences>(defaultQuickPreferences);
  const [usage, setUsage] = React.useState<TokenUsageSnapshot>(defaultTokenUsage);
  const [contextUsage, setContextUsage] = React.useState<ContextUsage>(defaultContextUsage);
  const [preferenceBusy, setPreferenceBusy] = React.useState(false);
  const [error, setError] = React.useState("");
  const firstItem = React.useRef<HTMLButtonElement>(null);

  React.useEffect(() => {
    if (!inTauri) return;
    firstItem.current?.focus();
    let disposed = false;
    const cleanups: Array<() => void> = [];
    const refresh = () => {
      void invoke<boolean>("get_chat_visibility").then(value => {
        if (!disposed) setChatVisible(value);
      }).catch(() => { if (!disposed) setError("无法读取聊天状态"); });
      void invoke<QuickPreferences>("get_quick_preferences").then(value => {
        if (!disposed) setPreferences(value);
      }).catch(() => { if (!disposed) setError("无法读取模式与语音状态"); });
      void invoke<TokenUsageSnapshot>("get_token_usage").then(value => {
        if (!disposed) setUsage(value);
      }).catch(() => { if (!disposed) setError("无法读取 Token 用量"); });
      void invoke<ContextUsage>("get_context_usage").then(value => {
        if (!disposed) setContextUsage(value);
      }).catch(() => {});
    };
    refresh();
    void listen<boolean>("pet-menu-visibility", event => {
      if (!event.payload) return;
      setError(""); refresh(); firstItem.current?.focus();
    }).then(fn => { if (disposed) fn(); else cleanups.push(fn); });
    void listen<TokenUsageSnapshot>("token-usage-changed", event => setUsage(event.payload))
      .then(fn => { if (disposed) fn(); else cleanups.push(fn); });
    void listen<ContextUsage>("context-usage-changed", event => setContextUsage(event.payload))
      .then(fn => { if (disposed) fn(); else cleanups.push(fn); });
    void listen<QuickPreferences>("quick-preferences-changed", event => setPreferences(event.payload))
      .then(fn => { if (disposed) fn(); else cleanups.push(fn); });
    return () => { disposed = true; cleanups.forEach(fn => fn()); };
  }, []);

  async function run(command: string) {
    try {
      const result = await invoke<boolean | void>(command);
      if (command !== "quit_application" && command !== "hide_pet") {
        await invoke("hide_pet_menu", {
          returnFocus: command === "wave_from_menu" || (command === "toggle_chat" && result === false),
        });
      }
    } catch { setError("操作失败，请从系统托盘重试。"); }
  }

  async function updatePreferences(next: QuickPreferences) {
    if (!inTauri || preferenceBusy) return;
    setPreferenceBusy(true); setError("");
    try {
      setPreferences(await invoke<QuickPreferences>("update_quick_preferences", {
        interactionMode: next.interaction_mode,
        voiceEnabled: next.voice_enabled,
      }));
      await invoke("hide_pet_menu", { returnFocus: true });
    } catch { setError("无法保存模式或语音状态，请重试。"); }
    finally { setPreferenceBusy(false); }
  }

  async function updateScale(percent: number) {
    if (!inTauri || preferenceBusy || percent === preferences.pet_scale_percent) return;
    setPreferenceBusy(true); setError("");
    try {
      setPreferences(await invoke<QuickPreferences>("set_pet_scale", { petScalePercent: percent }));
      await invoke("hide_pet_menu", { returnFocus: true });
    } catch { setError("无法缩放桌宠，请重试。"); }
    finally { setPreferenceBusy(false); }
  }

  return <main className="menu-shell" onKeyDown={event => {
    if (event.key === "Escape") { event.stopPropagation(); void invoke("hide_pet_menu", { returnFocus: true }); }
  }}>
    <QuickMenu chatVisible={chatVisible} preferences={preferences} usage={usage} contextUsage={contextUsage} preferenceBusy={preferenceBusy} firstItem={firstItem}
      onMode={mode => void updatePreferences({ ...preferences, interaction_mode: mode })}
      onVoice={() => void updatePreferences({ ...preferences, voice_enabled: !preferences.voice_enabled })}
      onScale={percent => void updateScale(percent)}
      onChat={() => void run("toggle_chat")} onWave={() => void run("wave_from_menu")}
      onSettings={() => void run("show_settings")} onHide={() => void run("hide_pet")}
      onQuit={() => void run("quit_application")} onClose={() => void invoke("hide_pet_menu", { returnFocus: true })} />
    {error && <p className="menu-error" role="alert">{error}</p>}
  </main>;
}

function PetApp() {
  const [chatVisible, setChatVisible] = React.useState(false);
  const [preferences, setPreferences] = React.useState<QuickPreferences>(defaultQuickPreferences);
  const [assistantState, setAssistantState] = React.useState("idle");
  const [voiceState, setVoiceState] = React.useState<VoiceEvent["state"]>("idle");
  const [reaction, setReaction] = React.useState<Pick<AssistantReply, "emotion" | "gesture">>({ emotion: "calm", gesture: "idle" });
  const [reactionActive, setReactionActive] = React.useState(false);
  const [reactionOneShotDone, setReactionOneShotDone] = React.useState(false);
  const [cancelActive, setCancelActive] = React.useState(false);
  const [menuOpen, setMenuOpen] = React.useState(false);
  const [notice, setNotice] = React.useState("");
  const [pose, setPose] = React.useState<"idle" | "wave">("idle");
  const [activityEpoch, setActivityEpoch] = React.useState(0);
  const initialPreviewPose = new URLSearchParams(location.search).get("pose");
  const [previewPose, setPreviewPose] = React.useState<"idle" | "blink" | "wave" | "auto">(
    initialPreviewPose === "blink" || initialPreviewPose === "wave" ? initialPreviewPose : "idle",
  );
  const [previewSurface, setPreviewSurface] = React.useState<"light" | "dark">("light");
  const reducedMotion = useReducedMotion();
  const reactionTimer = React.useRef<ReturnType<typeof setTimeout> | null>(null);
  const voiceReactionTimer = React.useRef<ReturnType<typeof setTimeout> | null>(null);
  const dragStart = React.useRef<{ x: number; y: number } | null>(null);
  const dragged = React.useRef(false);
  const clickTimer = React.useRef<ReturnType<typeof setTimeout> | null>(null);
  const firstMenuItem = React.useRef<HTMLButtonElement>(null);
  const petButton = React.useRef<HTMLButtonElement>(null);
  const canWave = assistantState === "idle"
    && (voiceState === "idle" || voiceState === "stopped")
    && !reactionActive
    && !cancelActive;

  const markUserActivity = React.useCallback(() => setActivityEpoch(value => value + 1), []);
  const triggerWave = React.useCallback(() => {
    if (!canWave) return;
    setPose("wave");
  }, [canWave]);
  const triggerWaveRef = React.useRef(triggerWave);
  React.useEffect(() => { triggerWaveRef.current = triggerWave; }, [triggerWave]);

  const handleAnimationComplete = React.useCallback((clipId: string) => {
    if (clipId.endsWith("-wave")) setPose("idle");
    if (clipId.endsWith("-nod") || clipId.endsWith("-happy")) setReactionOneShotDone(true);
    if (clipId.endsWith("-cancel")) setCancelActive(false);
  }, []);

  React.useEffect(() => { if (menuOpen) firstMenuItem.current?.focus(); }, [menuOpen]);

  React.useEffect(() => {
    if (reducedMotion || !canWave || pose !== "idle" || chatVisible || menuOpen) return;
    const timeout = window.setTimeout(() => setPose("wave"), AUTO_WAVE_IDLE_MS);
    return () => window.clearTimeout(timeout);
  }, [activityEpoch, canWave, chatVisible, menuOpen, pose, reducedMotion]);

  function showReaction(next: Pick<AssistantReply, "emotion" | "gesture">, duration = 3200) {
    if (reactionTimer.current) clearTimeout(reactionTimer.current);
    setReaction(next);
    setReactionActive(true);
    setReactionOneShotDone(false);
    reactionTimer.current = setTimeout(() => {
      setReactionActive(false);
      reactionTimer.current = null;
    }, duration);
  }

  function closeMenu() {
    setMenuOpen(false);
    markUserActivity();
    petButton.current?.focus();
  }

  React.useEffect(() => {
    if (!inTauri) return;
    void invoke<boolean>("get_chat_visibility").then(setChatVisible).catch(() => setNotice("无法读取聊天窗状态"));
    void invoke<QuickPreferences>("get_quick_preferences").then(setPreferences).catch(() => setNotice("无法读取模式与语音状态"));
    let disposed = false;
    const cleanups: Array<() => void> = [];
    void listen<boolean>("chat-visibility", event => setChatVisible(event.payload)).then(fn => {
      if (disposed) fn(); else cleanups.push(fn);
    });
    void listen<AssistantEvent>("assistant-event", event => {
      const payload = event.payload;
      if (payload.type === "state") {
        setAssistantState(payload.state);
        if (payload.state !== "idle") {
          setPose("idle");
          setCancelActive(false);
        }
      }
      else if (payload.type === "completed") {
        setAssistantState("idle");
        showReaction({ emotion: payload.reply.emotion, gesture: payload.reply.gesture });
      }
      else if (payload.type === "cancelled") {
        setAssistantState("idle");
        setPose("idle");
        setCancelActive(true);
      }
      else if (payload.type === "error") setAssistantState("failed");
    }).then(fn => { if (disposed) fn(); else cleanups.push(fn); });
    void listen<VoiceEvent>("voice-event", event => {
      const payload = event.payload;
      setVoiceState(payload.state);
      if (payload.emotion && payload.gesture) setReaction({ emotion: payload.emotion, gesture: payload.gesture });
      if (payload.state === "speaking") {
        if (voiceReactionTimer.current) clearTimeout(voiceReactionTimer.current);
        setPose("idle");
        setReactionActive(true);
        setReactionOneShotDone(false);
      }
      else if (payload.state === "idle" || payload.state === "stopped") {
        if (voiceReactionTimer.current) clearTimeout(voiceReactionTimer.current);
        voiceReactionTimer.current = setTimeout(() => {
          setReactionActive(false);
          voiceReactionTimer.current = null;
        }, 1700);
      }
    }).then(fn => {
      if (disposed) fn(); else cleanups.push(fn);
    });
    void listen<QuickPreferences>("quick-preferences-changed", event => {
      setPreferences(event.payload);
      setPose("idle");
    }).then(fn => {
      if (disposed) fn(); else cleanups.push(fn);
    });
    void listen("pet-wave", () => triggerWaveRef.current()).then(fn => { if (disposed) fn(); else cleanups.push(fn); });
    return () => {
      disposed = true;
      cleanups.forEach(fn => fn());
      if (clickTimer.current) clearTimeout(clickTimer.current);
      if (reactionTimer.current) clearTimeout(reactionTimer.current);
      if (voiceReactionTimer.current) clearTimeout(voiceReactionTimer.current);
    };
  }, []);

  async function toggleChat() {
    markUserActivity();
    setMenuOpen(false);
    if (!inTauri) { location.search = "?view=chat"; return; }
    try { setChatVisible(await invoke<boolean>("toggle_chat")); setNotice(""); }
    catch { setNotice("无法切换聊天窗；请从托盘重试。"); }
  }

  function scheduleToggle() {
    if (dragged.current) return;
    if (clickTimer.current) clearTimeout(clickTimer.current);
    clickTimer.current = setTimeout(() => { clickTimer.current = null; void toggleChat(); }, 520);
  }

  async function handleDoubleClick() {
    markUserActivity();
    if (clickTimer.current) { clearTimeout(clickTimer.current); clickTimer.current = null; }
    setMenuOpen(false);
    if (!inTauri) { location.search = "?view=settings"; return; }
    try { await invoke("show_settings"); setNotice(""); }
    catch { setNotice("无法打开设置窗；请从托盘重试。"); }
  }

  async function hidePet() {
    setMenuOpen(false);
    if (!inTauri) { setNotice("浏览器预览不支持托盘隐藏。"); return; }
    try { await invoke("hide_pet"); }
    catch { setNotice("隐藏失败，请从托盘重试。"); }
  }

  async function quitApplication() {
    if (!inTauri) { setMenuOpen(false); return; }
    try { await invoke("quit_application"); }
    catch { setNotice("退出失败，请从系统托盘重试。"); }
  }

  async function updatePreferences(next: QuickPreferences) {
    markUserActivity();
    setMenuOpen(false);
    setPose("idle");
    if (!inTauri) { setPreferences(next); return; }
    try {
      setPreferences(await invoke<QuickPreferences>("update_quick_preferences", {
        interactionMode: next.interaction_mode,
        voiceEnabled: next.voice_enabled,
      }));
      setNotice("");
    } catch { setNotice("无法保存模式或语音状态，请重试。"); }
  }

  async function updatePetScale(percent: number) {
    markUserActivity();
    setMenuOpen(false);
    if (!inTauri) {
      setPreferences(current => ({ ...current, pet_scale_percent: percent }));
      return;
    }
    try {
      setPreferences(await invoke<QuickPreferences>("set_pet_scale", { petScalePercent: percent }));
      setNotice("");
    } catch { setNotice("无法缩放桌宠，请重试。"); }
  }

  const activePreviewPose = !inTauri ? previewPose : "auto";
  const activePetState = voiceState === "synthesizing" ? "thinking" : voiceState === "speaking" ? "speaking" : assistantState;
  const activeClip = selectPetClip({
    mode: preferences.interaction_mode,
    previewPose: activePreviewPose,
    manualWave: pose === "wave",
    assistantState,
    voiceState,
    reaction,
    reactionActive,
    reactionOneShotDone,
    cancelActive,
  });
  const petAnimation = usePetAnimation(activeClip, reducedMotion, handleAnimationComplete);
  const iconized = preferences.pet_scale_percent === 0;
  const petCard = <section className="pet-card" aria-label={iconized ? "菲比助手图标窗口" : "菲比助手角色窗口"} data-state={activePetState} data-mode={preferences.interaction_mode} data-iconized={iconized}>
      <button ref={petButton} className="pet-character" data-clip={petAnimation.clipId} data-motion={petAnimation.motion} data-preview-pose={inTauri ? undefined : previewPose} aria-label={chatVisible ? "收起聊天窗，或拖动菲比" : "打开聊天窗，或拖动菲比"} aria-expanded={chatVisible}
        onPointerDown={event => { markUserActivity(); if (event.button !== 0) return; dragStart.current = { x: event.screenX, y: event.screenY }; dragged.current = false; }}
        onPointerMove={event => {
          if (!dragStart.current || dragged.current || !inTauri) return;
          if (Math.hypot(event.screenX - dragStart.current.x, event.screenY - dragStart.current.y) < 8) return;
          dragged.current = true;
          dragStart.current = null;
          void invoke("start_pet_drag").catch(() => setNotice("拖动失败，请重试。"));
        }}
        onPointerUp={() => { dragStart.current = null; }}
        onPointerCancel={() => { dragStart.current = null; dragged.current = true; }}
        onClick={scheduleToggle} onDoubleClick={handleDoubleClick}
        onContextMenu={event => {
          event.preventDefault();
          markUserActivity();
          if (inTauri) void invoke("show_pet_menu").catch(() => setNotice("无法打开快捷菜单，请从托盘重试。"));
          else setMenuOpen(true);
        }}>
        <span className="pet-orb" aria-hidden="true"><img src="/pet/phoebe-avatar-v2.png" alt="" draggable={false} /></span>
        <span className="pet-animation" aria-hidden="true">
          <img className="pet-animation-frame" src={petAnimation.src} alt="" draggable={false} />
        </span>
      </button>
      {menuOpen && <QuickMenu chatVisible={chatVisible} preferences={preferences} usage={defaultTokenUsage} contextUsage={defaultContextUsage} firstItem={firstMenuItem}
        onMode={mode => void updatePreferences({ ...preferences, interaction_mode: mode })}
        onVoice={() => void updatePreferences({ ...preferences, voice_enabled: !preferences.voice_enabled })}
        onScale={percent => void updatePetScale(percent)}
        onChat={() => void toggleChat()}
        onWave={() => { markUserActivity(); setMenuOpen(false); if (!inTauri) setPreviewPose("wave"); triggerWave(); }}
        onSettings={() => void handleDoubleClick()} onHide={() => void hidePet()}
        onQuit={() => void quitApplication()} onClose={closeMenu} />}
      {notice && <p className="pet-notice" role="status">{notice}</p>}
    </section>;

  if (inTauri) return <main className="pet-shell" onKeyDown={event => { if (event.key === "Escape") closeMenu(); }}>{petCard}</main>;
  return <main className="pet-preview-shell" onKeyDown={event => { if (event.key === "Escape" && menuOpen) closeMenu(); }}>
    <div className="pet-preview-content">
      <header className="pet-preview-heading">
        <div><p className="preview-eyebrow">PHOEBE · DESKTOP PET PROTOTYPE</p><h1>菲比桌宠动作预览</h1>
          <p>用同一套透明立绘对比待机、眨眼与挥手。这里是浏览器中的交互原型，不是最终角色动画。</p></div>
        <span className="preview-badge">本机预览 · {iconized ? "84 × 84" : "240 × 328"}</span>
      </header>
      <div className="pet-preview-layout">
        <div className={`pet-preview-stage is-${previewSurface}`} aria-label={`${previewSurface === "dark" ? "深色" : "浅色"}桌面背景预览`}>
          <div className={`pet-preview-window${iconized ? " is-iconized" : ""}`}>{petCard}</div>
          <div className="preview-face-detail" aria-label="表情局部近景">
            <div className="preview-face-crop"><img src={previewPose === "blink" ? "/pet/phoebe-assistant-blink-v2.png" : previewPose === "wave" ? "/pet/phoebe-assistant-wave-v1.png" : "/pet/phoebe-assistant-idle-v1.png"}
              alt={previewPose === "blink" ? "菲比闭眼表情近景" : previewPose === "wave" ? "菲比挥手表情近景" : "菲比待机表情近景"} draggable={false} /></div>
            <span>表情近景</span>
          </div>
          <p className="preview-stage-caption">{iconized ? <>图标化窗口 <strong>84 × 84</strong> · 保留拖动与快捷交互</> : <>角色窗口真实尺寸 <strong>240 × 328</strong> · 顶部与底部保留动作安全区</>}</p>
        </div>
        <aside className="pet-preview-controls" aria-label="原型演示控制">
          <div><p className="preview-section-label">动作帧</p><h2>选一个状态，仔细看细节</h2><p>手动状态会固定展示，便于比较脸部和落脚点；自动演示恢复周期挥手与自然眨眼。</p></div>
          <div className="preview-options" role="group" aria-label="选择动作状态">
            {(["idle", "blink", "wave", "auto"] as const).map(action => <button type="button" key={action} className={previewPose === action ? "is-active" : ""}
              aria-pressed={previewPose === action} onClick={() => setPreviewPose(action)}>
              <strong>{action === "idle" ? "待机" : action === "blink" ? "眨眼" : action === "wave" ? "挥手" : "自动演示"}</strong>
              <small>{action === "idle" ? "基础站姿" : action === "blink" ? "闭眼局部叠层" : action === "wave" ? "完整姿势替换" : "恢复周期动作"}</small>
            </button>)}
          </div>
          <div className="preview-divider" />
          <div><p className="preview-section-label">桌面环境</p><h2>看看透明边缘</h2><p>切换背景检查发丝、披风和法杖边缘的抠图质量。</p></div>
          <div className="preview-surfaces" role="group" aria-label="选择桌面背景">
            <button type="button" aria-pressed={previewSurface === "light"} className={previewSurface === "light" ? "is-active" : ""} onClick={() => setPreviewSurface("light")}>浅色桌面</button>
            <button type="button" aria-pressed={previewSurface === "dark"} className={previewSurface === "dark" ? "is-active" : ""} onClick={() => setPreviewSurface("dark")}>深色桌面</button>
          </div>
          <p className="preview-caveat">动作包已接入眨眼、倾听、思考、工具执行、回复、害羞、担忧与一次性挥手；自动挥手在连续空闲 120 秒后触发。</p>
        </aside>
      </div>
    </div>
  </main>;
}

createRoot(document.getElementById("root")!).render(<React.StrictMode>{view === "chat" ? <ChatApp /> : view === "settings" ? <SettingsApp /> : view === "menu" ? <MenuApp /> : <PetApp />}</React.StrictMode>);
