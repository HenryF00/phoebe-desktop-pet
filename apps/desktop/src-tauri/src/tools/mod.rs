//! The single Rust-owned gateway for every operating-system tool the Agent can
//! request.
//!
//! The Node sidecar never executes an OS action itself. It emits a
//! `tool_request`, the gateway validates the caller's per-run allow-list, the
//! argument schema and the current policy, optionally asks the user, executes
//! the action and writes a `tool_result` back. Approval is always shown by Rust
//! (React modal first, native dialog as fallback), so a compromised or
//! prompt-injected sidecar cannot approve its own request.

mod audit;
mod grants;
mod registry;

pub use audit::AuditEntry;

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::time::Duration;

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_dialog::{DialogExt, MessageDialogButtons, MessageDialogKind};

use crate::settings::{ActionMode, InteractionMode};
use registry::{Parsed, Tool as ToolSpec};

/// How long a user has to answer an approval before it is denied.
const APPROVAL_TIMEOUT: Duration = Duration::from_secs(60);

/// Tools registered for one conversation turn. Rust is authoritative: the
/// Node sidecar receives exactly this list and the gateway independently
/// rejects anything outside it.
pub fn allowed_tools_for(
    interaction_mode: InteractionMode,
    action_mode: ActionMode,
) -> Vec<String> {
    let mut tools: Vec<String> = [
        "web_search",
        "read_selected_file",
        "get_device_location",
        "get_current_time",
        "get_system_status",
    ]
    .iter()
    .map(|name| (*name).to_owned())
    .collect();
    if interaction_mode == InteractionMode::Assistant && action_mode != ActionMode::Disabled {
        tools.push("open_url".to_owned());
    }
    tools
}

#[derive(Clone, Serialize)]
pub struct ApprovalView {
    pub id: String,
    #[serde(rename = "runId")]
    pub run_id: String,
    pub tool: String,
    pub target: String,
    pub impact: String,
    pub scope: &'static str,
    pub rememberable: bool,
}

pub struct ToolOutcome {
    status: &'static str,
    text: String,
    details: serde_json::Value,
    is_error: bool,
}

impl ToolOutcome {
    pub fn success(text: impl Into<String>) -> Self {
        Self {
            status: "success",
            text: text.into(),
            details: serde_json::Value::Null,
            is_error: false,
        }
    }

    pub fn failed(text: impl Into<String>) -> Self {
        Self {
            status: "failed",
            text: text.into(),
            details: serde_json::Value::Null,
            is_error: true,
        }
    }

    pub fn denied(text: impl Into<String>) -> Self {
        Self {
            status: "denied",
            text: text.into(),
            details: serde_json::Value::Null,
            is_error: true,
        }
    }

    fn into_line(self, request_id: &str) -> String {
        serde_json::json!({
            "type": "tool_result",
            "requestId": request_id,
            "status": self.status,
            "content": [{ "type": "text", "text": self.text }],
            "details": self.details,
            "isError": self.is_error,
        })
        .to_string()
    }
}

#[derive(Clone, Default)]
pub struct ToolBroker {
    inner: Arc<Inner>,
}

#[derive(Default)]
struct Inner {
    approvals: Mutex<HashMap<String, PendingApproval>>,
    audit: Mutex<audit::AuditLog>,
    grants: Mutex<grants::Grants>,
    counter: AtomicU64,
}

struct PendingApproval {
    view: ApprovalView,
    sender: mpsc::Sender<Resolution>,
}

#[derive(Clone, Copy)]
enum Resolution {
    Deny,
    AllowOnce,
    AllowAlways,
}

enum Decision {
    Allow,
    Deny,
    Ask { rememberable: bool },
}

impl ToolBroker {
    pub fn load(&self, app: &AppHandle) -> Result<(), String> {
        self.inner
            .audit
            .lock()
            .map_err(|_| "审计状态不可用")?
            .init(app)?;
        *self.inner.grants.lock().map_err(|_| "授权状态不可用")? =
            grants::Grants::load(app)?;
        Ok(())
    }

