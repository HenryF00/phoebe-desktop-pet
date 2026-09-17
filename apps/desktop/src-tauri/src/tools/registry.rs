//! Static catalog of operating-system tools the Agent may request.
//!
//! Every tool parses its own arguments with `deny_unknown_fields`, so a model
//! cannot smuggle extra parameters past the gateway. Execution is deliberately
//! performed here in Rust, never inside the Node sidecar.

use serde::Deserialize;
use url::Url;

use super::ToolOutcome;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct EmptyArgs {}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct OpenUrlArgs {
    url: String,
}

#[derive(Debug)]
pub enum Parsed {
    Empty,
    OpenUrl { url: String, host: String },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tool {
    GetCurrentTime,
    GetSystemStatus,
    OpenUrl,
}

impl Tool {
    pub fn from_name(name: &str) -> Option<Self> {
        match name {
            "get_current_time" => Some(Self::GetCurrentTime),
            "get_system_status" => Some(Self::GetSystemStatus),
            "open_url" => Some(Self::OpenUrl),
            _ => None,
        }
    }

    pub fn name(&self) -> &'static str {
        match self {
            Self::GetCurrentTime => "get_current_time",
            Self::GetSystemStatus => "get_system_status",
            Self::OpenUrl => "open_url",
        }
    }

    /// Auto tools never require approval and are safe in every action mode.
    pub fn is_auto(&self) -> bool {
        matches!(self, Self::GetCurrentTime | Self::GetSystemStatus)
    }

    pub fn parse(&self, args: &serde_json::Value) -> Result<Parsed, String> {
        match self {
            Self::GetCurrentTime | Self::GetSystemStatus => {
                serde_json::from_value::<EmptyArgs>(args.clone())
                    .map_err(|_| "参数包含未允许的字段".to_owned())?;
                Ok(Parsed::Empty)
            }
            Self::OpenUrl => {
                let parsed: OpenUrlArgs = serde_json::from_value(args.clone())
                    .map_err(|_| "参数包含未允许的字段".to_owned())?;
                if parsed.url.len() > 2048 {
                    return Err("链接过长".to_owned());
                }
                let url = Url::parse(&parsed.url).map_err(|_| "链接格式无效".to_owned())?;
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
                Ok(Parsed::OpenUrl {
                    url: url.to_string(),
                    host,
                })
            }
        }
    }

    /// Persistent trust key used by "always allow" grants, when supported.
    pub fn bound_key(&self, parsed: &Parsed) -> Option<String> {
        match (self, parsed) {
            (Self::OpenUrl, Parsed::OpenUrl { host, .. }) => Some(format!("domain:{host}")),
            _ => None,
        }
    }

    /// Human-readable target and impact shown in the approval dialog.
    pub fn describe(&self, parsed: &Parsed) -> (String, String) {
        match (self, parsed) {
            (Self::OpenUrl, Parsed::OpenUrl { url, host }) => {
                (host.clone(), format!("用系统默认浏览器打开 {url}"))
            }
            (Self::GetCurrentTime, _) => ("本机时间".to_owned(), "只读，不产生副作用".to_owned()),
            (Self::GetSystemStatus, _) => ("应用状态".to_owned(), "只读，不产生副作用".to_owned()),
            _ => ("未知目标".to_owned(), "未知影响".to_owned()),
        }
    }

    pub fn execute(&self, parsed: &Parsed) -> Result<ToolOutcome, String> {
        match (self, parsed) {
            (Self::GetCurrentTime, _) => Ok(ToolOutcome::success(format!(
                "当前本地时间：{}",
                chrono::Local::now().format("%Y-%m-%d %H:%M:%S")
            ))),
            (Self::GetSystemStatus, _) => Ok(ToolOutcome::success(
                "桌面核心已运行；Agent 与语音状态请查看面板。".to_owned(),
            )),
            (Self::OpenUrl, Parsed::OpenUrl { url, .. }) => open_https(url),
            _ => Err("工具参数不匹配".to_owned()),
        }
    }
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
}
