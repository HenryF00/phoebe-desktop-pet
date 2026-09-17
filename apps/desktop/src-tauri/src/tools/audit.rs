//! Append-only local audit trail for every mediated operation.
//!
//! Entries live in `audit.jsonl` inside the application data directory. The
//! in-memory ring buffer is capped, and the file is compacted when it grows
//! past a fixed size so it cannot grow without bound.

use std::collections::VecDeque;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};

const MAX_FILE_BYTES: u64 = 512 * 1024;
const MAX_MEMORY: usize = 500;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct AuditEntry {
    pub timestamp: String,
    #[serde(rename = "runId", default)]
    pub run_id: Option<String>,
    pub tool: String,
    pub target: String,
    pub decision: String,
    pub outcome: String,
    pub message: String,
}

#[derive(Default)]
pub struct AuditLog {
    path: Option<PathBuf>,
    entries: VecDeque<AuditEntry>,
}

impl AuditLog {
    pub fn init(&mut self, app: &AppHandle) -> Result<(), String> {
        let path = app
            .path()
            .app_data_dir()
            .map_err(|_| "无法取得应用数据目录")?
            .join("audit.jsonl");
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|_| "无法建立审计目录")?;
        }
        let mut entries = VecDeque::new();
        if let Ok(bytes) = fs::read(&path) {
            for line in String::from_utf8_lossy(&bytes).lines().rev().take(MAX_MEMORY) {
                if let Ok(entry) = serde_json::from_str::<AuditEntry>(line) {
                    entries.push_front(entry);
                }
            }
        }
        self.path = Some(path);
        self.entries = entries;
        Ok(())
    }

    pub fn record(&mut self, entry: AuditEntry) {
        if self.entries.len() >= MAX_MEMORY {
            self.entries.pop_front();
        }
        self.entries.push_back(entry.clone());
        let Some(path) = &self.path else { return };
        if fs::metadata(path)
            .map(|metadata| metadata.len() > MAX_FILE_BYTES)
            .unwrap_or(false)
        {
            let kept: Vec<String> = self
                .entries
                .iter()
                .filter_map(|entry| serde_json::to_string(entry).ok())
                .collect();
            let _ = fs::write(path, format!("{}\n", kept.join("\n")));
            return;
        }
        if let Ok(line) = serde_json::to_string(&entry) {
            if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(path) {
                let _ = writeln!(file, "{line}");
            }
        }
    }

    pub fn recent(&self, limit: usize) -> Vec<AuditEntry> {
        self.entries.iter().rev().take(limit).cloned().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::{AuditEntry, AuditLog};

    fn entry(tool: &str) -> AuditEntry {
        AuditEntry {
            timestamp: "2026-09-17T00:00:00Z".to_owned(),
            run_id: Some("r-1".to_owned()),
            tool: tool.to_owned(),
            target: "example.com".to_owned(),
            decision: "allow_once".to_owned(),
            outcome: "success".to_owned(),
            message: "ok".to_owned(),
        }
    }

    #[test]
    fn recent_returns_newest_first_without_a_file() {
        let mut log = AuditLog::default();
        log.record(entry("open_url"));
        log.record(entry("get_current_time"));
        let recent = log.recent(10);
        assert_eq!(recent.len(), 2);
        assert_eq!(recent[0].tool, "get_current_time");
    }
}