    /// Handles one `tool_request` and returns the JSONL `tool_result` line that
    /// must be written back to the sidecar's stdin.
    pub fn handle_request(
        &self,
        app: &AppHandle,
        run_id: &str,
        tool_name: &str,
        args: &serde_json::Value,
        request_id: &str,
    ) -> String {
        let Some(spec) = ToolSpec::from_name(tool_name) else {
            self.record_audit(run_id, tool_name, "", "deny", "denied", "未注册的工具");
            return ToolOutcome::denied("该工具未注册").into_line(request_id);
        };
        let parsed = match spec.parse(args) {
            Ok(parsed) => parsed,
            Err(message) => {
                self.record_audit(run_id, tool_name, "", "deny", "denied", &message);
                return ToolOutcome::denied(message).into_line(request_id);
            }
        };
        let (target, impact) = spec.describe(&parsed);

        let mut decision_label = "auto".to_owned();
        if !spec.is_auto() {
            match self.decide(app, &spec, &parsed) {
                Decision::Deny => {
                    self.record_audit(run_id, tool_name, &target, "deny", "denied", "当前策略不允许");
                    return ToolOutcome::denied("当前操作策略不允许该工具").into_line(request_id);
                }
                Decision::Allow => decision_label = "allow_trusted".to_owned(),
                Decision::Ask { rememberable } => {
                    match self.ask(app, run_id, &spec, &target, &impact, rememberable) {
                        Resolution::Deny => {
                            self.record_audit(run_id, tool_name, &target, "deny", "denied", "用户拒绝");
                            return ToolOutcome::denied("用户拒绝了该操作").into_line(request_id);
                        }
                        Resolution::AllowOnce => decision_label = "allow_once".to_owned(),
                        Resolution::AllowAlways => {
                            decision_label = "allow_always".to_owned();
                            if let Some(key) = spec.bound_key(&parsed) {
                                self.remember_grant(app, &key);
                            }
                        }
                    }
                }
            }
        }

        let outcome = spec
            .execute(&parsed)
            .unwrap_or_else(ToolOutcome::failed);
        let outcome_label = if outcome.is_error { "failed" } else { "success" };
        self.record_audit(run_id, tool_name, &target, &decision_label, outcome_label, &outcome.text);
        outcome.into_line(request_id)
    }

    pub fn resolve(&self, approval_id: &str, decision: &str) -> Result<(), String> {
        let resolution = match decision {
            "deny" => Resolution::Deny,
            "allow_once" => Resolution::AllowOnce,
            "always" => Resolution::AllowAlways,
            _ => return Err("无效的审批决定".to_owned()),
        };
        let mut approvals = self.inner.approvals.lock().map_err(|_| "审批状态不可用")?;
        let pending = approvals.remove(approval_id).ok_or("该审批已过期或不存在")?;
        if matches!(resolution, Resolution::AllowAlways) && !pending.view.rememberable {
            return Err("当前策略不允许长期授权".to_owned());
        }
        pending.sender.send(resolution).map_err(|_| "审批已取消".to_owned())
    }

    pub fn pending_view(&self) -> Option<ApprovalView> {
        self.inner
            .approvals
            .lock()
            .ok()
            .and_then(|approvals| approvals.values().next().map(|pending| pending.view.clone()))
    }

    pub fn deny_run(&self, run_id: &str) {
        let ids: Vec<String> = self
            .inner
            .approvals
            .lock()
            .map(|approvals| {
                approvals
                    .iter()
                    .filter(|(_, pending)| pending.view.run_id == run_id)
                    .map(|(id, _)| id.clone())
                    .collect()
            })
            .unwrap_or_default();
        for id in ids {
            let _ = self.resolve(&id, "deny");
        }
    }

    pub fn deny_all(&self) {
        let ids: Vec<String> = self
            .inner
            .approvals
            .lock()
            .map(|approvals| approvals.keys().cloned().collect())
            .unwrap_or_default();
        for id in ids {
            let _ = self.resolve(&id, "deny");
        }
    }

    pub fn recent_audit(&self, limit: usize) -> Vec<AuditEntry> {
        self.inner
            .audit
            .lock()
            .map(|audit| audit.recent(limit))
            .unwrap_or_default()
    }

    fn decide(&self, app: &AppHandle, spec: &ToolSpec, parsed: &Parsed) -> Decision {
        let mode = app
            .state::<crate::settings::SettingsStore>()
            .get()
            .map(|settings| settings.action_mode)
            .unwrap_or(ActionMode::Disabled);
        match mode {
            ActionMode::Disabled => Decision::Deny,
            ActionMode::Standard => Decision::Ask { rememberable: false },
            ActionMode::Trust => match spec.bound_key(parsed) {
                Some(key) => {
                    let granted = self
                        .inner
                        .grants
                        .lock()
                        .map(|grants| grants.contains(&key))
                        .unwrap_or(false);
                    if granted {
                        Decision::Allow
                    } else {
                        Decision::Ask { rememberable: true }
                    }
                }
                None => Decision::Ask { rememberable: false },
            },
        }
    }

