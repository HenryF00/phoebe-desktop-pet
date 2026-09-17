mod agent;
mod history;
pub mod secure_store;
mod settings;
mod tools;
mod voice;
mod window_manager;

use agent::AgentSupervisor;
use history::HistoryStore;
use serde::Serialize;
use tauri::{Emitter, Manager};
use tauri_plugin_autostart::ManagerExt;
use tauri_plugin_dialog::{DialogExt, MessageDialogButtons, MessageDialogKind};
use voice::VoiceService;

#[derive(Clone, Serialize)]
struct AutostartStatus {
    enabled: bool,
    available: bool,
}

#[derive(Clone, Serialize)]
struct QuickPreferences {
    interaction_mode: settings::InteractionMode,
    voice_enabled: bool,
    pet_scale_percent: u16,
}

impl From<settings::Settings> for QuickPreferences {
    fn from(settings: settings::Settings) -> Self {
        Self {
            interaction_mode: settings.interaction_mode,
            voice_enabled: settings.voice_enabled,
            pet_scale_percent: settings.pet_scale_percent,
        }
    }
}

fn autostart_available(app: &tauri::AppHandle) -> bool {
    !cfg!(debug_assertions) && app.config().bundle.active
}

#[derive(Serialize)]
struct SystemStatus {
    platform: &'static str,
    agent: &'static str,
    voice: String,
    version: &'static str,
}

#[tauri::command]
async fn get_system_status(window: tauri::WebviewWindow) -> Result<SystemStatus, String> {
    require_any_caller(&window, &["chat", "settings"])?;
    let app = window.app_handle().clone();
    let agent = tauri::async_runtime::spawn_blocking(move || {
        agent::key_status(&app.state::<secure_store::SecureStore>())
    })
    .await
    .unwrap_or("failed");
    Ok(SystemStatus {
        platform: std::env::consts::OS,
        agent,
        voice: window
            .app_handle()
            .state::<VoiceService>()
            .status()
            .to_owned(),
        version: env!("CARGO_PKG_VERSION"),
    })
}

#[tauri::command]
async fn set_deepseek_api_key(
    window: tauri::WebviewWindow,
    app: tauri::AppHandle,
    key: String,
) -> Result<(), String> {
    require_caller(&window, "settings")?;
    let key_app = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        key_app.state::<secure_store::SecureStore>().set_key(&key)
    })
    .await
    .map_err(|_| "系统安全存储线程不可用".to_owned())?
    .map_err(str::to_owned)?;
    let _ = app.emit("agent-key-status", ());
    Ok(())
}

fn require_any_caller(window: &tauri::WebviewWindow, expected: &[&str]) -> Result<(), String> {
    if expected.contains(&window.label()) {
        Ok(())
    } else {
        Err("当前窗口没有执行该操作的权限".into())
    }
}

#[tauri::command]
fn show_settings(window: tauri::WebviewWindow, app: tauri::AppHandle) -> Result<(), String> {
    require_any_caller(&window, &["pet", "menu"])?;
    window_manager::show_settings(&app)
}

#[tauri::command]
fn show_pet_menu(window: tauri::WebviewWindow, app: tauri::AppHandle) -> Result<(), String> {
    require_caller(&window, "pet")?;
    window_manager::show_pet_menu(&app)
}

#[tauri::command]
fn hide_pet_menu(
    window: tauri::WebviewWindow,
    app: tauri::AppHandle,
    return_focus: bool,
) -> Result<(), String> {
    require_caller(&window, "menu")?;
    window_manager::hide_pet_menu(&app, return_focus)
}

#[tauri::command]
fn wave_from_menu(window: tauri::WebviewWindow, app: tauri::AppHandle) -> Result<(), String> {
    require_caller(&window, "menu")?;
    app.emit("pet-wave", ())
        .map_err(|_| "无法通知桌宠挥手".to_owned())
}

#[tauri::command]
fn hide_settings(window: tauri::WebviewWindow, app: tauri::AppHandle) -> Result<(), String> {
    require_caller(&window, "settings")?;
    window_manager::hide_settings(&app)
}

#[tauri::command]
fn get_settings(
    window: tauri::WebviewWindow,
    app: tauri::AppHandle,
) -> Result<settings::Settings, String> {
    require_caller(&window, "settings")?;
    app.state::<settings::SettingsStore>().get()
}

