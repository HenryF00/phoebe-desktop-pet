//! Persistent, user-approved trust grants.
//!
//! Phase 2a stores folder grants created through a native directory picker.
//! The Agent never sees an absolute path: it only receives an opaque
//! `grantId` plus a human label, and the broker resolves the real path
//! internally. Domain grants (Phase 1) live in the same file.

use std::collections::BTreeSet;
use std::fs;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};

pub const MAX_DOMAINS: usize = 200;
pub const MAX_FOLDERS: usize = 100;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FolderGrant {
    /// Opaque identifier shared with the Agent.
    pub id: String,
    /// Canonical absolute path. Never exposed to the model.
    pub path: String,
    pub label: String,
    pub read: bool,
    pub write: bool,
    #[serde(rename = "createdAt")]
    pub created_at: String,
}

/// Grant data that is safe to hand to the model.
#[derive(Clone, Debug, Serialize)]
pub struct GrantSummary {
    #[serde(rename = "grantId")]
    pub grant_id: String,
    pub label: String,
    pub read: bool,
    pub write: bool,
}

#[derive(Default, Serialize, Deserialize)]
pub struct Grants {
    #[serde(default)]
    domains: BTreeSet<String>,
    #[serde(default)]
    folders: Vec<FolderGrant>,
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

    /// Checks a trust key. Domain keys require an exact match; folder keys
    /// require an existing grant with read permission.
    pub fn contains(&self, key: &str) -> bool {
        if let Some(domain) = key.strip_prefix("domain:") {
            return self.domains.contains(domain);
        }
        if let Some(id) = key.strip_prefix("folder:") {
            return self.folders.iter().any(|folder| folder.id == id && folder.read);
        }
        false
    }

    /// Only domain keys are added here; folders use [`Self::add_folder`].
    pub fn insert(&mut self, key: &str) -> bool {
        let Some(domain) = key.strip_prefix("domain:") else {
            return false;
        };
        if self.domains.len() >= MAX_DOMAINS {
            return false;
        }
        self.domains.insert(domain.to_owned())
    }

    pub fn folder(&self, id: &str) -> Option<&FolderGrant> {
        self.folders.iter().find(|folder| folder.id == id)
    }

    pub fn summaries(&self) -> Vec<GrantSummary> {
        self.folders
            .iter()
            .map(|folder| GrantSummary {
                grant_id: folder.id.clone(),
                label: folder.label.clone(),
                read: folder.read,
                write: folder.write,
            })
            .collect()
    }

    /// Adds or upgrades a folder grant. Callers must pass a canonical path.
    /// Returns the grant id (existing or newly created).
    pub fn add_folder(&mut self, path: &str, label: &str, read: bool, write: bool) -> Result<String, String> {
        if let Some(existing) = self.folders.iter_mut().find(|folder| folder.path == path) {
            existing.read |= read;
            existing.write |= write;
            return Ok(existing.id.clone());
        }
        if self.folders.len() >= MAX_FOLDERS {
            return Err("已达到授权文件夹数量上限".to_owned());
        }
        let id = new_grant_id();
        self.folders.push(FolderGrant {
            id: id.clone(),
            path: path.to_owned(),
            label: label.to_owned(),
            read,
            write,
            created_at: chrono::Utc::now().to_rfc3339(),
        });
        Ok(id)
    }

    pub fn folder_views(&self) -> Vec<FolderGrant> {
        self.folders.clone()
    }

    pub fn remove_folder(&mut self, id: &str) -> bool {
        let before = self.folders.len();
        self.folders.retain(|folder| folder.id != id);
        self.folders.len() != before
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

/// A short, unguessable identifier seeded from OS entropy.
pub fn new_grant_id() -> String {
    use std::collections::hash_map::RandomState;
    use std::hash::{BuildHasher, Hasher};
    let mut left = RandomState::new().build_hasher();
    left.write_u128(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or(0),
    );
    let mut right = RandomState::new().build_hasher();
    right.write_u64(std::process::id() as u64);
    format!("{:016x}{:016x}", left.finish(), right.finish())
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
    fn domains_and_folders_are_scoped_to_their_keys() {
        let mut grants = Grants::default();
        assert!(grants.insert("domain:example.com"));
        assert!(!grants.insert("domain:example.com"));
        assert!(!grants.insert("folder:/etc"));
        assert!(grants.contains("domain:example.com"));
        assert!(!grants.contains("domain:evil.example"));

        assert!(grants.add_folder("/Users/me/Notes", "Notes", true, false).is_ok());
        let id = grants.summaries()[0].grant_id.clone();
        assert!(grants.contains(&format!("folder:{id}")));
        assert!(!grants.contains("folder:missing"));
    }

    #[test]
    fn re_granting_a_folder_upgrades_permissions() {
        let mut grants = Grants::default();
        grants.add_folder("/Users/me/Notes", "Notes", true, false).unwrap();
        grants.add_folder("/Users/me/Notes", "Notes", true, true).unwrap();
        assert_eq!(grants.summaries().len(), 1);
        let summary = &grants.summaries()[0];
        assert!(summary.read && summary.write);
        let id = summary.grant_id.clone();
        assert!(grants.remove_folder(&id));
        assert!(!grants.remove_folder(&id));
        assert!(grants.summaries().is_empty());
    }
}
