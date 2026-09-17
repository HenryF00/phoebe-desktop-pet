//! Enumerates installed applications so the Agent can reference them by an
//! opaque id instead of an arbitrary filesystem path.
//!
//! The scan is bounded, skips symlinks and never descends into an application
//! bundle. Results are cached with a short TTL because the directories only
//! change when the user installs or removes software.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use serde::Serialize;

const CACHE_TTL: Duration = Duration::from_secs(600);
const MAX_APPS: usize = 600;
const MAX_DEPTH: usize = 3;

#[derive(Clone, Debug, Serialize)]
pub struct AppEntry {
    #[serde(rename = "appId")]
    pub app_id: String,
    pub name: String,
    /// Absolute bundle path. Never exposed to the model.
    #[serde(skip)]
    pub path: String,
}

#[derive(Default)]
pub struct AppCache {
    entries: Vec<AppEntry>,
    scanned_at: Option<Instant>,
}

impl AppCache {
    pub fn is_fresh(&self) -> bool {
        self.scanned_at
            .map(|scanned| scanned.elapsed() < CACHE_TTL)
            .unwrap_or(false)
    }

    pub fn refresh(&mut self) {
        self.entries = scan();
        self.scanned_at = Some(Instant::now());
    }

    pub fn entries(&self) -> &[AppEntry] {
        &self.entries
    }
}

pub fn scan() -> Vec<AppEntry> {
    let mut entries = Vec::new();
    let mut seen = BTreeSet::new();
    for root in roots() {
        collect(&root, 0, &mut entries, &mut seen);
        if entries.len() >= MAX_APPS {
            break;
        }
    }
    entries.sort_by(|left, right| left.name.to_lowercase().cmp(&right.name.to_lowercase()));
    entries.truncate(MAX_APPS);
    entries
}

#[cfg(target_os = "macos")]
fn roots() -> Vec<PathBuf> {
    let mut roots = vec![
        PathBuf::from("/Applications"),
        PathBuf::from("/Applications/Utilities"),
        PathBuf::from("/System/Applications"),
        PathBuf::from("/System/Applications/Utilities"),
    ];
    if let Some(home) = std::env::var_os("HOME") {
        roots.push(PathBuf::from(home).join("Applications"));
    }
    roots
}

#[cfg(target_os = "windows")]
fn roots() -> Vec<PathBuf> {
    let mut roots = Vec::new();
    for key in ["ProgramData", "APPDATA"] {
        if let Some(base) = std::env::var_os(key) {
            roots.push(PathBuf::from(base).join("Microsoft/Windows/Start Menu/Programs"));
        }
    }
    roots
}

#[cfg(all(unix, not(target_os = "macos")))]
fn roots() -> Vec<PathBuf> {
    vec![PathBuf::from("/usr/share/applications")]
}

fn is_app_bundle(name: &str) -> bool {
    cfg!(target_os = "macos") && name.ends_with(".app")
}

fn is_app_file(name: &str) -> bool {
    if cfg!(target_os = "windows") {
        let lower = name.to_ascii_lowercase();
        lower.ends_with(".lnk") || lower.ends_with(".exe")
    } else if cfg!(all(unix, not(target_os = "macos"))) {
        name.ends_with(".desktop")
    } else {
        false
    }
}

fn collect(dir: &Path, depth: usize, entries: &mut Vec<AppEntry>, seen: &mut BTreeSet<String>) {
    if depth > MAX_DEPTH || entries.len() >= MAX_APPS {
        return;
    }
    let Ok(reader) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in reader.flatten() {
        if entries.len() >= MAX_APPS {
            return;
        }
        let name = entry.file_name().to_string_lossy().to_string();
        if name.starts_with('.') {
            continue;
        }
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        if file_type.is_symlink() {
            continue;
        }
        let path = entry.path();
        if is_app_bundle(&name) {
            push(
                entries,
                seen,
                path.to_string_lossy().to_string(),
                name.trim_end_matches(".app").to_owned(),
            );
            continue;
        }
        if file_type.is_file() && is_app_file(&name) {
            let display = name
                .rsplit_once('.')
                .map(|(stem, _)| stem.to_owned())
                .unwrap_or_else(|| name.clone());
            push(entries, seen, path.to_string_lossy().to_string(), display);
            continue;
        }
        if file_type.is_dir() {
            collect(&path, depth + 1, entries, seen);
        }
    }
}

fn push(entries: &mut Vec<AppEntry>, seen: &mut BTreeSet<String>, path: String, name: String) {
    let app_id = format!("app-{:016x}", hash(&path));
    if seen.insert(app_id.clone()) {
        entries.push(AppEntry { app_id, name, path });
    }
}

/// Deterministic across runs so trust grants survive a restart.
fn hash(value: &str) -> u64 {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut hasher = DefaultHasher::new();
    value.hash(&mut hasher);
    hasher.finish()
}

#[cfg(test)]
mod tests {
    use super::{hash, is_app_bundle};

    #[test]
    fn app_ids_are_stable_and_bounded() {
        assert_eq!(hash("/Applications/Safari.app"), hash("/Applications/Safari.app"));
        assert_ne!(hash("/Applications/A.app"), hash("/Applications/B.app"));
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn macos_detects_app_bundles() {
        assert!(is_app_bundle("Safari.app"));
        assert!(!is_app_bundle("Safari"));
    }

    #[test]
    fn scan_is_bounded_and_uses_opaque_ids() {
        let entries = super::scan();
        assert!(entries.len() <= super::MAX_APPS);
        for entry in &entries {
            assert!(entry.app_id.starts_with("app-"));
            assert!(!entry.name.is_empty());
        }
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn macos_scan_finds_installed_apps() {
        let entries = super::scan();
        assert!(!entries.is_empty(), "expected at least one installed application");
        assert!(entries.iter().any(|entry| entry.path.ends_with(".app")));
    }
}
