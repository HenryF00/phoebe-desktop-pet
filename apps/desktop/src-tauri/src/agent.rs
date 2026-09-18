use serde::{Deserialize, Serialize};
use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::{
    atomic::{AtomicU64, Ordering},
    Arc, Mutex,
};
#[cfg(not(debug_assertions))]
use tauri::path::BaseDirectory;
use tauri::{Emitter, Manager};

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum AgentEvent {
    State {
        #[serde(rename = "runId")]
        run_id: String,
        state: String,
    },
    TextDelta {
        #[serde(rename = "runId")]
        run_id: String,
        delta: String,
    },
    Usage {
        #[serde(rename = "runId")]
        run_id: String,
        usage: TokenUsage,
    },
    ToolStarted {
        #[serde(rename = "runId")]
        run_id: String,
        tool: String,
    },
    ToolFinished {
        #[serde(rename = "runId")]
        run_id: String,
        result: serde_json::Value,
    },
    ToolRequest {
        #[serde(rename = "requestId")]
        request_id: String,
        #[serde(rename = "runId")]
        run_id: String,
        tool: String,
        arguments: serde_json::Value,
    },
    ToolProgress {
        #[serde(rename = "runId")]
        run_id: String,
        #[serde(rename = "requestId")]
        request_id: String,
        detail: String,
    },
    ContextUsage {
        #[serde(rename = "runId")]
        run_id: String,
        estimated: u64,
        budget: u64,
    },
    Completed {
        #[serde(rename = "runId")]
        run_id: String,
        reply: Reply,
    },
    Cancelled {
        #[serde(rename = "runId")]
        run_id: String,
    },
    Error {
        #[serde(rename = "runId")]
        run_id: String,
        message: String,
    },
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct TokenUsage {
    input: u64,
    output: u64,
    cache_read: u64,
    cache_write: u64,
    total_tokens: u64,
}

impl TokenUsage {
    fn delta_from(&self, previous: &Self) -> Self {
        Self {
            input: self.input.saturating_sub(previous.input),
            output: self.output.saturating_sub(previous.output),
            cache_read: self.cache_read.saturating_sub(previous.cache_read),
            cache_write: self.cache_write.saturating_sub(previous.cache_write),
            total_tokens: self.total_tokens.saturating_sub(previous.total_tokens),
        }
    }

    fn add_assign(&mut self, next: &Self) {
        self.input = self.input.saturating_add(next.input);
        self.output = self.output.saturating_add(next.output);
        self.cache_read = self.cache_read.saturating_add(next.cache_read);
        self.cache_write = self.cache_write.saturating_add(next.cache_write);
        self.total_tokens = self.total_tokens.saturating_add(next.total_tokens);
    }
}

#[derive(Clone, Debug, Default, Serialize, PartialEq, Eq)]
pub struct TokenUsageSnapshot {
    current: TokenUsage,
    session: TokenUsage,
}

/// Estimated token size of the retained conversation history and its budget.
#[derive(Clone, Debug, Default, Serialize, PartialEq, Eq)]
pub struct ContextUsageSnapshot {
    estimated: u64,
    budget: u64,
}

impl TokenUsageSnapshot {
    fn record_cumulative(&mut self, next: &TokenUsage) {
        self.session.add_assign(&next.delta_from(&self.current));
        self.current = next.clone();
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct Reply {
    display_text: String,
    speech_text: String,
    emotion: String,
    gesture: String,
}

impl AgentEvent {
    fn run_id(&self) -> &str {
        match self {
            Self::State { run_id, .. }
            | Self::TextDelta { run_id, .. }
            | Self::Usage { run_id, .. }
            | Self::ToolStarted { run_id, .. }
            | Self::ToolFinished { run_id, .. }
            | Self::ToolRequest { run_id, .. }
            | Self::ToolProgress { run_id, .. }
            | Self::ContextUsage { run_id, .. }
            | Self::Completed { run_id, .. }
            | Self::Cancelled { run_id }
            | Self::Error { run_id, .. } => run_id,
        }
    }

    fn terminal(&self) -> bool {
        matches!(
            self,
            Self::Completed { .. } | Self::Cancelled { .. } | Self::Error { .. }
        )
    }
}

struct AgentProcess {
    child: Child,
    stdin: ChildStdin,
    key: String,
    model: String,
    search_proxy: String,
}

impl AgentProcess {
    /// Writes one JSON command to the sidecar. Used both by the supervisor and
    /// by tool-result threads, so it must stay atomic per line.
    fn write_line(&mut self, line: &str) -> std::io::Result<()> {
        self.stdin.write_all(line.as_bytes())?;
        self.stdin.write_all(b"\n")?;
        self.stdin.flush()
    }
}

#[derive(Clone)]
struct ActiveRun {
    run_id: String,
    question: String,
    private_at_start: bool,
    allowed_tools: Vec<String>,
    mode: crate::settings::InteractionMode,
    birthday: bool,
}

#[derive(Clone, Serialize)]
struct HistoryOutcome {
    #[serde(rename = "runId")]
    run_id: String,
    status: &'static str,
}

#[derive(Clone, Serialize)]
struct FileAttachment {
    name: String,
    content: String,
}

#[derive(Clone, Serialize)]
struct ImageFrame {
    #[serde(rename = "mimeType")]
    mime_type: String,
    /// Base64-encoded frame bytes (no data-URL prefix).
    data: String,
}

#[derive(Clone, Serialize)]
struct ImageAttachment {
    name: String,
    frames: Vec<ImageFrame>,
}

#[derive(Default)]
pub struct AgentSupervisor {
    process: Arc<Mutex<Option<AgentProcess>>>,
    active: Arc<Mutex<Option<ActiveRun>>>,
    generation: Arc<AtomicU64>,
    file_attachment: Mutex<Option<FileAttachment>>,
    image_attachment: Mutex<Option<ImageAttachment>>,
    usage: Arc<Mutex<TokenUsageSnapshot>>,
    context_usage: Arc<Mutex<ContextUsageSnapshot>>,
    /// Conversation currently loaded into the sidecar, keyed by interaction
    /// mode, so switching mode or conversation reseeds it.
    applied_conversation: Arc<Mutex<Option<(crate::settings::InteractionMode, String)>>>,
}

pub fn key_status(store: &crate::secure_store::SecureStore) -> &'static str {
    match store.effective_key() {
        Ok(Some(_)) => "ready",
        Ok(None) => "not_configured",
        Err(_) => "failed",
    }
}

fn validate_prompt(run_id: &str, text: &str) -> Result<(), String> {
    if run_id.is_empty()
        || run_id.len() > 100
        || !run_id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-')
    {
        return Err("无效的对话标识".into());
    }
    if text.trim().is_empty() || text.len() > 10_000 {
        return Err("消息应为 1 到 10000 字节".into());
    }
    Ok(())
}

impl AgentSupervisor {
    pub fn is_busy(&self) -> bool {
        self.active.lock().map(|run| run.is_some()).unwrap_or(true)
    }

    pub fn usage_snapshot(&self) -> Result<TokenUsageSnapshot, String> {
        self.usage
            .lock()
            .map(|usage| usage.clone())
            .map_err(|_| "Token 统计状态不可用".into())
    }

    pub fn context_usage_snapshot(&self) -> Result<ContextUsageSnapshot, String> {
        self.context_usage
            .lock()
            .map(|usage| usage.clone())
            .map_err(|_| "上下文统计状态不可用".into())
    }

    pub fn attach_file(&self, path: &std::path::Path) -> Result<String, String> {
        if self.is_busy() {
            return Err("请先等待当前回复结束".into());
        }
        let extension = path
            .extension()
            .and_then(|ext| ext.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        if !["txt", "md", "csv", "json", "log"].contains(&extension.as_str()) {
            return Err("目前仅支持 TXT、Markdown、CSV、JSON 和 LOG 文本文件".into());
        }
        let metadata = std::fs::metadata(path).map_err(|_| "无法读取所选文件")?;
        if !metadata.is_file() || metadata.len() > 20_000 {
            return Err("请选择不超过 20 KB 的文本文件".into());
        }
        let content =
            std::fs::read_to_string(path).map_err(|_| "所选文件不是可读取的 UTF-8 文本")?;
        if content.len() > 20_000 {
            return Err("所选文件过大".into());
        }
        let name = path
            .file_name()
            .and_then(|part| part.to_str())
            .ok_or("文件名不可读取")?
            .to_owned();
        *self
            .file_attachment
            .lock()
            .map_err(|_| "文件授权状态不可用")? = Some(FileAttachment {
            name: name.clone(),
            content,
        });
        Ok(name)
    }

    pub fn attach_image(
        &self,
        name: &str,
        frames: Vec<(String, String)>,
    ) -> Result<String, String> {
        if self.is_busy() {
            return Err("请先等待当前回复结束".into());
        }
        let name = name.trim();
        if name.is_empty() || name.len() > 240 || name.chars().any(char::is_control) {
            return Err("图片/视频名称无效".into());
        }
        if frames.is_empty() || frames.len() > 6 {
            return Err("图片帧数无效（最多 6 帧）".into());
        }
        let mut total = 0usize;
        let mut checked = Vec::with_capacity(frames.len());
        for (mime_type, data) in frames {
            if !matches!(mime_type.as_str(), "image/png" | "image/jpeg" | "image/webp") {
                return Err("目前仅支持 PNG、JPEG 和 WebP 图像".into());
            }
            if data.is_empty()
                || data.len() > 3_000_000
                || !data
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '/' | '='))
            {
                return Err("图像数据无效或单帧过大".into());
            }
            total += data.len();
            checked.push(ImageFrame { mime_type, data });
        }
        if total > 7_000_000 {
            return Err("图像总大小过大（上限约 5 MB）".into());
        }
        *self
            .image_attachment
            .lock()
            .map_err(|_| "图像授权状态不可用")? = Some(ImageAttachment {
            name: name.to_owned(),
            frames: checked,
        });
        Ok(name.to_owned())
    }

    pub fn clear_attachments(&self) -> Result<(), String> {
        *self
            .file_attachment
            .lock()
            .map_err(|_| "文件授权状态不可用")? = None;
        *self
            .image_attachment
            .lock()
            .map_err(|_| "图片授权状态不可用")? = None;
        Ok(())
    }

    pub fn prompt(&self, app: &tauri::AppHandle, run_id: &str, text: &str) -> Result<(), String> {
        validate_prompt(run_id, text)?;
        app.state::<crate::voice::VoiceService>().stop(app);
        let settings = app.state::<crate::settings::SettingsStore>().get()?;
        let interaction_mode = settings.interaction_mode;
        let action_mode = settings.action_mode;
        let allowed_tools = crate::tools::allowed_tools_for(interaction_mode, action_mode);
        let folder_grants = app.state::<crate::tools::ToolBroker>().grant_summaries();
        let model = settings.model;
        let search_proxy = settings.search_proxy;
        let memories = app
            .state::<crate::history::HistoryStore>()
            .memories()
            .unwrap_or_default()
            .into_iter()
            .take(20)
            .map(|memory| serde_json::json!({ "title": memory.title, "content": memory.content }))
            .collect::<Vec<_>>();
        let key = app
            .state::<crate::secure_store::SecureStore>()
            .effective_key()
            .map_err(str::to_owned)?
            .ok_or("DeepSeek 尚未在桌面核心中配置；文字对话不可用")?;
        // Greet on the user's birthday once per day.
        let today = crate::settings::today_month_day();
        let is_birthday_greeting = settings.birthday.as_deref() == Some(today.as_str())
            && settings.last_birthday_greeting.as_deref() != Some(today.as_str());
        let mut active = self.active.lock().map_err(|_| "Agent 状态不可用")?;
        if active.is_some() {
            return Err("请等待当前回复结束或先停止".into());
        }
        let file = self
            .file_attachment
            .lock()
            .map_err(|_| "文件授权状态不可用")?
            .take();
        let image = self
            .image_attachment
            .lock()
            .map_err(|_| "图片授权状态不可用")?
            .take();
        *active = Some(ActiveRun {
            run_id: run_id.to_owned(),
            question: text.to_owned(),
            private_at_start: settings.privacy_mode,
            allowed_tools: allowed_tools.clone(),
            mode: interaction_mode,
            birthday: is_birthday_greeting,
        });
        drop(active);
        let usage_snapshot = {
            let mut usage = self.usage.lock().map_err(|_| "Token 统计状态不可用")?;
            usage.current = TokenUsage::default();
            usage.clone()
        };
        let _ = app.emit("token-usage-changed", usage_snapshot);

        // Seeding history is best-effort: if the database is unavailable the
        // conversation still proceeds, just without replayed history.
        let _ = self.seed_conversation(app, &key, &model, &search_proxy);

        let result = self.write_command(
            app,
            &key,
            &model,
            &search_proxy,
            serde_json::json!({
                "type": "prompt", "runId": run_id, "text": text, "memories": memories,
                "file": file, "image": image, "interactionMode": interaction_mode,
                "occasion": is_birthday_greeting.then_some("birthday"),
                "userAddress": settings.user_address,
                "tools": allowed_tools, "folderGrants": folder_grants
            }),
        );
        if is_birthday_greeting && result.is_ok() {
            let _ = app
                .state::<crate::settings::SettingsStore>()
                .mark_birthday_greeted(app, today);
        }
        if result.is_err() {
            if let Ok(mut active) = self.active.lock() {
                *active = None;
            }
        }
        result
    }

    pub fn cancel(&self, app: &tauri::AppHandle, run_id: &str) -> Result<(), String> {
        let active = self.active.lock().map_err(|_| "Agent 状态不可用")?;
        if active.as_ref().map(|run| run.run_id.as_str()) != Some(run_id) {
            return Err("该轮回复已结束".into());
        }
        drop(active);
        if let Some(broker) = app.try_state::<crate::tools::ToolBroker>() {
            broker.deny_run(run_id);
        }
        let line = serde_json::json!({ "type": "cancel", "runId": run_id }).to_string();
        let mut process = self.process.lock().map_err(|_| "Agent 进程不可用")?;
        let child = process.as_mut().ok_or("Agent 进程未运行")?;
        child
            .write_line(&line)
            .map_err(|_| "无法中断 Agent 进程".into())
    }

    /// Cancels whatever run is active, if any. Used by the emergency stop.
    pub fn cancel_active(&self, app: &tauri::AppHandle) -> Result<(), String> {
        let run_id = self
            .active
            .lock()
            .map_err(|_| "Agent 状态不可用")?
            .as_ref()
            .map(|run| run.run_id.clone());
        match run_id {
            Some(run_id) => self.cancel(app, &run_id),
            None => Ok(()),
        }
    }

    /// Loads the active conversation into the sidecar when it changed. The
    /// sidecar keeps its own message list, so switching conversations must
    /// reset it and replay the stored user/assistant text.
    fn seed_conversation(
        &self,
        app: &tauri::AppHandle,
        key: &str,
        model: &str,
        search_proxy: &str,
    ) -> Result<(), String> {
        let mode = app
            .state::<crate::settings::SettingsStore>()
            .get()?
            .interaction_mode;
        let store = app.state::<crate::history::HistoryStore>();
        let conversation_id = store.active_id(mode)?;
        let already_applied = self
            .applied_conversation
            .lock()
            .map_err(|_| "会话状态不可用")?
            .as_ref()
            == Some(&(mode, conversation_id.clone()));
        if already_applied {
            return Ok(());
        }
        let seed = store.seed(mode)?;
        self.write_command(
            app,
            key,
            model,
            search_proxy,
            serde_json::json!({
                "type": "conversation",
                "conversationId": conversation_id,
                "history": seed,
            }),
        )?;
        *self.applied_conversation.lock().map_err(|_| "会话状态不可用")? =
            Some((mode, conversation_id));
        Ok(())
    }

    fn write_command(
        &self,
        app: &tauri::AppHandle,
        key: &str,
        model: &str,
        search_proxy: &str,
        command: serde_json::Value,
    ) -> Result<(), String> {
        let mut process = self.process.lock().map_err(|_| "Agent 进程不可用")?;
        let restart = process.as_mut().is_some_and(|p| {
            p.key != key
                || p.model != model
                || p.search_proxy != search_proxy
                || p.child.try_wait().ok().flatten().is_some()
        });
        if restart {
            // An old reader must not report an intentional restart as a crash of the new run.
            self.generation.fetch_add(1, Ordering::SeqCst);
            if let Ok(mut applied) = self.applied_conversation.lock() {
                *applied = None;
            }
            if let Some(mut old) = process.take() {
                let _ = old.child.kill();
                let _ = old.child.wait();
            }
        }
        if process.is_none() {
            *process = Some(self.spawn_sidecar(app, key, model, search_proxy)?);
            if let Ok(mut applied) = self.applied_conversation.lock() {
                *applied = None;
            }
        }
        let child = process.as_mut().ok_or("Agent 进程未运行")?;
        let line = command.to_string();
        child
            .write_line(&line)
            .map_err(|_| "无法向 Agent 发送消息".into())
    }

    fn spawn_sidecar(
        &self,
        app: &tauri::AppHandle,
        key: &str,
        model: &str,
        search_proxy: &str,
    ) -> Result<AgentProcess, String> {
        #[cfg(debug_assertions)]
        let script = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../../packages/agent/src/sidecar.mjs");
        #[cfg(debug_assertions)]
        let program = std::path::PathBuf::from("node");
        #[cfg(not(debug_assertions))]
        let script = app
            .path()
            .resolve("phoebe-agent.cjs", BaseDirectory::Resource)
            .map_err(|_| "无法定位打包的 Agent 资源")?;
        #[cfg(not(debug_assertions))]
        let program = std::env::current_exe()
            .map_err(|_| "无法定位桌面应用程序")?
            .parent()
            .ok_or("桌面应用程序路径无效")?
            .join(if cfg!(windows) {
                "phoebe-agent-node.exe"
            } else {
                "phoebe-agent-node"
            });
        if !script.is_file() || (program.is_absolute() && !program.is_file()) {
            return Err("打包的 Agent Sidecar 不完整，请重新安装应用".into());
        }
        let mut command = Command::new(&program);
        command
            .arg(&script)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .env_clear()
            .env("DEEPSEEK_API_KEY", key)
            .env("PHOEBE_DEEPSEEK_MODEL", model);
        for name in [
            "PATH",
            "SYSTEMROOT",
            "TEMP",
            "TMP",
            "TMPDIR",
            "https_proxy",
            "HTTPS_PROXY",
            "http_proxy",
            "HTTP_PROXY",
            "no_proxy",
            "NO_PROXY",
        ] {
            if let Some(value) = std::env::var_os(name) {
                command.env(name, value);
            }
        }
        if !search_proxy.is_empty() {
            command
                .env("https_proxy", search_proxy)
                .env("http_proxy", search_proxy);
        }
        let mut child = command
            .spawn()
            .map_err(|_| "无法启动固定的 Agent Sidecar")?;
        let stdin = child.stdin.take().ok_or("Pi Sidecar 输入不可用")?;
        let stdout = child.stdout.take().ok_or("Pi Sidecar 输出不可用")?;
        let handle = app.clone();
        let active = Arc::clone(&self.active);
        let process_for_results = Arc::clone(&self.process);
        let broker = app
            .try_state::<crate::tools::ToolBroker>()
            .map(|state| state.inner().clone());
        let usage_totals = Arc::clone(&self.usage);
        let context_usage_totals = Arc::clone(&self.context_usage);
        let applied_conversation = Arc::clone(&self.applied_conversation);
        let supervisor_generation = Arc::clone(&self.generation);
        let my_generation = supervisor_generation.fetch_add(1, Ordering::SeqCst) + 1;
        std::thread::spawn(move || {
            for line in BufReader::new(stdout).lines() {
                let Ok(line) = line else {
                    break;
                };
                let Ok(event) = serde_json::from_str::<AgentEvent>(&line) else {
                    continue;
                };
                // Context usage is reported outside the active-run lifecycle (for
                // example right after a run finishes), so handle it before the
                // run filter and forward it to the UI.
                if let AgentEvent::ContextUsage { estimated, budget, .. } = &event {
                    if let Ok(mut snapshot) = context_usage_totals.lock() {
                        *snapshot = ContextUsageSnapshot {
                            estimated: *estimated,
                            budget: *budget,
                        };
                        let _ = handle.emit("context-usage-changed", snapshot.clone());
                    }
                    continue;
                }
                let Ok(mut current) = active.lock() else {
                    break;
                };
                if current.as_ref().map(|run| run.run_id.as_str()) != Some(event.run_id()) {
                    continue;
                }
                let allowed_tools = current
                    .as_ref()
                    .map(|run| run.allowed_tools.clone())
                    .unwrap_or_default();
                let finished = if event.terminal() {
                    current.take()
                } else {
                    None
                };
                drop(current);
                if let AgentEvent::ToolRequest {
                    request_id,
                    run_id,
                    tool,
                    arguments,
                } = &event
                {
                    let broker = broker.clone();
                    let handle = handle.clone();
                    let process = Arc::clone(&process_for_results);
                    let request_id = request_id.clone();
                    let run_id = run_id.clone();
                    let tool = tool.clone();
                    let arguments = arguments.clone();
                    let authorized = allowed_tools.iter().any(|name| name == &tool);
                    std::thread::spawn(move || {
                        let line = match broker {
                            Some(broker) if authorized => broker.handle_request(
                                &handle,
                                &run_id,
                                &tool,
                                &arguments,
                                &request_id,
                            ),
                            _ => serde_json::json!({
                                "type": "tool_result",
                                "requestId": request_id,
                                "status": "denied",
                                "content": [{ "type": "text", "text": "本轮未授权该工具" }],
                                "details": serde_json::Value::Null,
                                "isError": true,
                            })
                            .to_string(),
                        };
                        if let Ok(mut guard) = process.lock() {
                            if let Some(process) = guard.as_mut() {
                                let _ = process.write_line(&line);
                            }
                        }
                    });
                    continue;
                }
                if let (Some(run), Some(broker)) = (&finished, broker.as_ref()) {
                    broker.deny_run(&run.run_id);
                }
                if let AgentEvent::Usage { usage, .. } = &event {
                    if let Ok(mut totals) = usage_totals.lock() {
                        totals.record_cumulative(usage);
                        let _ = handle.emit("token-usage-changed", totals.clone());
                    }
                }
                if let (Some(run), AgentEvent::Completed { reply, .. }) = (&finished, &event) {
                    let status = match handle.state::<crate::settings::SettingsStore>().get() {
                        Ok(settings)
                            if crate::history::may_persist(
                                run.private_at_start,
                                settings.privacy_mode,
                            ) =>
                        {
                            let turn = crate::history::HistoryTurn {
                                run_id: run.run_id.clone(),
                                question: run.question.clone(),
                                answer: reply.display_text.clone(),
                            };
                            if handle
                                .state::<crate::history::HistoryStore>()
                                .save(run.mode, &turn)
                                .is_ok()
                            {
                                "saved"
                            } else {
                                "failed"
                            }
                        }
                        Ok(_) => "private",
                        Err(_) => "failed",
                    };
                    let _ = handle.emit(
                        "history-event",
                        HistoryOutcome {
                            run_id: run.run_id.clone(),
                            status,
                        },
                    );
                    if handle
                        .state::<crate::settings::SettingsStore>()
                        .get()
                        .map(|settings| settings.voice_enabled)
                        .unwrap_or(false)
                    {
                        let voice = handle.state::<crate::voice::VoiceService>();
                        if run.birthday {
                            // Play the bundled birthday line directly instead of TTS.
                            voice.play_clip(
                                &handle,
                                run.run_id.clone(),
                                reply.emotion.clone(),
                                reply.gesture.clone(),
                            );
                        } else {
                            voice.synthesize(
                                &handle,
                                run.run_id.clone(),
                                reply.speech_text.clone(),
                                reply.emotion.clone(),
                                reply.gesture.clone(),
                            );
                        }
                    }
                }
                let _ = handle.emit("assistant-event", event);
            }
            if supervisor_generation.load(Ordering::SeqCst) == my_generation {
                if let Ok(mut applied) = applied_conversation.lock() {
                    *applied = None;
                }
                if let Ok(mut current) = active.lock() {
                    if let Some(run) = current.take() {
                        if let Some(broker) = broker.as_ref() {
                            broker.deny_run(&run.run_id);
                        }
                        let _ = handle.emit(
                            "assistant-event",
                            AgentEvent::Error {
                                run_id: run.run_id,
                                message: "Agent Sidecar 已退出；请重试".into(),
                            },
                        );
                    }
                }
            }
        });
        Ok(AgentProcess {
            child,
            stdin,
            key: key.to_owned(),
            model: model.to_owned(),
            search_proxy: search_proxy.to_owned(),
        })
    }
}

impl Drop for AgentSupervisor {
    fn drop(&mut self) {
        if let Ok(mut process) = self.process.lock() {
            if let Some(mut process) = process.take() {
                let _ = process.child.kill();
                let _ = process.child.wait();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{validate_prompt, AgentEvent, AgentSupervisor, TokenUsage, TokenUsageSnapshot};

    #[test]
    fn rejects_invalid_and_oversized_prompts() {
        assert!(validate_prompt("r-1", "你好").is_ok());
        assert!(validate_prompt("../../x", "你好").is_err());
        assert!(validate_prompt("r-1", "  ").is_err());
        assert!(validate_prompt("r-1", &"x".repeat(10_001)).is_err());
    }

    #[test]
    fn selected_file_is_scoped_to_supported_small_text() {
        let supervisor = AgentSupervisor::default();
        let path =
            std::env::temp_dir().join(format!("phoebe-agent-file-test-{}.txt", std::process::id()));
        std::fs::write(&path, "测试文本").unwrap();
        assert!(supervisor.attach_file(&path).is_ok());
        assert_eq!(
            supervisor
                .file_attachment
                .lock()
                .unwrap()
                .take()
                .unwrap()
                .content,
            "测试文本"
        );
        std::fs::remove_file(&path).unwrap();
        assert!(supervisor
            .attach_file(std::path::Path::new("/private/not-selected.bin"))
            .is_err());
    }

    #[test]
    fn raw_sidecar_events_are_narrowed_before_ui_delivery() {
        let parsed = serde_json::from_str::<AgentEvent>(
            r#"{"type":"text_delta","runId":"r-1","delta":"好","secret":"ignored"}"#,
        )
        .unwrap();
        let output = serde_json::to_string(&parsed).unwrap();
        assert!(!output.contains("secret"));
        assert!(
            serde_json::from_str::<AgentEvent>(r#"{"type":"tool_started","runId":"r-1"}"#).is_err()
        );
    }

    #[test]
    fn context_usage_snapshot_defaults_to_zero() {
        let supervisor = AgentSupervisor::default();
        let snapshot = supervisor.context_usage_snapshot().unwrap();
        assert_eq!(snapshot.estimated, 0);
        assert_eq!(snapshot.budget, 0);
    }

    #[test]
    fn sidecar_context_usage_is_parsed() {
        let parsed = serde_json::from_str::<AgentEvent>(
            r#"{"type":"context_usage","runId":"r-1","estimated":1234,"budget":24000}"#,
        )
        .unwrap();
        match parsed {
            AgentEvent::ContextUsage {
                run_id,
                estimated,
                budget,
            } => {
                assert_eq!(run_id, "r-1");
                assert_eq!(estimated, 1234);
                assert_eq!(budget, 24_000);
            }
            _ => panic!("expected context usage"),
        }
    }

    #[test]
    fn sidecar_tool_requests_are_parsed_with_their_wire_fields() {
        let parsed = serde_json::from_str::<AgentEvent>(
            r#"{"type":"tool_request","requestId":"req-1","runId":"r-1","tool":"open_url","arguments":{"url":"https://example.com"}}"#,
        )
        .unwrap();
        match parsed {
            AgentEvent::ToolRequest {
                request_id,
                run_id,
                tool,
                arguments,
            } => {
                assert_eq!(request_id, "req-1");
                assert_eq!(run_id, "r-1");
                assert_eq!(tool, "open_url");
                assert_eq!(arguments["url"], "https://example.com");
            }
            _ => panic!("expected a tool request"),
        }
    }

    #[test]
    fn cumulative_run_usage_only_adds_new_tokens_to_session() {
        let mut snapshot = TokenUsageSnapshot::default();
        snapshot.record_cumulative(&TokenUsage {
            input: 100,
            output: 20,
            cache_read: 10,
            cache_write: 0,
            total_tokens: 130,
        });
        snapshot.record_cumulative(&TokenUsage {
            input: 240,
            output: 50,
            cache_read: 30,
            cache_write: 5,
            total_tokens: 325,
        });
        assert_eq!(snapshot.current.total_tokens, 325);
        assert_eq!(snapshot.session.total_tokens, 325);
        snapshot.current = TokenUsage::default();
        snapshot.record_cumulative(&TokenUsage {
            input: 12,
            output: 3,
            cache_read: 0,
            cache_write: 0,
            total_tokens: 15,
        });
        assert_eq!(snapshot.session.total_tokens, 340);
    }
}