    fn ask(
        &self,
        app: &AppHandle,
        run_id: &str,
        spec: &ToolSpec,
        target: &str,
        impact: &str,
        rememberable: bool,
    ) -> Resolution {
        let id = self.new_approval_id();
        let (sender, receiver) = mpsc::channel();
        let view = ApprovalView {
            id: id.clone(),
            run_id: run_id.to_owned(),
            tool: spec.name().to_owned(),
            target: target.to_owned(),
            impact: impact.to_owned(),
            scope: if rememberable { "bound_target" } else { "once" },
            rememberable,
        };
        {
            let Ok(mut approvals) = self.inner.approvals.lock() else {
                return Resolution::Deny;
            };
            approvals.insert(
                id.clone(),
                PendingApproval {
                    view: view.clone(),
                    sender,
                },
            );
        }

        let primary = crate::window_manager::show_chat(app).is_ok()
            && crate::window_manager::is_chat_visible(app).unwrap_or(false);
        let resolution = if primary {
            let _ = app.emit(
                "assistant-event",
                serde_json::json!({
                    "type": "state",
                    "runId": run_id,
                    "state": "awaiting_approval",
                }),
            );
            let _ = app.emit("approval-required", view.clone());
            receiver.recv_timeout(APPROVAL_TIMEOUT).unwrap_or(Resolution::Deny)
        } else {
            self.native_ask(app, &view)
        };

        if let Ok(mut approvals) = self.inner.approvals.lock() {
            approvals.remove(&id);
        }
        resolution
    }

    fn native_ask(&self, app: &AppHandle, view: &ApprovalView) -> Resolution {
        let confirmed = app
            .dialog()
            .message(format!(
                "菲比请求执行操作。\n\n操作：{}\n目标：{}\n影响：{}\n\n仅在确认安全时允许。原生确认框不提供长期授权，可回到聊天窗使用「始终允许」。",
                view.tool, view.target, view.impact
            ))
            .title("菲比助手操作确认")
            .kind(MessageDialogKind::Warning)
            .buttons(MessageDialogButtons::YesNo)
            .blocking_show();
        if confirmed {
            Resolution::AllowOnce
        } else {
            Resolution::Deny
        }
    }

    fn remember_grant(&self, app: &AppHandle, key: &str) {
        let Ok(mut grants) = self.inner.grants.lock() else {
            return;
        };
        if grants.insert(key) {
            let _ = grants.save(app);
        }
    }

    fn record_audit(
        &self,
        run_id: &str,
        tool: &str,
        target: &str,
        decision: &str,
        outcome: &str,
        message: &str,
    ) {
        if let Ok(mut audit) = self.inner.audit.lock() {
            audit.record(AuditEntry {
                timestamp: chrono::Utc::now().to_rfc3339(),
                run_id: Some(run_id.to_owned()),
                tool: tool.to_owned(),
                target: target.to_owned(),
                decision: decision.to_owned(),
                outcome: outcome.to_owned(),
                message: message.chars().take(200).collect(),
            });
        }
    }

    /// A short, unguessable local identifier. `RandomState` is seeded from OS
    /// entropy, and the counter guarantees uniqueness within one process.
    fn new_approval_id(&self) -> String {
        use std::collections::hash_map::RandomState;
        use std::hash::{BuildHasher, Hasher};
        let counter = self.inner.counter.fetch_add(1, Ordering::Relaxed);
        let mut left = RandomState::new().build_hasher();
        left.write_u64(counter);
        let mut right = RandomState::new().build_hasher();
        right.write_u128(
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|duration| duration.as_nanos())
                .unwrap_or(0),
        );
        format!("{:016x}{:016x}", left.finish(), right.finish())
    }
}

#[cfg(test)]
mod tests {
    use super::{allowed_tools_for, ToolBroker};
    use crate::settings::{ActionMode, InteractionMode};

    #[test]
    fn chat_mode_hides_operation_tools() {
        let chat = allowed_tools_for(InteractionMode::Chat, ActionMode::Standard);
        assert!(!chat.iter().any(|name| name == "open_url"));
        let disabled = allowed_tools_for(InteractionMode::Assistant, ActionMode::Disabled);
        assert!(!disabled.iter().any(|name| name == "open_url"));
        let assistant = allowed_tools_for(InteractionMode::Assistant, ActionMode::Standard);
        assert!(assistant.iter().any(|name| name == "open_url"));
    }

    #[test]
    fn approvals_are_bounded_to_the_latest_state() {
        let broker = ToolBroker::default();
        assert!(broker.pending_view().is_none());
        assert!(broker.resolve("missing", "allow_once").is_err());
        assert!(broker.resolve("missing", "bogus").is_err());
    }
}
