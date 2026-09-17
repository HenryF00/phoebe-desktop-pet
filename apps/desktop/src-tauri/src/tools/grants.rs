//! Persistent, user-approved trust grants.
//!
//! Phase 1 only stores domains that the user explicitly marked as "always
//! allow" for `open_url`. Folder and application grants arrive in later phases
//! and will extend this file, never the Agent-visible surface: the model only
//! ever sees an opaque grant identifier, never an absolute path.

use std::collections::BTreeSet;
use std::fs;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};

pub const MAX_DOMAINS: usize = 200;

#[derive(Default, Serialize, Deserialize)]
pub struct Grants {
    #[serde(default)]
    domains: BTreeSet<String>,
}

impl Grants {
    pub fn load(app: &AppHandle) -> Result<Self, String> {
        let path = file(app)?;
        match fs::read(&path) {
            Ok(bytes) if bytes.len() <= 64 * 1024 => {
                serde_json::from_slice(&bytes).map_err(|_| "授权文件不可读取".to_owned())
            }
            Ok(_) => Err("授权文件过大".to_owned()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(_) => Err("无法读取授权文件".to_owned()),
        }
    }

    /// Only domain keys are understood in Phase 1.
    pub fn contains(&self, key: &str) -> bool {
        key.strip_prefix("domain:")
            .is_some_and(|domain| self.domains.contains(domain))
    }

    /// Returns true when the grant was newly added.
    pub fn insert(&mut self, key: &str) -> bool {
        let Some(domain) = key.strip_prefix("domain:") else {
            return false;
        };
        if self.domains.len() >= MAX_DOMAINS {
            return false;
        }
        self.domains.insert(domain.to_owned())
    }

    pub fn save(&self, app: &AppHandle) -> Result<(), String> {
        let path = file(app)?;
        fs::create_dir_all(path.parent().ok_or("应用数据目录不可用")?)
            .map_err(|_| "无法建立应用数据目录")?;
        let data = serde_json::to_vec_pretty(self).map_err(|_| "无法编码授权")?;
        let temp = path.with_extension("json.tmp");
        fs::write(&temp, data).map_err(|_| "无法写入授权")?;
        fs::rename(&temp, &path).map_err(|_| "无法保存授权".to_owned())?;
        Ok(())
    }
}

fn file(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(app
        .path()
        .app_data_dir()
        .map_err(|_| "无法取得应用数据目录")?
        .join("grants.json"))
}

#[cfg(test)]
mod tests {
    use super::Grants;

    #[test]
    fn grants_are_scoped_to_domain_keys() {
        let mut grants = Grants::default();
        assert!(grants.insert("domain:example.com"));
        assert!(!grants.insert("domain:example.com"));
        assert!(!grants.insert("folder:/etc"));
        assert!(grants.contains("domain:example.com"));
        assert!(!grants.contains("domain:evil.example"));
    }
}