#[tauri::command]
fn get_quick_preferences(
    window: tauri::WebviewWindow,
    app: tauri::AppHandle,
) -> Result<QuickPreferences, String> {
    require_any_caller(&window, &["pet", "menu"])?;
    app.state::<settings::SettingsStore>().get().map(Into::into)
}

#[tauri::command]
fn get_token_usage(
    window: tauri::WebviewWindow,
    app: tauri::AppHandle,
) -> Result<agent::TokenUsageSnapshot, String> {
    require_any_caller(&window, &["pet", "menu"])?;
    app.state::<AgentSupervisor>().usage_snapshot()
}

#[tauri::command]
fn get_context_usage(
    window: tauri::WebviewWindow,
    app: tauri::AppHandle,
) -> Result<agent::ContextUsageSnapshot, String> {
    require_any_caller(&window, &["pet", "menu"])?;
    app.state::<AgentSupervisor>().context_usage_snapshot()
}

#[tauri::command]
fn set_pet_scale(
    window: tauri::WebviewWindow,
    app: tauri::AppHandle,
    pet_scale_percent: u16,
) -> Result<QuickPreferences, String> {
    require_any_caller(&window, &["pet", "menu"])?;
    if !settings::valid_pet_scale_percent(pet_scale_percent) {
        return Err("仅支持预设的桌宠缩放比例".into());
    }
    let store = app.state::<settings::SettingsStore>();
    let previous = store.get()?.pet_scale_percent;
    window_manager::apply_pet_scale(&app, pet_scale_percent)?;
    let settings = match store.update_pet_scale(&app, pet_scale_percent) {
        Ok(settings) => settings,
        Err(error) => {
            let _ = window_manager::apply_pet_scale(&app, previous);
            return Err(error);
        }
    };
    let _ = window_manager::persist_pet_position(&app);
    let preferences = QuickPreferences::from(settings);
    app.emit("quick-preferences-changed", preferences.clone())
        .map_err(|_| "无法同步桌宠大小".to_owned())?;
    Ok(preferences)
}

#[tauri::command]
fn update_quick_preferences(
    window: tauri::WebviewWindow,
    app: tauri::AppHandle,
    interaction_mode: settings::InteractionMode,
    voice_enabled: bool,
) -> Result<QuickPreferences, String> {
    require_any_caller(&window, &["pet", "menu"])?;
    let settings = app
        .state::<settings::SettingsStore>()
        .update_quick_preferences(&app, interaction_mode, voice_enabled)?;
    if !voice_enabled {
        app.state::<VoiceService>().stop(&app);
    } else {
        VoiceService::prewarm(&app);
    }
    let preferences = QuickPreferences::from(settings);
    app.emit("quick-preferences-changed", preferences.clone())
        .map_err(|_| "无法同步快捷设置".to_owned())?;
    Ok(preferences)
}

#[tauri::command]
async fn get_recent_history(
    window: tauri::WebviewWindow,
    app: tauri::AppHandle,
) -> Result<Vec<history::HistoryTurn>, String> {
    require_caller(&window, "chat")?;
    tauri::async_runtime::spawn_blocking(move || app.state::<HistoryStore>().recent())
        .await
        .map_err(|_| "历史数据库线程不可用".to_owned())?
}

#[tauri::command]
async fn list_conversations(
    window: tauri::WebviewWindow,
    app: tauri::AppHandle,
) -> Result<Vec<history::Conversation>, String> {
    require_caller(&window, "chat")?;
    tauri::async_runtime::spawn_blocking(move || app.state::<HistoryStore>().list())
        .await
        .map_err(|_| "历史数据库线程不可用".to_owned())?
}

#[tauri::command]
async fn get_active_conversation(
    window: tauri::WebviewWindow,
    app: tauri::AppHandle,
) -> Result<history::ActiveConversation, String> {
    require_caller(&window, "chat")?;
    tauri::async_runtime::spawn_blocking(move || app.state::<HistoryStore>().active())
        .await
        .map_err(|_| "历史数据库线程不可用".to_owned())?
}

