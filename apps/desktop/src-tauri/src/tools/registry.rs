//! Static catalog of operating-system tools the Agent may request.
//!
//! Every tool parses its own arguments with `deny_unknown_fields`, so a model
//! cannot smuggle extra parameters past the gateway. Execution is deliberately
//! performed here in Rust, never inside the Node sidecar. File tools only ever
//! accept a grant id plus a relative path; absolute paths never reach the model.

use std::path::{Path, PathBuf};

use serde::Deserialize;
use tauri::Manager;
use url::Url;

use super::paths::{resolve_read_path, resolve_write_path, PathPolicy};
use super::{ToolContext, ToolOutcome};

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct EmptyArgs {}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct OpenUrlArgs {
    url: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct BrowserOpenArgs {
    url: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct BrowserRefArgs {
    #[serde(rename = "ref")]
    ref_id: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct BrowserTypeArgs {
    #[serde(rename = "ref")]
    ref_id: String,
    text: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct BrowserSelectArgs {
    #[serde(rename = "ref")]
    ref_id: String,
    value: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RememberPreferenceArgs {
    title: String,
    content: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ForgetPreferenceArgs {
    title: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct SystemPathArgs {
    path: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct WriteSystemArgs {
    path: String,
    content: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ListDirectoryArgs {
    grant_id: String,
    #[serde(default)]
    relative_path: String,
    #[serde(default)]
    max_entries: Option<u32>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ReadTextFileArgs {
    grant_id: String,
    #[serde(default)]
    relative_path: String,
    #[serde(default)]
    max_bytes: Option<u64>,
    #[serde(default)]
    offset: Option<u64>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct SearchFilesArgs {
    grant_id: String,
    #[serde(default)]
    relative_path: String,
    query: String,
    #[serde(default)]
    max_results: Option<u32>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct WriteFileArgs {
    grant_id: String,
    relative_path: String,
    content: String,
    #[serde(default)]
    create_only: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct MoveFileArgs {
    grant_id: String,
    from_relative_path: String,
    to_relative_path: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct DeleteFileArgs {
    grant_id: String,
    relative_path: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct AppIdArgs {
    app_id: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RevealFileArgs {
    grant_id: String,
    relative_path: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct OpenFileWithApplicationArgs {
    grant_id: String,
    relative_path: String,
    app_id: String,
}

#[derive(Debug)]
pub enum Parsed {
    Empty,
    OpenUrl { url: String, host: String },
    ListGrantedFolders,
    ListDirectory { grant_id: String, relative_path: String, max_entries: u32 },
    ReadTextFile { grant_id: String, relative_path: String, max_bytes: u64, offset: u64 },
    SearchFiles { grant_id: String, relative_path: String, query: String, max_results: u32 },
    WriteFile { grant_id: String, relative_path: String, content: String, create_only: bool },
    MoveFile { grant_id: String, from_path: String, to_path: String },
    DeleteFile { grant_id: String, relative_path: String },
    ListInstalledApps,
    LaunchApplication { app_id: String },
    FocusApplication { app_id: String },
    RevealFile { grant_id: String, relative_path: String },
    OpenFileWithApplication { grant_id: String, relative_path: String, app_id: String },
    BrowserOpen { url: String, host: String },
    BrowserSnapshot,
    BrowserClick { ref_id: String },
    BrowserType { ref_id: String, text: String },
    BrowserSelect { ref_id: String, value: String },
    BrowserWait,
    BrowserExtractText,
    BrowserClose,
    RememberPreference { title: String, content: String },
    ForgetPreference { title: String },
    LaunchWutheringWaves,
    ReadSystemFile { path: String },
    WriteSystemFile { path: String, content: String },
}

/// Description shown to the human in the approval dialog. Never sent to the model.
#[derive(Debug)]
pub struct Describe {
    pub target: String,
    pub impact: String,
    pub preview: Option<String>,
}

#[derive(Clone, Debug)]
pub struct GrantRequirement {
    pub grant_id: String,
    pub write: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tool {
    GetCurrentTime,
    GetSystemStatus,
    GetDeviceLocation,
    OpenUrl,
    ListGrantedFolders,
    ListDirectory,
    ReadTextFile,
    SearchFiles,
    WriteFile,
    MoveFile,
    DeleteFile,
    ListInstalledApps,
    LaunchApplication,
    FocusApplication,
    RevealFile,
    OpenFileWithApplication,
    BrowserOpen,
    BrowserSnapshot,
    BrowserClick,
    BrowserType,
    BrowserSelect,
    BrowserWait,
    BrowserExtractText,
    BrowserClose,
    RememberPreference,
    ForgetPreference,
    LaunchWutheringWaves,
    ReadSystemFile,
    WriteSystemFile,
}

const MAX_READ_BYTES: u64 = 32 * 1024;
const DEFAULT_READ_BYTES: u64 = 16 * 1024;
const MAX_FILE_BYTES: u64 = 512 * 1024;
const MAX_LIST_ENTRIES: u32 = 500;
const DEFAULT_LIST_ENTRIES: u32 = 200;
const MAX_SEARCH_RESULTS: u32 = 50;
const DEFAULT_SEARCH_RESULTS: u32 = 20;
const SEARCH_MAX_DEPTH: usize = 4;
const SEARCH_SCAN_LIMIT: usize = 2000;
const SEARCH_CONTENT_BYTES: u64 = 64 * 1024;
const MAX_WRITE_BYTES: u64 = 256 * 1024;
const MAX_SYSTEM_READ_BYTES: u64 = 64 * 1024;
const MAX_SYSTEM_WRITE_BYTES: u64 = 256 * 1024;

impl Tool {
    pub fn from_name(name: &str) -> Option<Self> {
        match name {
            "get_current_time" => Some(Self::GetCurrentTime),
            "get_system_status" => Some(Self::GetSystemStatus),
            "get_device_location" => Some(Self::GetDeviceLocation),
            "open_url" => Some(Self::OpenUrl),
            "list_granted_folders" => Some(Self::ListGrantedFolders),
            "list_directory" => Some(Self::ListDirectory),
            "read_text_file" => Some(Self::ReadTextFile),
            "search_files" => Some(Self::SearchFiles),
            "write_file" => Some(Self::WriteFile),
            "move_file" => Some(Self::MoveFile),
            "delete_file" => Some(Self::DeleteFile),
            "list_installed_apps" => Some(Self::ListInstalledApps),
            "launch_application" => Some(Self::LaunchApplication),
            "focus_application" => Some(Self::FocusApplication),
            "reveal_file" => Some(Self::RevealFile),
            "open_file_with_application" => Some(Self::OpenFileWithApplication),
            "browser_open" => Some(Self::BrowserOpen),
            "browser_snapshot" => Some(Self::BrowserSnapshot),
            "browser_click" => Some(Self::BrowserClick),
            "browser_type" => Some(Self::BrowserType),
            "browser_select" => Some(Self::BrowserSelect),
            "browser_wait" => Some(Self::BrowserWait),
            "browser_extract_text" => Some(Self::BrowserExtractText),
            "browser_close" => Some(Self::BrowserClose),
            "remember_preference" => Some(Self::RememberPreference),
            "forget_preference" => Some(Self::ForgetPreference),
            "launch_wuthering_waves" => Some(Self::LaunchWutheringWaves),
            "read_system_file" => Some(Self::ReadSystemFile),
            "write_system_file" => Some(Self::WriteSystemFile),
            _ => None,
        }
    }

    pub fn name(&self) -> &'static str {
        match self {
            Self::GetCurrentTime => "get_current_time",
            Self::GetSystemStatus => "get_system_status",
            Self::GetDeviceLocation => "get_device_location",
            Self::OpenUrl => "open_url",
            Self::ListGrantedFolders => "list_granted_folders",
            Self::ListDirectory => "list_directory",
            Self::ReadTextFile => "read_text_file",
            Self::SearchFiles => "search_files",
            Self::WriteFile => "write_file",
            Self::MoveFile => "move_file",
            Self::DeleteFile => "delete_file",
            Self::ListInstalledApps => "list_installed_apps",
            Self::LaunchApplication => "launch_application",
            Self::FocusApplication => "focus_application",
            Self::RevealFile => "reveal_file",
            Self::OpenFileWithApplication => "open_file_with_application",
            Self::BrowserOpen => "browser_open",
            Self::BrowserSnapshot => "browser_snapshot",
            Self::BrowserClick => "browser_click",
            Self::BrowserType => "browser_type",
            Self::BrowserSelect => "browser_select",
            Self::BrowserWait => "browser_wait",
            Self::BrowserExtractText => "browser_extract_text",
            Self::BrowserClose => "browser_close",
            Self::RememberPreference => "remember_preference",
            Self::ForgetPreference => "forget_preference",
            Self::LaunchWutheringWaves => "launch_wuthering_waves",
            Self::ReadSystemFile => "read_system_file",
            Self::WriteSystemFile => "write_system_file",
        }
    }

    /// Auto tools never require approval and are safe in every action mode.
    /// Browser actions after `browser_open` are auto: the page itself was
    /// already approved when it was opened.
    pub fn is_auto(&self) -> bool {
        matches!(
            self,
            Self::GetCurrentTime
                | Self::GetSystemStatus
                | Self::GetDeviceLocation
                | Self::ListGrantedFolders
                | Self::ListInstalledApps
                | Self::BrowserSnapshot
                | Self::BrowserClick
                | Self::BrowserType
                | Self::BrowserSelect
                | Self::BrowserWait
                | Self::BrowserExtractText
                | Self::BrowserClose
        )
    }

    /// Tools that must be confirmed on every call, even in trust mode.
    pub fn always_confirms(&self) -> bool {
        matches!(
            self,
            Self::WriteFile
                | Self::MoveFile
                | Self::DeleteFile
                | Self::RememberPreference
                | Self::ForgetPreference
                | Self::ReadSystemFile
                | Self::WriteSystemFile
        )
    }

    pub fn parse(&self, args: &serde_json::Value) -> Result<Parsed, String> {
        match self {
            Self::GetCurrentTime | Self::GetSystemStatus | Self::GetDeviceLocation => {
                serde_json::from_value::<EmptyArgs>(args.clone())
                    .map_err(|_| "参数包含未允许的字段".to_owned())?;
                Ok(Parsed::Empty)
            }
            Self::OpenUrl => {
                let parsed: OpenUrlArgs = serde_json::from_value(args.clone())
                    .map_err(|_| "参数包含未允许的字段".to_owned())?;
                let (url, host) = validate_https_url(&parsed.url)?;
                Ok(Parsed::OpenUrl { url, host })
            }
            Self::BrowserOpen => {
                let parsed: BrowserOpenArgs = serde_json::from_value(args.clone())
                    .map_err(|_| "参数包含未允许的字段".to_owned())?;
                let (url, host) = validate_https_url(&parsed.url)?;
                Ok(Parsed::BrowserOpen { url, host })
            }
            Self::BrowserSnapshot | Self::BrowserWait | Self::BrowserExtractText
            | Self::BrowserClose => {
                serde_json::from_value::<EmptyArgs>(args.clone())
                    .map_err(|_| "参数包含未允许的字段".to_owned())?;
                match self {
                    Self::BrowserSnapshot => Ok(Parsed::BrowserSnapshot),
                    Self::BrowserWait => Ok(Parsed::BrowserWait),
                    Self::BrowserExtractText => Ok(Parsed::BrowserExtractText),
                    _ => Ok(Parsed::BrowserClose),
                }
            }
            Self::BrowserClick => {
                let parsed: BrowserRefArgs = serde_json::from_value(args.clone())
                    .map_err(|_| "参数包含未允许的字段".to_owned())?;
                Ok(Parsed::BrowserClick { ref_id: validate_browser_ref(&parsed.ref_id)? })
            }
            Self::BrowserType => {
                let parsed: BrowserTypeArgs = serde_json::from_value(args.clone())
                    .map_err(|_| "参数包含未允许的字段".to_owned())?;
                if parsed.text.len() > 2000 {
                    return Err("输入文本过长".to_owned());
                }
                Ok(Parsed::BrowserType {
                    ref_id: validate_browser_ref(&parsed.ref_id)?,
                    text: parsed.text,
                })
            }
            Self::BrowserSelect => {
                let parsed: BrowserSelectArgs = serde_json::from_value(args.clone())
                    .map_err(|_| "参数包含未允许的字段".to_owned())?;
                if parsed.value.len() > 200 {
                    return Err("选项值过长".to_owned());
                }
                Ok(Parsed::BrowserSelect {
                    ref_id: validate_browser_ref(&parsed.ref_id)?,
                    value: parsed.value,
                })
            }
            Self::RememberPreference => {
                let parsed: RememberPreferenceArgs = serde_json::from_value(args.clone())
                    .map_err(|_| "参数包含未允许的字段".to_owned())?;
                crate::history::validate_memory(&parsed.title, &parsed.content)?;
                Ok(Parsed::RememberPreference {
                    title: parsed.title.trim().to_owned(),
                    content: parsed.content.trim().to_owned(),
                })
            }
            Self::ForgetPreference => {
                let parsed: ForgetPreferenceArgs = serde_json::from_value(args.clone())
                    .map_err(|_| "参数包含未允许的字段".to_owned())?;
                let title = parsed.title.trim().to_owned();
                if title.is_empty() || title.chars().any(char::is_control) {
                    return Err("记忆标题无效".to_owned());
                }
                Ok(Parsed::ForgetPreference { title })
            }
            Self::LaunchWutheringWaves => {
                serde_json::from_value::<EmptyArgs>(args.clone())
                    .map_err(|_| "参数包含未允许的字段".to_owned())?;
                Ok(Parsed::LaunchWutheringWaves)
            }
            Self::ReadSystemFile => {
                let parsed: SystemPathArgs = serde_json::from_value(args.clone())
                    .map_err(|_| "参数包含未允许的字段".to_owned())?;
                Ok(Parsed::ReadSystemFile {
                    path: validate_system_path(&parsed.path)?,
                })
            }
            Self::WriteSystemFile => {
                let parsed: WriteSystemArgs = serde_json::from_value(args.clone())
                    .map_err(|_| "参数包含未允许的字段".to_owned())?;
                if parsed.content.len() as u64 > MAX_SYSTEM_WRITE_BYTES {
                    return Err(format!(
                        "写入内容超过 {} KB 上限",
                        MAX_SYSTEM_WRITE_BYTES / 1024
                    ));
                }
                Ok(Parsed::WriteSystemFile {
                    path: validate_system_path(&parsed.path)?,
                    content: parsed.content,
                })
            }
            Self::ListGrantedFolders => {
                serde_json::from_value::<EmptyArgs>(args.clone())
                    .map_err(|_| "参数包含未允许的字段".to_owned())?;
                Ok(Parsed::ListGrantedFolders)
            }
            Self::ListDirectory => {
                let parsed: ListDirectoryArgs = serde_json::from_value(args.clone())
                    .map_err(|_| "参数包含未允许的字段".to_owned())?;
                Ok(Parsed::ListDirectory {
                    grant_id: validate_grant_id(&parsed.grant_id)?,
                    relative_path: validate_relative(&parsed.relative_path)?,
                    max_entries: parsed
                        .max_entries
                        .unwrap_or(DEFAULT_LIST_ENTRIES)
                        .clamp(1, MAX_LIST_ENTRIES),
                })
            }
            Self::ReadTextFile => {
                let parsed: ReadTextFileArgs = serde_json::from_value(args.clone())
                    .map_err(|_| "参数包含未允许的字段".to_owned())?;
                Ok(Parsed::ReadTextFile {
                    grant_id: validate_grant_id(&parsed.grant_id)?,
                    relative_path: validate_relative(&parsed.relative_path)?,
                    max_bytes: parsed
                        .max_bytes
                        .unwrap_or(DEFAULT_READ_BYTES)
                        .clamp(1, MAX_READ_BYTES),
                    offset: parsed.offset.unwrap_or(0).min(MAX_FILE_BYTES),
                })
            }
            Self::SearchFiles => {
                let parsed: SearchFilesArgs = serde_json::from_value(args.clone())
                    .map_err(|_| "参数包含未允许的字段".to_owned())?;
                let query = parsed.query.trim().to_owned();
                if query.is_empty() || query.chars().count() > 64 {
                    return Err("搜索词应为 1 到 64 个字符".to_owned());
                }
                Ok(Parsed::SearchFiles {
                    grant_id: validate_grant_id(&parsed.grant_id)?,
                    relative_path: validate_relative(&parsed.relative_path)?,
                    query,
                    max_results: parsed
                        .max_results
                        .unwrap_or(DEFAULT_SEARCH_RESULTS)
                        .clamp(1, MAX_SEARCH_RESULTS),
                })
            }
            Self::WriteFile => {
                let parsed: WriteFileArgs = serde_json::from_value(args.clone())
                    .map_err(|_| "参数包含未允许的字段".to_owned())?;
                if parsed.content.len() as u64 > MAX_WRITE_BYTES {
                    return Err(format!("写入内容超过 {} KB 上限", MAX_WRITE_BYTES / 1024));
                }
                Ok(Parsed::WriteFile {
                    grant_id: validate_grant_id(&parsed.grant_id)?,
                    relative_path: validate_non_empty_relative(&parsed.relative_path)?,
                    content: parsed.content,
                    create_only: parsed.create_only,
                })
            }
            Self::MoveFile => {
                let parsed: MoveFileArgs = serde_json::from_value(args.clone())
                    .map_err(|_| "参数包含未允许的字段".to_owned())?;
                Ok(Parsed::MoveFile {
                    grant_id: validate_grant_id(&parsed.grant_id)?,
                    from_path: validate_non_empty_relative(&parsed.from_relative_path)?,
                    to_path: validate_non_empty_relative(&parsed.to_relative_path)?,
                })
            }
            Self::DeleteFile => {
                let parsed: DeleteFileArgs = serde_json::from_value(args.clone())
                    .map_err(|_| "参数包含未允许的字段".to_owned())?;
                Ok(Parsed::DeleteFile {
                    grant_id: validate_grant_id(&parsed.grant_id)?,
                    relative_path: validate_non_empty_relative(&parsed.relative_path)?,
                })
            }
            Self::ListInstalledApps => {
                serde_json::from_value::<EmptyArgs>(args.clone())
                    .map_err(|_| "参数包含未允许的字段".to_owned())?;
                Ok(Parsed::ListInstalledApps)
            }
            Self::LaunchApplication => {
                let parsed: AppIdArgs = serde_json::from_value(args.clone())
                    .map_err(|_| "参数包含未允许的字段".to_owned())?;
                Ok(Parsed::LaunchApplication {
                    app_id: validate_app_id(&parsed.app_id)?,
                })
            }
            Self::FocusApplication => {
                let parsed: AppIdArgs = serde_json::from_value(args.clone())
                    .map_err(|_| "参数包含未允许的字段".to_owned())?;
                Ok(Parsed::FocusApplication {
                    app_id: validate_app_id(&parsed.app_id)?,
                })
            }
            Self::RevealFile => {
                let parsed: RevealFileArgs = serde_json::from_value(args.clone())
                    .map_err(|_| "参数包含未允许的字段".to_owned())?;
                Ok(Parsed::RevealFile {
                    grant_id: validate_grant_id(&parsed.grant_id)?,
                    relative_path: validate_non_empty_relative(&parsed.relative_path)?,
                })
            }
            Self::OpenFileWithApplication => {
                let parsed: OpenFileWithApplicationArgs = serde_json::from_value(args.clone())
                    .map_err(|_| "参数包含未允许的字段".to_owned())?;
                Ok(Parsed::OpenFileWithApplication {
                    grant_id: validate_grant_id(&parsed.grant_id)?,
                    relative_path: validate_non_empty_relative(&parsed.relative_path)?,
                    app_id: validate_app_id(&parsed.app_id)?,
                })
            }
        }
    }

    /// Persistent trust key used by "always allow" grants, when supported.
    pub fn bound_key(&self, parsed: &Parsed) -> Option<String> {
        match parsed {
            Parsed::OpenUrl { host, .. } | Parsed::BrowserOpen { host, .. } => {
                Some(format!("domain:{host}"))
            }
            Parsed::ListDirectory { grant_id, .. }
            | Parsed::ReadTextFile { grant_id, .. }
            | Parsed::SearchFiles { grant_id, .. }
            | Parsed::WriteFile { grant_id, .. }
            | Parsed::MoveFile { grant_id, .. }
            | Parsed::DeleteFile { grant_id, .. }
            | Parsed::RevealFile { grant_id, .. }
            | Parsed::OpenFileWithApplication { grant_id, .. } => Some(format!("folder:{grant_id}")),
            Parsed::LaunchApplication { app_id } | Parsed::FocusApplication { app_id } => {
                Some(format!("app:{app_id}"))
            }
            Parsed::LaunchWutheringWaves => Some("app:wuthering-waves".to_owned()),
            _ => None,
        }
    }

    pub fn required_grant(&self, parsed: &Parsed) -> Option<GrantRequirement> {
        match parsed {
            Parsed::ListDirectory { grant_id, .. }
            | Parsed::ReadTextFile { grant_id, .. }
            | Parsed::SearchFiles { grant_id, .. }
            | Parsed::RevealFile { grant_id, .. }
            | Parsed::OpenFileWithApplication { grant_id, .. } => Some(GrantRequirement {
                grant_id: grant_id.clone(),
                write: false,
            }),
            Parsed::WriteFile { grant_id, .. }
            | Parsed::MoveFile { grant_id, .. }
            | Parsed::DeleteFile { grant_id, .. } => Some(GrantRequirement {
                grant_id: grant_id.clone(),
                write: true,
            }),
            _ => None,
        }
    }

    /// The opaque application id a tool needs resolved from the catalog.
    pub fn required_app<'a>(&self, parsed: &'a Parsed) -> Option<&'a str> {
        match parsed {
            Parsed::LaunchApplication { app_id }
            | Parsed::FocusApplication { app_id }
            | Parsed::OpenFileWithApplication { app_id, .. } => Some(app_id),
            _ => None,
        }
    }

    pub fn is_app_listing(&self) -> bool {
        matches!(self, Self::ListInstalledApps)
    }

    /// Human-readable target and impact shown in the approval dialog.
    pub fn describe(&self, ctx: &ToolContext, parsed: &Parsed) -> Result<Describe, String> {
        match (self, parsed) {
            (Self::OpenUrl, Parsed::OpenUrl { url, host }) => Ok(Describe {
                target: host.clone(),
                impact: format!("用系统默认浏览器打开 {url}"),
                preview: None,
            }),
            (Self::BrowserOpen, Parsed::BrowserOpen { url, host }) => Ok(Describe {
                target: host.clone(),
                impact: format!("在受控浏览器中打开 {url}（无头模式，独立配置，不触碰你的登录会话）"),
                preview: None,
            }),
            (Self::BrowserSnapshot, _) => read_only_describe("受控浏览器页面快照"),
            (Self::BrowserClick, _) => read_only_describe("点击页面元素"),
            (Self::BrowserType, _) => read_only_describe("向页面输入文本"),
            (Self::BrowserSelect, _) => read_only_describe("选择下拉选项"),
            (Self::BrowserWait, _) => read_only_describe("等待页面加载"),
            (Self::BrowserExtractText, _) => read_only_describe("提取页面文本"),
            (Self::BrowserClose, _) => read_only_describe("关闭受控浏览器"),
            (Self::RememberPreference, Parsed::RememberPreference { title, content }) => {
                Ok(Describe {
                    target: format!("长期记忆：{title}"),
                    impact: format!("记住：{content}\n（将用于后续对话）"),
                    preview: None,
                })
            }
            (Self::ForgetPreference, Parsed::ForgetPreference { title }) => {
                let store = ctx.app.state::<crate::history::HistoryStore>();
                let memory = store
                    .memory_by_title(title)?
                    .ok_or("没有找到标题匹配的长期记忆；请先询问或列出记忆")?;
                let count = store.count_memories_by_title(title)?;
                Ok(Describe {
                    target: format!("长期记忆：{}", memory.title),
                    impact: if count > 1 {
                        format!("删除 {count} 条同名记忆（含：{}）", memory.content)
                    } else {
                        format!("删除：{}", memory.content)
                    },
                    preview: None,
                })
            }
            (Self::LaunchWutheringWaves, _) => {
                let entry = super::apps::find_wuthering_waves()
                    .ok_or("未找到鸣潮客户端；请确认已安装")?;
                Ok(Describe {
                    target: entry.name.clone(),
                    impact: format!("启动游戏：{}（{}）", entry.name, entry.path),
                    preview: None,
                })
            }
            (Self::ReadSystemFile, Parsed::ReadSystemFile { path }) => {
                let resolved = resolve_system_path(ctx.app, path)?;
                ensure_system_path_allowed(ctx.app, &resolved)?;
                Ok(Describe {
                    target: resolved.display().to_string(),
                    impact: "读取系统文件（只读，不产生副作用）".to_owned(),
                    preview: None,
                })
            }
            (Self::WriteSystemFile, Parsed::WriteSystemFile { path, content }) => {
                let resolved = resolve_system_path(ctx.app, path)?;
                ensure_system_path_allowed(ctx.app, &resolved)?;
                let exists = resolved.exists();
                let old = read_optional_text(&resolved);
                let mut preview = String::new();
                preview.push_str(if exists { "覆盖现有文件\n" } else { "新建文件\n" });
                preview.push_str(&bounded_diff(old.as_deref(), content));
                Ok(Describe {
                    target: resolved.display().to_string(),
                    impact: format!("写入系统文件（{} 字节）", content.len()),
                    preview: Some(preview),
                })
            }
            (Self::GetCurrentTime, _) => read_only_describe("本机时间"),
            (Self::GetSystemStatus, _) => read_only_describe("应用状态"),
            (Self::GetDeviceLocation, _) => read_only_describe("设备城市级位置"),
            (Self::ListGrantedFolders, _) => {
                let labels: Vec<String> = ctx.grants.iter().map(|grant| grant.label.clone()).collect();
                Ok(Describe {
                    target: format!("{} 个已授权文件夹", ctx.grants.len()),
                    impact: if labels.is_empty() {
                        "列出授权目录（当前为空）".to_owned()
                    } else {
                        format!("列出授权目录名称与权限：{}", labels.join("、"))
                    },
                    preview: None,
                })
            }
            (Self::ListDirectory, Parsed::ListDirectory { relative_path, .. }) => {
                let path = resolve_read(ctx, relative_path)?;
                Ok(Describe {
                    target: path.display().to_string(),
                    impact: format!("列出目录内容（相对路径 {}）", display_relative(relative_path)),
                    preview: None,
                })
            }
            (Self::ReadTextFile, Parsed::ReadTextFile { relative_path, .. }) => {
                let path = resolve_read(ctx, relative_path)?;
                Ok(Describe {
                    target: path.display().to_string(),
                    impact: format!("读取文本文件（相对路径 {}）", display_relative(relative_path)),
                    preview: None,
                })
            }
            (Self::SearchFiles, Parsed::SearchFiles { relative_path, query, .. }) => {
                let path = resolve_read(ctx, relative_path)?;
                Ok(Describe {
                    target: path.display().to_string(),
                    impact: format!(
                        "在相对路径 {} 下搜索“{query}”",
                        display_relative(relative_path)
                    ),
                    preview: None,
                })
            }
            (
                Self::WriteFile,
                Parsed::WriteFile {
                    relative_path,
                    content,
                    create_only,
                    ..
                },
            ) => {
                let path = resolve_write(ctx, relative_path)?;
                let exists = path.exists();
                if *create_only && exists {
                    return Err("目标文件已存在，createOnly 已阻止覆盖".to_owned());
                }
                let old = read_optional_text(&path);
                let mut preview = String::new();
                preview.push_str(if exists { "覆盖现有文件\n" } else { "新建文件\n" });
                preview.push_str(&bounded_diff(old.as_deref(), content));
                Ok(Describe {
                    target: path.display().to_string(),
                    impact: format!(
                        "写入文本文件（相对路径 {}，{} 字节）",
                        display_relative(relative_path),
                        content.len()
                    ),
                    preview: Some(preview),
                })
            }
            (Self::MoveFile, Parsed::MoveFile { from_path, to_path, .. }) => {
                let source = resolve_read(ctx, from_path)?;
                let destination = resolve_write(ctx, to_path)?;
                Ok(Describe {
                    target: destination.display().to_string(),
                    impact: format!(
                        "在同一授权目录内移动：{} → {}",
                        display_relative(from_path),
                        display_relative(to_path)
                    ),
                    preview: Some(format!("{} → {}", source.display(), destination.display())),
                })
            }
            (Self::DeleteFile, Parsed::DeleteFile { relative_path, .. }) => {
                let path = resolve_read(ctx, relative_path)?;
                Ok(Describe {
                    target: path.display().to_string(),
                    impact: "移入系统废纸篓（可恢复）".to_owned(),
                    preview: None,
                })
            }
            (Self::ListInstalledApps, _) => Ok(Describe {
                target: "已安装应用".to_owned(),
                impact: "列出应用名称与标识（只读，不启动）".to_owned(),
                preview: None,
            }),
            (Self::LaunchApplication, _) | (Self::FocusApplication, _) => {
                let entry = ctx.selected_app.as_ref().ok_or("未找到该应用")?;
                let verb = if matches!(self, Self::FocusApplication) { "激活" } else { "启动" };
                Ok(Describe {
                    target: entry.name.clone(),
                    impact: format!("{verb}应用：{}（{}）", entry.name, entry.path),
                    preview: None,
                })
            }
            (Self::RevealFile, Parsed::RevealFile { relative_path, .. }) => {
                let path = resolve_read(ctx, relative_path)?;
                Ok(Describe {
                    target: path.display().to_string(),
                    impact: format!(
                        "在文件管理器中显示（相对路径 {}）",
                        display_relative(relative_path)
                    ),
                    preview: None,
                })
            }
            (Self::OpenFileWithApplication, Parsed::OpenFileWithApplication { relative_path, .. }) => {
                let path = resolve_read(ctx, relative_path)?;
                let entry = ctx.selected_app.as_ref().ok_or("未找到该应用")?;
                Ok(Describe {
                    target: path.display().to_string(),
                    impact: format!(
                        "用 {} 打开（相对路径 {}）",
                        entry.name,
                        display_relative(relative_path)
                    ),
                    preview: None,
                })
            }
            _ => Err("工具参数不匹配".to_owned()),
        }
    }

    pub fn execute(&self, ctx: &ToolContext, parsed: &Parsed) -> Result<ToolOutcome, String> {
        match (self, parsed) {
            (Self::GetCurrentTime, _) => Ok(ToolOutcome::success(format!(
                "当前本地时间：{}",
                chrono::Local::now().format("%Y-%m-%d %H:%M:%S")
            ))),
            (Self::GetSystemStatus, _) => Ok(ToolOutcome::success(
                "桌面核心已运行；Agent 与语音状态请查看面板。".to_owned(),
            )),
            (Self::GetDeviceLocation, _) => {
                let location = super::location::current_location(ctx.app)?;
                Ok(ToolOutcome::success(format!(
                    "城市级粗略位置（约 0.1 度，非实时天气）：\n纬度 {}\n经度 {}\n精度约 {} 米\n获取时间 {}\n来源：操作系统原生定位，仅本次获取，不会持续跟踪。",
                    location.latitude,
                    location.longitude,
                    if location.accuracy_meters >= 0.0 {
                        format!("{:.0}", location.accuracy_meters)
                    } else {
                        "未知".to_owned()
                    },
                    location.captured_at
                )))
            }
            (Self::OpenUrl, Parsed::OpenUrl { url, .. }) => open_https(url),
            (Self::ListGrantedFolders, _) => Ok(ToolOutcome::success(
                serde_json::to_string(&ctx.grants).unwrap_or_else(|_| "[]".to_owned()),
            )),
            (
                Self::ListDirectory,
                Parsed::ListDirectory {
                    relative_path,
                    max_entries,
                    ..
                },
            ) => list_directory(ctx, relative_path, *max_entries),
            (
                Self::ReadTextFile,
                Parsed::ReadTextFile {
                    relative_path,
                    max_bytes,
                    offset,
                    ..
                },
            ) => read_text_file(ctx, relative_path, *max_bytes, *offset),
            (
                Self::SearchFiles,
                Parsed::SearchFiles {
                    relative_path,
                    query,
                    max_results,
                    ..
                },
            ) => search_files(ctx, relative_path, query, *max_results),
            (
                Self::WriteFile,
                Parsed::WriteFile {
                    relative_path,
                    content,
                    create_only,
                    ..
                },
            ) => write_text_file(ctx, relative_path, content, *create_only),
            (Self::MoveFile, Parsed::MoveFile { from_path, to_path, .. }) => {
                move_within_grant(ctx, from_path, to_path)
            }
            (Self::DeleteFile, Parsed::DeleteFile { relative_path, .. }) => {
                delete_to_trash(ctx, relative_path)
            }
            (Self::ListInstalledApps, _) => Ok(ToolOutcome::success(
                serde_json::to_string(&ctx.installed_apps).unwrap_or_else(|_| "[]".to_owned()),
            )),
            (Self::LaunchApplication, _) | (Self::FocusApplication, _) => open_application(ctx, self),
            (Self::RevealFile, Parsed::RevealFile { relative_path, .. }) => {
                reveal_file(ctx, relative_path)
            }
            (Self::OpenFileWithApplication, Parsed::OpenFileWithApplication { relative_path, .. }) => {
                open_file_with_application(ctx, relative_path)
            }
            (Self::BrowserOpen, Parsed::BrowserOpen { url, .. }) => {
                browser_request(ctx, serde_json::json!({ "type": "open", "url": url }))
            }
            (Self::BrowserSnapshot, _) => browser_request(ctx, serde_json::json!({ "type": "snapshot" })),
            (Self::BrowserClick, Parsed::BrowserClick { ref_id }) => {
                browser_request(ctx, serde_json::json!({ "type": "click", "ref": ref_id }))
            }
            (Self::BrowserType, Parsed::BrowserType { ref_id, text }) => {
                browser_request(ctx, serde_json::json!({ "type": "type", "ref": ref_id, "text": text }))
            }
            (Self::BrowserSelect, Parsed::BrowserSelect { ref_id, value }) => {
                browser_request(ctx, serde_json::json!({ "type": "select", "ref": ref_id, "value": value }))
            }
            (Self::BrowserWait, _) => browser_request(ctx, serde_json::json!({ "type": "wait" })),
            (Self::BrowserExtractText, _) => browser_request(ctx, serde_json::json!({ "type": "extract_text" })),
            (Self::BrowserClose, _) => browser_request(ctx, serde_json::json!({ "type": "close" })),
            (Self::RememberPreference, Parsed::RememberPreference { title, content }) => {
                let store = ctx.app.state::<crate::history::HistoryStore>();
                store.remember_preference(title, content).map_err(|error| error)?;
                Ok(ToolOutcome::success(format!("已记住：{title}")))
            }
            (Self::ForgetPreference, Parsed::ForgetPreference { title }) => {
                let store = ctx.app.state::<crate::history::HistoryStore>();
                store.forget_preference(title).map_err(|error| error)?;
                Ok(ToolOutcome::success(format!("已移除记忆：{title}")))
            }
            (Self::LaunchWutheringWaves, _) => {
                let entry = super::apps::find_wuthering_waves()
                    .ok_or("未找到鸣潮客户端；请确认已安装")?;
                spawn_open(&entry.path)?;
                Ok(ToolOutcome::success(format!("已启动：{}", entry.name)))
            }
            (Self::ReadSystemFile, Parsed::ReadSystemFile { path }) => {
                let resolved = resolve_system_path(ctx.app, path)?;
                ensure_system_path_allowed(ctx.app, &resolved)?;
                let metadata = std::fs::metadata(&resolved).map_err(|_| "文件不存在或不可访问")?;
                if !metadata.is_file() {
                    return Err("目标不是文件".to_owned());
                }
                if metadata.len() > MAX_SYSTEM_READ_BYTES {
                    return Err(format!("文件超过 {} KB 上限", MAX_SYSTEM_READ_BYTES / 1024));
                }
                let content =
                    std::fs::read_to_string(&resolved).map_err(|_| "文件不是可读取的 UTF-8 文本")?;
                Ok(ToolOutcome::success(format!(
                    "文件：{}\n内容（不可信数据）：\n{content}",
                    resolved.display()
                )))
            }
            (Self::WriteSystemFile, Parsed::WriteSystemFile { path, content }) => {
                let resolved = resolve_system_path(ctx.app, path)?;
                ensure_system_path_allowed(ctx.app, &resolved)?;
                let file_name = resolved
                    .file_name()
                    .ok_or("目标文件名无效")?
                    .to_string_lossy()
                    .to_string();
                let temp = resolved.with_file_name(format!(".{file_name}.phoebe-{}.tmp", std::process::id()));
                std::fs::write(&temp, content.as_bytes()).map_err(|_| "无法写入临时文件")?;
                if std::fs::rename(&temp, &resolved).is_err() {
                    let _ = std::fs::remove_file(&temp);
                    return Err("无法保存文件".to_owned());
                }
                Ok(ToolOutcome::success(format!("已写入 {}", resolved.display())))
            }
            _ => Err("工具参数不匹配".to_owned()),
        }
    }
}

fn read_only_describe(target: &str) -> Result<Describe, String> {
    Ok(Describe {
        target: target.to_owned(),
        impact: "只读，不产生副作用".to_owned(),
        preview: None,
    })
}

fn validate_https_url(value: &str) -> Result<(String, String), String> {
    if value.len() > 2048 {
        return Err("链接过长".to_owned());
    }
    let url = Url::parse(value).map_err(|_| "链接格式无效".to_owned())?;
    if url.scheme() != "https" {
        return Err("只允许打开 HTTPS 链接".to_owned());
    }
    if !url.username().is_empty() || url.password().is_some() {
        return Err("链接不能包含用户名或密码".to_owned());
    }
    let host = url.host_str().ok_or("链接缺少主机名")?.to_ascii_lowercase();
    if host.is_empty() {
        return Err("链接缺少主机名".to_owned());
    }
    Ok((url.to_string(), host))
}

fn validate_grant_id(value: &str) -> Result<String, String> {
    if value.len() != 32 || !value.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err("目录授权标识无效".to_owned());
    }
    Ok(value.to_ascii_lowercase())
}

fn validate_app_id(value: &str) -> Result<String, String> {
    if value.is_empty()
        || value.len() > 200
        || !value
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
    {
        return Err("应用标识无效".to_owned());
    }
    Ok(value.to_owned())
}

fn validate_browser_ref(value: &str) -> Result<String, String> {
    if value.len() > 16 || !value.chars().all(|c| c.is_ascii_alphanumeric()) {
        return Err("页面元素引用无效；请重新 snapshot 获取最新 ref".to_owned());
    }
    Ok(value.to_owned())
}

fn validate_system_path(value: &str) -> Result<String, String> {
    let trimmed = value.trim();
    if trimmed.is_empty() || trimmed.len() > 4096 || trimmed.chars().any(|c| c == '\0') {
        return Err("路径无效".into());
    }
    if !Path::new(trimmed).is_absolute() {
        return Err("系统级读写需要绝对路径".into());
    }
    Ok(trimmed.to_owned())
}

/// Resolves an absolute path, following symlinks when it exists and
/// canonicalizing the parent when it does not (for new files).
fn resolve_system_path(_app: &tauri::AppHandle, raw: &str) -> Result<PathBuf, String> {
    let path = Path::new(raw);
    if !path.is_absolute() {
        return Err("系统级读写需要绝对路径".into());
    }
    if path.exists() {
        return std::fs::canonicalize(path).map_err(|_| "无法解析该路径".to_owned());
    }
    let parent = path.parent().ok_or("路径无效")?;
    let canonical_parent = std::fs::canonicalize(parent).map_err(|_| "父目录不存在".to_owned())?;
    Ok(match path.file_name() {
        Some(name) => canonical_parent.join(name),
        None => canonical_parent,
    })
}

fn ensure_system_path_allowed(app: &tauri::AppHandle, path: &Path) -> Result<(), String> {
    if PathPolicy::from_app(app).is_denied(path) {
        return Err("该路径属于受保护的敏感位置，已拒绝".to_owned());
    }
    Ok(())
}

/// Forwards one command to the browser sidecar and turns the answer into a
/// tool outcome. Structured details (like the DOM snapshot) are appended as
/// JSON so the model can see element refs for click/type/select.
fn browser_request(ctx: &ToolContext, command: serde_json::Value) -> Result<ToolOutcome, String> {
    let result = ctx
        .app
        .state::<crate::browser::BrowserService>()
        .request(ctx.app, command)?;
    if !result.ok {
        return Ok(ToolOutcome::failed(result.text));
    }
    let text = if result.details.is_null() {
        result.text
    } else {
        match serde_json::to_string(&result.details) {
            Ok(details) => format!("{}\n{}", result.text, details),
            Err(_) => result.text,
        }
    };
    Ok(ToolOutcome::success(text))
}

fn validate_relative(value: &str) -> Result<String, String> {
    // Reuse the confinement helper so argument validation and path resolution
    // agree on what a relative path is.
    super::paths::safe_relative(value).map_err(|error| error.to_string())?;
    Ok(value.to_owned())
}

fn validate_non_empty_relative(value: &str) -> Result<String, String> {
    let validated = validate_relative(value)?;
    if validated.trim().is_empty() {
        return Err("必须提供相对路径".to_owned());
    }
    Ok(validated)
}

fn display_relative(value: &str) -> String {
    if value.trim().is_empty() {
        "（授权目录根）".to_owned()
    } else {
        value.to_owned()
    }
}

fn resolve_read(ctx: &ToolContext, relative_path: &str) -> Result<PathBuf, String> {
    let folder = ctx
        .folder
        .as_ref()
        .ok_or("该目录尚未授权".to_owned())?;
    let policy = PathPolicy::from_app(ctx.app);
    resolve_read_path(Path::new(&folder.path), relative_path, &policy)
}

fn list_directory(ctx: &ToolContext, relative_path: &str, max_entries: u32) -> Result<ToolOutcome, String> {
    let folder = ctx.folder.as_ref().ok_or("该目录尚未授权")?;
    let path = resolve_read(ctx, relative_path)?;
    let metadata = std::fs::metadata(&path).map_err(|_| "目录不存在或不可访问".to_owned())?;
    if !metadata.is_dir() {
        return Err("目标不是目录".to_owned());
    }
    let mut entries = Vec::new();
    let reader = std::fs::read_dir(&path).map_err(|_| "无法读取目录".to_owned())?;
    for entry in reader.flatten() {
        if entries.len() >= max_entries as usize {
            break;
        }
        let name = entry.file_name().to_string_lossy().to_string();
        if name.starts_with('.') {
            continue;
        }
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        // Never follow symlinks while listing or searching: a link inside the
        // grant could otherwise point outside it.
        if file_type.is_symlink() {
            continue;
        }
        let Ok(metadata) = entry.metadata() else {
            continue;
        };
        let kind = if metadata.is_dir() {
            "dir"
        } else if metadata.is_file() {
            "file"
        } else {
            "other"
        };
        entries.push(serde_json::json!({
            "name": name,
            "kind": kind,
            "bytes": if metadata.is_file() { metadata.len() } else { 0 },
        }));
    }
    entries.sort_by(|left, right| left["name"].as_str().cmp(&right["name"].as_str()));
    let truncated = entries.len() >= max_entries as usize;
    Ok(ToolOutcome::success(
        serde_json::json!({
            "folder": folder.label,
            "relativePath": relative_path,
            "entries": entries,
            "truncated": truncated,
        })
        .to_string(),
    ))
}

fn read_text_file(
    ctx: &ToolContext,
    relative_path: &str,
    max_bytes: u64,
    offset: u64,
) -> Result<ToolOutcome, String> {
    let folder = ctx.folder.as_ref().ok_or("该目录尚未授权")?;
    let path = resolve_read(ctx, relative_path)?;
    let metadata = std::fs::metadata(&path).map_err(|_| "文件不存在或不可访问".to_owned())?;
    if !metadata.is_file() {
        return Err("目标不是文件".to_owned());
    }
    if metadata.len() > MAX_FILE_BYTES {
        return Err(format!(
            "文件为 {} KB，超过 {} KB 的整文件上限；请用 search_files 查找具体内容",
            metadata.len() / 1024,
            MAX_FILE_BYTES / 1024
        ));
    }
    let bytes = std::fs::read(&path).map_err(|_| "无法读取文件".to_owned())?;
    let total = bytes.len() as u64;
    if offset > total {
        return Err("offset 超出文件大小".to_owned());
    }
    let start = offset as usize;
    let end = (start + max_bytes as usize).min(bytes.len());
    let text = String::from_utf8_lossy(&bytes[start..end]);
    let truncated = (end as u64) < total;
    Ok(ToolOutcome::success(format!(
        "目录：{}\n相对路径：{}\n字节范围：{}..{} / 共 {} 字节{}\n内容（不可信数据）：\n{}",
        folder.label,
        display_relative(relative_path),
        start,
        end,
        total,
        if truncated { "（已截断，可用 offset 继续读取）" } else { "" },
        text
    )))
}

fn search_files(
    ctx: &ToolContext,
    relative_path: &str,
    query: &str,
    max_results: u32,
) -> Result<ToolOutcome, String> {
    let folder = ctx.folder.as_ref().ok_or("该目录尚未授权")?;
    let root = resolve_read(ctx, relative_path)?;
    let metadata = std::fs::metadata(&root).map_err(|_| "目录不存在或不可访问".to_owned())?;
    if !metadata.is_dir() {
        return Err("搜索起点不是目录".to_owned());
    }
    let needle = query.to_lowercase();
    let mut hits = Vec::new();
    let mut scanned = 0usize;
    walk_search(
        &root,
        &root,
        &needle,
        0,
        max_results as usize,
        &mut scanned,
        &mut hits,
    );
    Ok(ToolOutcome::success(
        serde_json::json!({
            "folder": folder.label,
            "relativePath": relative_path,
            "query": query,
            "matches": hits,
            "truncated": hits.len() >= max_results as usize || scanned >= SEARCH_SCAN_LIMIT,
        })
        .to_string(),
    ))
}

fn walk_search(
    root: &Path,
    dir: &Path,
    needle: &str,
    depth: usize,
    max_results: usize,
    scanned: &mut usize,
    hits: &mut Vec<serde_json::Value>,
) {
    if depth > SEARCH_MAX_DEPTH || hits.len() >= max_results || *scanned >= SEARCH_SCAN_LIMIT {
        return;
    }
    let Ok(reader) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in reader.flatten() {
        if hits.len() >= max_results || *scanned >= SEARCH_SCAN_LIMIT {
            return;
        }
        let name = entry.file_name().to_string_lossy().to_string();
        if name.starts_with('.') {
            continue;
        }
        // Use the non-following file type so a symlink cannot pull content from
        // outside the granted folder into the search results.
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        if file_type.is_symlink() {
            continue;
        }
        *scanned += 1;
        if file_type.is_dir() {
            walk_search(root, &entry.path(), needle, depth + 1, max_results, scanned, hits);
            continue;
        }
        if !file_type.is_file() {
            continue;
        }
        let Ok(metadata) = entry.metadata() else {
            continue;
        };
        let path = entry.path();
        let relative = path
            .strip_prefix(root)
            .unwrap_or(&path)
            .to_string_lossy()
            .to_string();
        let name_match = name.to_lowercase().contains(needle);
        let mut content_match = false;
        if is_text_name(&name) && metadata.len() <= SEARCH_CONTENT_BYTES {
            if let Ok(text) = std::fs::read_to_string(&path) {
                content_match = text.to_lowercase().contains(needle);
            }
        }
        if name_match || content_match {
            hits.push(serde_json::json!({
                "relativePath": relative,
                "bytes": metadata.len(),
                "nameMatch": name_match,
                "contentMatch": content_match,
            }));
        }
    }
}

fn resolve_write(ctx: &ToolContext, relative_path: &str) -> Result<PathBuf, String> {
    let folder = ctx.folder.as_ref().ok_or("该目录尚未授权")?;
    let policy = PathPolicy::from_app(ctx.app);
    resolve_write_path(Path::new(&folder.path), relative_path, &policy)
}

fn grant_root(ctx: &ToolContext) -> Result<PathBuf, String> {
    let folder = ctx.folder.as_ref().ok_or("该目录尚未授权")?;
    std::fs::canonicalize(&folder.path).map_err(|_| "授权目录当前不可用".to_owned())
}

fn read_optional_text(path: &Path) -> Option<String> {
    let metadata = std::fs::metadata(path).ok()?;
    if !metadata.is_file() || metadata.len() > MAX_WRITE_BYTES {
        return None;
    }
    std::fs::read_to_string(path).ok()
}

/// Focused line diff for the approval preview: trims the common prefix/suffix
/// and shows removed/added lines, bounded in both line count and characters.
fn bounded_diff(old: Option<&str>, new: &str) -> String {
    const MAX_LINES: usize = 40;
    const MAX_CHARS: usize = 2400;
    if let Some(old_text) = old {
        if old_text == new {
            return "  内容无变化\n".to_owned();
        }
    }
    let old_lines: Vec<&str> = old.unwrap_or("").lines().collect();
    let new_lines: Vec<&str> = new.lines().collect();
    let mut start = 0;
    while start < old_lines.len() && start < new_lines.len() && old_lines[start] == new_lines[start] {
        start += 1;
    }
    let mut old_end = old_lines.len();
    let mut new_end = new_lines.len();
    while old_end > start && new_end > start && old_lines[old_end - 1] == new_lines[new_end - 1] {
        old_end -= 1;
        new_end -= 1;
    }
    let mut body = String::new();
    let mut lines = 0usize;
    if start > 0 {
        body.push_str(&format!("  … 前 {start} 行未变化\n"));
    }
    for line in &old_lines[start..old_end] {
        if lines >= MAX_LINES || body.len() >= MAX_CHARS {
            body.push_str("  … 已截断\n");
            return body;
        }
        body.push_str(&format!("- {line}\n"));
        lines += 1;
    }
    for line in &new_lines[start..new_end] {
        if lines >= MAX_LINES || body.len() >= MAX_CHARS {
            body.push_str("  … 已截断\n");
            return body;
        }
        body.push_str(&format!("+ {line}\n"));
        lines += 1;
    }
    if body.is_empty() {
        body.push_str("  内容无变化\n");
    }
    body
}

fn write_text_file(
    ctx: &ToolContext,
    relative_path: &str,
    content: &str,
    create_only: bool,
) -> Result<ToolOutcome, String> {
    let folder = ctx.folder.as_ref().ok_or("该目录尚未授权")?;
    let target = resolve_write(ctx, relative_path)?;
    let exists = target.exists();
    if create_only && exists {
        return Err("目标文件已存在，createOnly 已阻止覆盖".to_owned());
    }
    let file_name = target
        .file_name()
        .ok_or("目标文件名无效")?
        .to_string_lossy()
        .to_string();
    let temp = target.with_file_name(format!(".{file_name}.phoebe-{}.tmp", std::process::id()));
    std::fs::write(&temp, content.as_bytes()).map_err(|_| "无法写入临时文件".to_owned())?;
    if std::fs::rename(&temp, &target).is_err() {
        let _ = std::fs::remove_file(&temp);
        return Err("无法保存文件".to_owned());
    }
    Ok(ToolOutcome::success(format!(
        "已{}文件：{}/{}（{} 字节）",
        if exists { "覆盖" } else { "创建" },
        folder.label,
        display_relative(relative_path),
        content.len()
    )))
}

fn move_within_grant(ctx: &ToolContext, from: &str, to: &str) -> Result<ToolOutcome, String> {
    let folder = ctx.folder.as_ref().ok_or("该目录尚未授权")?;
    let root = grant_root(ctx)?;
    let source = resolve_read(ctx, from)?;
    if source == root {
        return Err("不能移动授权目录本身".to_owned());
    }
    let destination = resolve_write(ctx, to)?;
    if destination.exists() {
        return Err("目标已存在，拒绝覆盖".to_owned());
    }
    std::fs::rename(&source, &destination).map_err(|_| "无法移动文件".to_owned())?;
    Ok(ToolOutcome::success(format!(
        "已移动：{}/{} → {}",
        folder.label,
        display_relative(from),
        display_relative(to)
    )))
}

fn delete_to_trash(ctx: &ToolContext, relative_path: &str) -> Result<ToolOutcome, String> {
    let folder = ctx.folder.as_ref().ok_or("该目录尚未授权")?;
    let root = grant_root(ctx)?;
    let source = resolve_read(ctx, relative_path)?;
    if source == root {
        return Err("不能删除授权目录本身".to_owned());
    }
    let trash = trash_dir(ctx.app);
    std::fs::create_dir_all(&trash).map_err(|_| "无法建立废纸篓目录".to_owned())?;
    let name = source.file_name().ok_or("文件名无效")?.to_owned();
    let target = unique_destination(&trash, &name);
    std::fs::rename(&source, &target).map_err(|error| match error.kind() {
        std::io::ErrorKind::CrossesDevices => {
            "目标位于不同磁盘卷，暂不支持跨卷移入废纸篓".to_owned()
        }
        _ => "无法移入废纸篓".to_owned(),
    })?;
    Ok(ToolOutcome::success(format!(
        "已将 {}/{} 移入废纸篓，可在系统废纸篓恢复",
        folder.label,
        display_relative(relative_path)
    )))
}

/// macOS uses the real per-user Trash; other platforms fall back to a
/// recoverable quarantine inside the application data directory.
fn trash_dir(app: &tauri::AppHandle) -> PathBuf {
    #[cfg(target_os = "macos")]
    {
        if let Ok(home) = app.path().home_dir() {
            return home.join(".Trash");
        }
    }
    app.path()
        .app_data_dir()
        .map(|dir| dir.join("file-trash"))
        .unwrap_or_else(|_| std::env::temp_dir().join("phoebe-file-trash"))
}

fn unique_destination(dir: &Path, name: &std::ffi::OsStr) -> PathBuf {
    let candidate = dir.join(name);
    if !candidate.exists() {
        return candidate;
    }
    let path = Path::new(name);
    let stem = path.file_stem().and_then(|value| value.to_str()).unwrap_or("file");
    let extension = path.extension().and_then(|value| value.to_str());
    for index in 1..10_000 {
        let file_name = match extension {
            Some(extension) => format!("{stem} {index}.{extension}"),
            None => format!("{stem} {index}"),
        };
        let candidate = dir.join(file_name);
        if !candidate.exists() {
            return candidate;
        }
    }
    dir.join(format!("{stem}-{}", chrono::Utc::now().timestamp_millis()))
}

fn is_text_name(name: &str) -> bool {
    const TEXT_EXTENSIONS: [&str; 22] = [
        "txt", "md", "markdown", "csv", "tsv", "json", "jsonl", "log", "yaml", "yml", "toml",
        "ini", "conf", "rs", "ts", "tsx", "js", "mjs", "py", "html", "css", "sql",
    ];
    Path::new(name)
        .extension()
        .and_then(|extension| extension.to_str())
        .map(|extension| TEXT_EXTENSIONS.contains(&extension.to_ascii_lowercase().as_str()))
        .unwrap_or(false)
}

fn open_application(ctx: &ToolContext, tool: &Tool) -> Result<ToolOutcome, String> {
    let entry = ctx.selected_app.as_ref().ok_or("未找到该应用")?;
    spawn_open(&entry.path)?;
    Ok(ToolOutcome::success(format!(
        "已{}应用：{}",
        if matches!(tool, Tool::FocusApplication) { "激活" } else { "启动" },
        entry.name
    )))
}

fn reveal_file(ctx: &ToolContext, relative_path: &str) -> Result<ToolOutcome, String> {
    let folder = ctx.folder.as_ref().ok_or("该目录尚未授权")?;
    let path = resolve_read(ctx, relative_path)?;
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("/usr/bin/open")
            .arg("-R")
            .arg(&path)
            .spawn()
            .map_err(|_| "无法在文件管理器中显示".to_owned())?;
    }
    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("explorer")
            .arg(format!("/select,{}", path.display()))
            .spawn()
            .map_err(|_| "无法在文件管理器中显示".to_owned())?;
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        let parent = path.parent().unwrap_or(&path);
        std::process::Command::new("xdg-open")
            .arg(parent)
            .spawn()
            .map_err(|_| "无法在文件管理器中显示".to_owned())?;
    }
    Ok(ToolOutcome::success(format!(
        "已在文件管理器中显示：{}/{}",
        folder.label,
        display_relative(relative_path)
    )))
}

fn open_file_with_application(ctx: &ToolContext, relative_path: &str) -> Result<ToolOutcome, String> {
    let folder = ctx.folder.as_ref().ok_or("该目录尚未授权")?;
    let entry = ctx.selected_app.as_ref().ok_or("未找到该应用")?;
    let path = resolve_read(ctx, relative_path)?;
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("/usr/bin/open")
            .arg("-a")
            .arg(&entry.path)
            .arg(&path)
            .spawn()
            .map_err(|_| "无法用该应用打开文件".to_owned())?;
    }
    #[cfg(target_os = "windows")]
    {
        std::process::Command::new(&entry.path)
            .arg(&path)
            .spawn()
            .map_err(|_| "无法用该应用打开文件".to_owned())?;
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        std::process::Command::new("xdg-open")
            .arg(&path)
            .spawn()
            .map_err(|_| "无法用该应用打开文件".to_owned())?;
    }
    Ok(ToolOutcome::success(format!(
        "已用 {} 打开：{}/{}",
        entry.name,
        folder.label,
        display_relative(relative_path)
    )))
}

/// Opens a path with the platform's default handler using a fixed program and
/// an argument array; no shell is involved.
#[cfg(target_os = "macos")]
fn spawn_open(path: &str) -> Result<(), String> {
    std::process::Command::new("/usr/bin/open")
        .arg(path)
        .spawn()
        .map(|_| ())
        .map_err(|_| "无法打开该系统目标".to_owned())
}

#[cfg(target_os = "windows")]
fn spawn_open(path: &str) -> Result<(), String> {
    std::process::Command::new("cmd")
        .args(["/C", "start", "", path])
        .spawn()
        .map(|_| ())
        .map_err(|_| "无法打开该系统目标".to_owned())
}

#[cfg(all(unix, not(target_os = "macos")))]
fn spawn_open(path: &str) -> Result<(), String> {
    std::process::Command::new("xdg-open")
        .arg(path)
        .spawn()
        .map(|_| ())
        .map_err(|_| "无法打开该系统目标".to_owned())
}

/// Launches the default handler for a validated HTTPS URL. No shell is used,
/// and the URL is passed as a single argument.
fn open_https(url: &str) -> Result<ToolOutcome, String> {
    #[cfg(target_os = "macos")]
    let launched = std::process::Command::new("/usr/bin/open").arg(url).spawn();
    #[cfg(target_os = "windows")]
    let launched = std::process::Command::new("rundll32")
        .args(["url.dll,FileProtocolHandler", url])
        .spawn();
    #[cfg(all(unix, not(target_os = "macos")))]
    let launched = std::process::Command::new("xdg-open").arg(url).spawn();

    launched
        .map(|_| ToolOutcome::success(format!("已在系统浏览器中打开 {url}")))
        .map_err(|_| "无法调用系统浏览器".to_owned())
}

#[cfg(test)]
mod tests {
    use super::{Parsed, Tool};
    use serde_json::json;

    #[test]
    fn unknown_tools_are_rejected() {
        assert!(Tool::from_name("bash").is_none());
        assert!(Tool::from_name("open_url").is_some());
        assert!(Tool::from_name("read_text_file").is_some());
    }

    #[test]
    fn device_location_is_read_only_and_auto() {
        let tool = Tool::from_name("get_device_location").unwrap();
        assert_eq!(tool.name(), "get_device_location");
        assert!(tool.is_auto());
        assert!(matches!(tool.parse(&json!({})), Ok(Parsed::Empty)));
        assert!(tool.parse(&json!({ "precise": true })).is_err());
        assert!(tool.required_grant(&Parsed::Empty).is_none());
        assert!(tool.bound_key(&Parsed::Empty).is_none());
    }

    #[test]
    fn browser_tools_parse_and_scope_correctly() {
        let open = Tool::from_name("browser_open").unwrap();
        assert!(!open.is_auto());
        assert!(Tool::BrowserOpen
            .parse(&json!({ "url": "http://example.com" }))
            .is_err());
        let parsed = Tool::BrowserOpen.parse(&json!({ "url": "https://example.com/x" })).unwrap();
        assert_eq!(
            Tool::BrowserOpen.bound_key(&parsed).as_deref(),
            Some("domain:example.com")
        );

        assert!(Tool::from_name("browser_snapshot").unwrap().is_auto());
        assert!(Tool::BrowserSnapshot.parse(&json!({})).is_ok());
        assert!(Tool::BrowserSnapshot.parse(&json!({ "url": "x" })).is_err());

        assert!(Tool::BrowserClick
            .parse(&json!({ "ref": "e0" }))
            .is_ok());
        assert!(Tool::BrowserClick
            .parse(&json!({ "ref": "../evil" }))
            .is_err());
        assert!(Tool::BrowserType
            .parse(&json!({ "ref": "e1", "text": "hi" }))
            .is_ok());
        assert!(Tool::BrowserType
            .parse(&json!({ "ref": "e1", "text": "x".repeat(2001) }))
            .is_err());
        assert!(Tool::BrowserSelect
            .parse(&json!({ "ref": "e2", "value": "opt" }))
            .is_ok());
    }

    #[test]
    fn preference_tools_validate_and_confirm() {
        let remember = Tool::from_name("remember_preference").unwrap();
        assert!(remember.always_confirms());
        assert!(!remember.is_auto());
        assert!(Tool::RememberPreference
            .parse(&json!({ "title": "语言", "content": "默认使用简体中文" }))
            .is_ok());
        assert!(Tool::RememberPreference
            .parse(&json!({ "title": "密码", "content": "123456" }))
            .is_err());
        assert!(Tool::RememberPreference
            .parse(&json!({ "title": "偏好", "content": "my access token is abc" }))
            .is_err());
        assert!(Tool::RememberPreference
            .parse(&json!({ "title": "x", "content": "y".repeat(601) }))
            .is_err());
        assert!(Tool::ForgetPreference.parse(&json!({ "title": "语言" })).is_ok());
        assert!(Tool::ForgetPreference.parse(&json!({ "title": "  " })).is_err());
    }

    #[test]
    fn system_file_tools_require_absolute_paths_and_confirm() {
        let read = Tool::from_name("read_system_file").unwrap();
        assert!(read.always_confirms());
        assert!(!read.is_auto());
        assert!(Tool::ReadSystemFile
            .parse(&json!({ "path": "relative/x.txt" }))
            .is_err());
        assert!(Tool::ReadSystemFile
            .parse(&json!({ "path": "/tmp/a.txt" }))
            .is_ok());
        assert!(Tool::WriteSystemFile
            .parse(&json!({ "path": "/tmp/a.txt", "content": "hi" }))
            .is_ok());
        assert!(Tool::WriteSystemFile
            .parse(&json!({ "path": "/tmp/a.txt", "content": "x".repeat(262_145) }))
            .is_err());
    }

    #[test]
    fn unknown_fields_are_rejected() {
        assert!(Tool::OpenUrl
            .parse(&json!({ "url": "https://example.com", "path": "/etc/passwd" }))
            .is_err());
        assert!(Tool::GetCurrentTime
            .parse(&json!({ "command": "pwd" }))
            .is_err());
        assert!(Tool::GetCurrentTime.parse(&json!({})).is_ok());
    }

    #[test]
    fn open_url_only_accepts_https_without_credentials() {
        assert!(matches!(
            Tool::OpenUrl.parse(&json!({ "url": "https://example.com/a" })),
            Ok(Parsed::OpenUrl { host, .. }) if host == "example.com"
        ));
        assert!(Tool::OpenUrl
            .parse(&json!({ "url": "http://example.com" }))
            .is_err());
        assert!(Tool::OpenUrl
            .parse(&json!({ "url": "https://user:secret@example.com" }))
            .is_err());
        assert!(Tool::OpenUrl
            .parse(&json!({ "url": "file:///tmp/a" }))
            .is_err());
        assert!(Tool::OpenUrl
            .parse(&json!({ "url": "https://" }))
            .is_err());
    }

    #[test]
    fn trust_key_is_scoped_to_the_host() {
        let parsed = Tool::OpenUrl
            .parse(&json!({ "url": "https://Example.com/x" }))
            .unwrap();
        assert_eq!(
            Tool::OpenUrl.bound_key(&parsed).as_deref(),
            Some("domain:example.com")
        );
        assert!(Tool::GetCurrentTime.bound_key(&Parsed::Empty).is_none());
    }

    #[test]
    fn file_tools_require_a_grant_and_relative_path() {
        assert!(Tool::ReadTextFile
            .parse(&json!({ "grantId": "not-a-grant", "relativePath": "a.txt" }))
            .is_err());
        assert!(Tool::ReadTextFile
            .parse(&json!({ "grantId": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa", "relativePath": "../x" }))
            .is_err());
        assert!(Tool::ReadTextFile
            .parse(&json!({ "grantId": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa", "relativePath": "/etc/passwd" }))
            .is_err());
        assert!(Tool::ReadTextFile
            .parse(&json!({ "grantId": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa", "relativePath": "a.txt", "path": "/etc/passwd" }))
            .is_err());
        let parsed = Tool::ReadTextFile
            .parse(&json!({ "grantId": "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA", "relativePath": "a.txt" }))
            .unwrap();
        assert!(matches!(parsed, Parsed::ReadTextFile { ref grant_id, .. } if grant_id == "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"));
        assert_eq!(
            Tool::ReadTextFile.bound_key(&parsed).as_deref(),
            Some("folder:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa")
        );
    }

    #[test]
    fn search_bounds_its_query_and_limits() {
        let grant = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
        assert!(Tool::SearchFiles
            .parse(&json!({ "grantId": grant, "relativePath": "", "query": "  " }))
            .is_err());
        assert!(Tool::SearchFiles
            .parse(&json!({ "grantId": grant, "relativePath": "", "query": "x".repeat(65) }))
            .is_err());
        let parsed = Tool::SearchFiles
            .parse(&json!({ "grantId": grant, "relativePath": "", "query": "note", "maxResults": 9999 }))
            .unwrap();
        assert!(matches!(parsed, Parsed::SearchFiles { max_results: 50, .. }));
    }

    #[test]
    fn app_tools_validate_ids_and_scope() {
        let app_id = "app-0123456789abcdef";
        assert!(Tool::LaunchApplication.parse(&json!({ "appId": app_id })).is_ok());
        assert!(Tool::LaunchApplication.parse(&json!({ "appId": "../evil" })).is_err());
        assert!(Tool::LaunchApplication
            .parse(&json!({ "appId": app_id, "path": "/Applications/X.app" }))
            .is_err());
        let parsed = Tool::LaunchApplication.parse(&json!({ "appId": app_id })).unwrap();
        assert_eq!(Tool::LaunchApplication.required_app(&parsed), Some(app_id));
        assert_eq!(
            Tool::LaunchApplication.bound_key(&parsed).as_deref(),
            Some("app:app-0123456789abcdef")
        );
        assert!(Tool::ListInstalledApps.is_auto());
        assert!(Tool::ListInstalledApps.is_app_listing());
        assert!(!Tool::LaunchApplication.is_app_listing());

        let reveal = Tool::RevealFile
            .parse(&json!({ "grantId": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa", "relativePath": "a.txt" }))
            .unwrap();
        assert!(!Tool::RevealFile.required_grant(&reveal).unwrap().write);
        assert!(Tool::RevealFile.required_app(&reveal).is_none());
    }

    #[test]
    fn read_offset_defaults_and_is_bounded() {
        let grant = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
        let parsed = Tool::ReadTextFile
            .parse(&json!({ "grantId": grant, "relativePath": "a.txt", "offset": 100 }))
            .unwrap();
        assert!(matches!(parsed, Parsed::ReadTextFile { offset: 100, .. }));
        let defaulted = Tool::ReadTextFile
            .parse(&json!({ "grantId": grant, "relativePath": "a.txt" }))
            .unwrap();
        assert!(matches!(defaulted, Parsed::ReadTextFile { offset: 0, max_bytes: 16_384, .. }));
    }

    #[test]
    fn write_tools_require_write_grant_and_bounded_content() {
        let grant = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
        let parsed = Tool::WriteFile
            .parse(&json!({ "grantId": grant, "relativePath": "a.txt", "content": "hi" }))
            .unwrap();
        let requirement = Tool::WriteFile.required_grant(&parsed).unwrap();
        assert!(requirement.write);
        assert!(Tool::WriteFile.always_confirms());
        assert!(!Tool::ReadTextFile.always_confirms());

        assert!(Tool::WriteFile
            .parse(&json!({ "grantId": grant, "relativePath": "", "content": "hi" }))
            .is_err());
        assert!(Tool::WriteFile
            .parse(&json!({ "grantId": grant, "relativePath": "a.txt", "content": "x".repeat(262_145) }))
            .is_err());
        assert!(Tool::WriteFile
            .parse(&json!({ "grantId": grant, "relativePath": "a.txt", "content": "hi", "mode": "append" }))
            .is_err());
    }

    #[test]
    fn move_and_delete_reject_empty_or_escaping_paths() {
        let grant = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
        assert!(Tool::MoveFile
            .parse(&json!({ "grantId": grant, "fromRelativePath": "a.txt", "toRelativePath": "../b.txt" }))
            .is_err());
        assert!(Tool::DeleteFile
            .parse(&json!({ "grantId": grant, "relativePath": "" }))
            .is_err());
        assert!(Tool::DeleteFile
            .parse(&json!({ "grantId": grant, "relativePath": "a.txt" }))
            .is_ok());
    }

    #[test]
    fn bounded_diff_marks_added_and_removed_lines() {
        use super::bounded_diff;
        let diff = bounded_diff(None, "one\ntwo");
        assert!(diff.contains("+ one"));
        assert!(diff.contains("+ two"));
        let unchanged = bounded_diff(Some("same"), "same");
        assert!(unchanged.contains("内容无变化"));
        let changed = bounded_diff(Some("a\nb"), "a\nc");
        assert!(changed.contains("- b"));
        assert!(changed.contains("+ c"));
        assert!(changed.contains("前 1 行未变化"));
    }
}