#[tauri::command]
async fn create_conversation(
    window: tauri::WebviewWindow,
    app: tauri::AppHandle,
) -> Result<history::ActiveConversation, String> {
    require_caller(&window, "chat")?;
    tauri::async_runtime::spawn_blocking(move || {
        if app.state::<AgentSupervisor>().is_busy() {
            return Err("请先结束或停止当前回复，再新建会话".into());
        }
        app.state::<HistoryStore>().create()
    })
    .await
    .map_err(|_| "历史数据库线程不可用".to_owned())?
}

#[tauri::command]
async fn switch_conversation(
    window: tauri::WebviewWindow,
    app: tauri::AppHandle,
    id: String,
) -> Result<history::ActiveConversation, String> {
    require_caller(&window, "chat")?;
    tauri::async_runtime::spawn_blocking(move || {
        if app.state::<AgentSupervisor>().is_busy() {
            return Err("请先结束或停止当前回复，再切换会话".into());
        }
        app.state::<HistoryStore>().switch(&id)
    })
    .await
    .map_err(|_| "历史数据库线程不可用".to_owned())?
}

#[tauri::command]
async fn rename_conversation(
    window: tauri::WebviewWindow,
    app: tauri::AppHandle,
    id: String,
    title: String,
) -> Result<history::ActiveConversation, String> {
    require_caller(&window, "chat")?;
    tauri::async_runtime::spawn_blocking(move || {
        app.state::<HistoryStore>().rename(&id, &title)?;
        app.state::<HistoryStore>().active()
    })
    .await
    .map_err(|_| "历史数据库线程不可用".to_owned())?
}

#[tauri::command]
async fn delete_conversation(
    window: tauri::WebviewWindow,
    app: tauri::AppHandle,
    id: String,
) -> Result<history::ActiveConversation, String> {
    require_caller(&window, "chat")?;
    tauri::async_runtime::spawn_blocking(move || {
        if app.state::<AgentSupervisor>().is_busy() {
            return Err("请先结束或停止当前回复，再删除会话".into());
        }
        let target = app
            .state::<HistoryStore>()
            .list()?
            .into_iter()
            .find(|conversation| conversation.id == id)
            .ok_or("该会话不存在；请刷新列表")?;
        let title = if target.title.trim().is_empty() { "（未命名会话）" } else { target.title.as_str() };
        let confirmed = app
            .dialog()
            .message(format!("删除会话「{}」？此操作无法在应用内撤销，该会话的文字记录将被移除。", title))
            .title("删除菲比助手会话")
            .parent(&window)
            .kind(MessageDialogKind::Warning)
            .buttons(MessageDialogButtons::YesNo)
            .blocking_show();
        if !confirmed {
            return Ok(app.state::<HistoryStore>().active()?);
        }
        app.state::<HistoryStore>().delete(&id)
    })
    .await
    .map_err(|_| "历史数据库线程不可用".to_owned())?
}

#[tauri::command]
async fn get_explicit_memories(
    window: tauri::WebviewWindow,
    app: tauri::AppHandle,
) -> Result<Vec<history::ExplicitMemory>, String> {
    require_caller(&window, "settings")?;
    tauri::async_runtime::spawn_blocking(move || app.state::<HistoryStore>().memories())
        .await
        .map_err(|_| "记忆数据库线程不可用".to_owned())?
}

#[tauri::command]
async fn save_explicit_memory(
    window: tauri::WebviewWindow,
    app: tauri::AppHandle,
    id: Option<String>,
    title: String,
    content: String,
) -> Result<bool, String> {
    require_caller(&window, "settings")?;
    history::validate_memory(&title, &content)?;
    tauri::async_runtime::spawn_blocking(move || {
        if let Some(ref id) = id {
            if !app.state::<HistoryStore>().memories()?.iter().any(|memory| &memory.id == id) {
                return Err("该记忆不存在；请刷新列表".into());
            }
        }
        let action = if id.is_some() { "修改" } else { "新增" };
        let confirmed_content = serde_json::to_string(content.trim()).map_err(|_| "记忆内容不可确认")?;
        let confirmed = app.dialog()
            .message(format!("确认{action}这条长期记忆？\n\n标题：{}\n内容：{}\n\n只保存你明确指定的偏好，不自动从对话推断。", title.trim(), confirmed_content))
            .title("确认菲比助手长期记忆")
            .parent(&window)
            .kind(MessageDialogKind::Warning)
            .buttons(MessageDialogButtons::YesNo)
            .blocking_show();
        if !confirmed { return Ok(false); }
        app.state::<HistoryStore>().upsert_memory(id.as_deref(), &title, &content)?;
        Ok(true)
    }).await.map_err(|_| "记忆数据库线程不可用".to_owned())?
}

#[tauri::command]
async fn delete_explicit_memory(
    window: tauri::WebviewWindow,
    app: tauri::AppHandle,
    id: String,
) -> Result<bool, String> {
    require_caller(&window, "settings")?;
    tauri::async_runtime::spawn_blocking(move || {
        let memory = app
            .state::<HistoryStore>()
            .memories()?
            .into_iter()
            .find(|memory| memory.id == id)
            .ok_or("该记忆不存在；请刷新列表")?;
        let confirmed_content =
            serde_json::to_string(&memory.content).map_err(|_| "记忆内容不可确认")?;
        let confirmed = app
            .dialog()
            .message(format!(
                "确定删除这条长期记忆？\n\n标题：{}\n内容：{}\n\n此操作无法在应用内撤销。",
                memory.title, confirmed_content
            ))
            .title("删除菲比助手长期记忆")
            .parent(&window)
            .kind(MessageDialogKind::Warning)
            .buttons(MessageDialogButtons::YesNo)
            .blocking_show();
        if !confirmed {
            return Ok(false);
        }
        app.state::<HistoryStore>().delete_memory(&id)?;
        Ok(true)
    })
    .await
    .map_err(|_| "记忆数据库线程不可用".to_owned())?
}

#[tauri::command]
async fn clear_chat_history(
    window: tauri::WebviewWindow,
    app: tauri::AppHandle,
) -> Result<bool, String> {
    require_caller(&window, "chat")?;
    tauri::async_runtime::spawn_blocking(move || {
        if app.state::<AgentSupervisor>().is_busy() {
            return Err("请先结束或停止当前回复，再清理历史".into());
        }
        let confirmed = app.dialog()
            .message("清理当前会话已保存的全部文字聊天历史？此操作无法在应用内撤销。未保存的对话仍保留在当前窗口。")
            .title("清理菲比助手聊天历史")
            .parent(&window)
            .kind(MessageDialogKind::Warning)
            .buttons(MessageDialogButtons::YesNo)
            .blocking_show();
        if !confirmed { return Ok(false); }
        if app.state::<AgentSupervisor>().is_busy() {
            return Err("确认期间开始了新回复；历史没有被清理".into());
        }
        app.state::<HistoryStore>().clear()?;
        Ok(true)
    })
        .await.map_err(|_| "历史数据库线程不可用".to_owned())?
}

#[tauri::command]
fn update_settings(
    window: tauri::WebviewWindow,
    app: tauri::AppHandle,
    model: String,
    privacy_mode: bool,
    search_proxy: String,
    voice_enabled: bool,
    interaction_mode: settings::InteractionMode,
    action_mode: settings::ActionMode,
) -> Result<settings::Settings, String> {
    require_caller(&window, "settings")?;
    let settings = app.state::<settings::SettingsStore>().update(
        &app,
        model,
        privacy_mode,
        search_proxy,
        voice_enabled,
        interaction_mode,
        action_mode,
    )?;
    if !settings.voice_enabled {
        app.state::<VoiceService>().stop(&app);
    } else {
        VoiceService::prewarm(&app);
    }
    app.emit(
        "quick-preferences-changed",
        QuickPreferences::from(settings.clone()),
    )
    .map_err(|_| "无法同步快捷设置".to_owned())?;
    Ok(settings)
}

#[tauri::command]
async fn check_voice_service(
    window: tauri::WebviewWindow,
    app: tauri::AppHandle,
) -> Result<voice::VoiceDiagnostic, String> {
    require_caller(&window, "settings")?;
    let handle = app.clone();
    let diagnostic =
        tauri::async_runtime::spawn_blocking(move || handle.state::<VoiceService>().check(&handle))
            .await
            .map_err(|_| "语音检查线程不可用".to_owned())?;
    let _ = app.emit("voice-status", diagnostic.status);
    Ok(diagnostic)
}

#[tauri::command]
fn stop_voice(window: tauri::WebviewWindow, app: tauri::AppHandle) -> Result<(), String> {
    require_caller(&window, "chat")?;
    app.state::<VoiceService>().stop(&app);
    Ok(())
}

#[tauri::command]
fn replay_voice(
    window: tauri::WebviewWindow,
    app: tauri::AppHandle,
    run_id: String,
    text: String,
    emotion: String,
    gesture: String,
) -> Result<(), String> {
    require_caller(&window, "chat")?;
    if run_id.is_empty() || run_id.len() > 128 {
        return Err("语音消息标识无效".into());
    }
    if !matches!(emotion.as_str(), "calm" | "happy" | "shy" | "concerned")
        || !matches!(gesture.as_str(), "idle" | "nod" | "point")
    {
        return Err("语音动作提示无效".into());
    }
    app.state::<VoiceService>().synthesize(
        &app,
        format!("replay-{run_id}"),
        text,
        emotion,
        gesture,
    );
    Ok(())
}

#[tauri::command]
fn get_autostart_status(
    window: tauri::WebviewWindow,
    app: tauri::AppHandle,
) -> Result<AutostartStatus, String> {
    require_caller(&window, "settings")?;
    if !autostart_available(&app) {
        return Ok(AutostartStatus {
            enabled: false,
            available: false,
        });
    }
    Ok(AutostartStatus {
        enabled: app
            .autolaunch()
            .is_enabled()
            .map_err(|_| "无法读取系统登录项")?,
        available: true,
    })
}

#[tauri::command]
fn set_autostart(
    window: tauri::WebviewWindow,
    app: tauri::AppHandle,
    enabled: bool,
) -> Result<AutostartStatus, String> {
    require_caller(&window, "settings")?;
    if !autostart_available(&app) {
        return Err("当前构建未打包正式安装包，不注册开机启动".into());
    }
    if enabled {
        app.autolaunch().enable()
    } else {
        app.autolaunch().disable()
    }
    .map_err(|_| "无法更新系统登录项，请检查系统权限")?;
    get_autostart_status(window, app)
}

fn require_caller(window: &tauri::WebviewWindow, expected: &str) -> Result<(), String> {
    if is_allowed_caller(window.label(), expected) {
        Ok(())
    } else {
        Err("当前窗口没有执行该操作的权限".to_owned())
    }
}

fn is_allowed_caller(actual: &str, expected: &str) -> bool {
    actual == expected
}

#[tauri::command]
fn get_chat_visibility(
    window: tauri::WebviewWindow,
    app: tauri::AppHandle,
) -> Result<bool, String> {
    require_any_caller(&window, &["pet", "menu"])?;
    window_manager::is_chat_visible(&app)
}

#[tauri::command]
fn toggle_chat(window: tauri::WebviewWindow, app: tauri::AppHandle) -> Result<bool, String> {
    require_any_caller(&window, &["pet", "menu"])?;
    window_manager::toggle_chat(&app)
}

#[tauri::command]
fn hide_chat(window: tauri::WebviewWindow, app: tauri::AppHandle) -> Result<(), String> {
    require_caller(&window, "chat")?;
    window_manager::hide_chat(&app)
}

#[tauri::command]
fn start_pet_drag(window: tauri::WebviewWindow, app: tauri::AppHandle) -> Result<(), String> {
    require_caller(&window, "pet")?;
    window
        .start_dragging()
        .map_err(|_| "无法拖动桌宠".to_owned())?;
    window_manager::enable_position_saves(&app);
    Ok(())
}

#[tauri::command]
fn hide_pet(window: tauri::WebviewWindow, app: tauri::AppHandle) -> Result<(), String> {
    require_any_caller(&window, &["pet", "menu"])?;
    window_manager::hide_pet(&app)
}

#[tauri::command]
fn quit_application(window: tauri::WebviewWindow, app: tauri::AppHandle) -> Result<(), String> {
    require_any_caller(&window, &["pet", "menu"])?;
    app.exit(0);
    Ok(())
}

#[tauri::command]
async fn prompt_agent(
    window: tauri::WebviewWindow,
    app: tauri::AppHandle,
    run_id: String,
    text: String,
) -> Result<(), String> {
    require_caller(&window, "chat")?;
    tauri::async_runtime::spawn_blocking(move || {
        app.state::<AgentSupervisor>().prompt(&app, &run_id, &text)
    })
    .await
    .map_err(|_| "Agent 工作线程不可用".to_owned())?
}

#[tauri::command]
async fn select_agent_file(
    window: tauri::WebviewWindow,
    app: tauri::AppHandle,
) -> Result<Option<String>, String> {
    require_caller(&window, "chat")?;
    tauri::async_runtime::spawn_blocking(move || {
        let path = app
            .dialog()
            .file()
            .set_title("选择供菲比读取的文本文件")
            .set_parent(&window)
            .add_filter("文本文件", &["txt", "md", "csv", "json", "log"])
            .blocking_pick_file();
        let Some(path) = path else {
            return Ok(None);
        };
        let path = path.into_path().map_err(|_| "所选路径不是本机文件")?;
        app.state::<AgentSupervisor>().attach_file(&path).map(Some)
    })
    .await
    .map_err(|_| "文件选择线程不可用".to_owned())?
}

#[tauri::command]
async fn select_folder_grant(
    window: tauri::WebviewWindow,
    app: tauri::AppHandle,
    writable: bool,
) -> Result<Option<tools::FolderGrant>, String> {
    require_caller(&window, "chat")?;
    tauri::async_runtime::spawn_blocking(move || {
        let path = app
            .dialog()
            .file()
            .set_title(if writable {
                "选择要授权给菲比读写的文件夹"
            } else {
                "选择要授权给菲比读取的文件夹"
            })
            .set_parent(&window)
            .blocking_pick_folder();
        let Some(path) = path else {
            return Ok(None);
        };
        let path = path.into_path().map_err(|_| "所选路径不是本机文件夹")?;
        app.state::<tools::ToolBroker>()
            .add_folder(&app, &path, true, writable)
            .map(Some)
    })
    .await
    .map_err(|_| "文件夹选择线程不可用".to_owned())?
}

#[tauri::command]
fn list_folder_grants(
    window: tauri::WebviewWindow,
    app: tauri::AppHandle,
) -> Result<Vec<tools::FolderGrant>, String> {
    require_any_caller(&window, &["chat", "settings"])?;
    Ok(app.state::<tools::ToolBroker>().grant_views())
}

#[tauri::command]
fn revoke_folder_grant(
    window: tauri::WebviewWindow,
    app: tauri::AppHandle,
    grant_id: String,
) -> Result<(), String> {
    require_caller(&window, "settings")?;
    if grant_id.len() != 32 || !grant_id.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err("目录授权标识无效".into());
    }
    app.state::<tools::ToolBroker>().revoke_folder(&app, &grant_id)
}

#[tauri::command]
fn attach_device_location(
    window: tauri::WebviewWindow,
    app: tauri::AppHandle,
    latitude: f64,
    longitude: f64,
    accuracy_meters: f64,
) -> Result<String, String> {
    require_caller(&window, "chat")?;
    app.state::<AgentSupervisor>()
        .attach_location(latitude, longitude, accuracy_meters)
}

#[tauri::command]
fn clear_agent_attachments(
    window: tauri::WebviewWindow,
    app: tauri::AppHandle,
) -> Result<(), String> {
    require_caller(&window, "chat")?;
    app.state::<AgentSupervisor>().clear_attachments()
}

#[tauri::command]
async fn cancel_agent(
    window: tauri::WebviewWindow,
    app: tauri::AppHandle,
    run_id: String,
) -> Result<(), String> {
    require_caller(&window, "chat")?;
    tauri::async_runtime::spawn_blocking(move || {
        app.state::<AgentSupervisor>().cancel(&app, &run_id)
    })
    .await
    .map_err(|_| "Agent 工作线程不可用".to_owned())?
}

#[tauri::command]
fn resolve_tool_approval(
    window: tauri::WebviewWindow,
    app: tauri::AppHandle,
    approval_id: String,
    decision: String,
) -> Result<(), String> {
    require_caller(&window, "chat")?;
    if approval_id.is_empty() || approval_id.len() > 64 {
        return Err("审批标识无效".into());
    }
    app.state::<tools::ToolBroker>().resolve(&approval_id, &decision)
}

#[tauri::command]
fn get_pending_approval(
    window: tauri::WebviewWindow,
    app: tauri::AppHandle,
) -> Result<Option<tools::ApprovalView>, String> {
    require_caller(&window, "chat")?;
    Ok(app.state::<tools::ToolBroker>().pending_view())
}

#[tauri::command]
fn get_audit_log(
    window: tauri::WebviewWindow,
    app: tauri::AppHandle,
    limit: usize,
) -> Result<Vec<tools::AuditEntry>, String> {
    require_caller(&window, "settings")?;
    Ok(app.state::<tools::ToolBroker>().recent_audit(limit.min(100)))
}

#[tauri::command]
fn panic_stop_operations(
    window: tauri::WebviewWindow,
    app: tauri::AppHandle,
) -> Result<(), String> {
    require_any_caller(&window, &["chat", "menu", "settings"])?;
    app.state::<tools::ToolBroker>().deny_all();
    let _ = app.state::<AgentSupervisor>().cancel_active(&app);
    Ok(())
}

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            None,
        ))
        .plugin(tauri_plugin_dialog::init())
        .manage(AgentSupervisor::default())
        .manage(secure_store::SecureStore::default())
        .manage(VoiceService::default())
        .manage(HistoryStore::default())
        .manage(settings::SettingsStore::default())
        .manage(window_manager::PositionDebouncer::default())
        .manage(tools::ToolBroker::default())
        .setup(|app| {
            if let Err(error) = app.state::<tools::ToolBroker>().load(app.handle()) {
                eprintln!("tool broker audit/grant load failed: {error}");
            }
            let pet_scale_percent = {
                let store = app.state::<settings::SettingsStore>();
                if let Err(error) = store.load(app.handle()) {
                    store.record_load_error(error);
                }
                store
                    .get()
                    .map(|settings| settings.pet_scale_percent)
                    .unwrap_or(settings::DEFAULT_PET_SCALE_PERCENT)
            };
            let _ = window_manager::apply_pet_scale(app.handle(), pet_scale_percent);
            let _ = app.state::<HistoryStore>().open(app.handle());
            window_manager::setup_tray(app)?;
            if app
                .state::<settings::SettingsStore>()
                .get()
                .map(|settings| settings.voice_enabled)
                .unwrap_or(false)
            {
                VoiceService::prewarm(app.handle());
            }
            Ok(())
        })
        .on_window_event(window_manager::on_window_event)
        .invoke_handler(tauri::generate_handler![
            get_system_status,
            set_deepseek_api_key,
            resolve_tool_approval,
            get_pending_approval,
            get_audit_log,
            panic_stop_operations,
            prompt_agent,
            select_agent_file,
            select_folder_grant,
            list_folder_grants,
            revoke_folder_grant,
            attach_device_location,
            clear_agent_attachments,
            cancel_agent,
            get_chat_visibility,
            toggle_chat,
            hide_chat,
            start_pet_drag,
            hide_pet,
            quit_application,
            show_settings,
            show_pet_menu,
            hide_pet_menu,
            wave_from_menu,
            hide_settings,
            get_settings,
            get_quick_preferences,
            get_token_usage,
            get_context_usage,
            set_pet_scale,
            update_quick_preferences,
            update_settings,
            check_voice_service,
            stop_voice,
            replay_voice,
            get_autostart_status,
            set_autostart,
            get_recent_history,
            clear_chat_history,
            list_conversations,
            get_active_conversation,
            create_conversation,
            switch_conversation,
            rename_conversation,
            delete_conversation,
            get_explicit_memories,
            save_explicit_memory,
            delete_explicit_memory
        ])
        .build(tauri::generate_context!())
        .expect("failed to build Phoebe desktop app")
        .run(|app, event| {
            #[cfg(target_os = "macos")]
            if let tauri::RunEvent::Reopen { .. } = &event {
                let _ = window_manager::show_pet(app);
            }
            if let tauri::RunEvent::Exit = &event {
                app.state::<VoiceService>().shutdown();
            }
        });
}

#[cfg(test)]
mod tests {
    use super::is_allowed_caller;

    #[test]
    fn pet_cannot_invoke_chat_only_agent_commands() {
        assert!(is_allowed_caller("chat", "chat"));
        assert!(is_allowed_caller("pet", "pet"));
        assert!(is_allowed_caller("menu", "menu"));
        assert!(!is_allowed_caller("pet", "chat"));
        assert!(!is_allowed_caller("chat", "pet"));
        assert!(!is_allowed_caller("menu", "chat"));
        assert!(!is_allowed_caller("menu", "settings"));
    }
}
