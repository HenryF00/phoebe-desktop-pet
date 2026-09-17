//! Path confinement for granted folders.
//!
//! The Agent only ever supplies a relative path. Everything here resolves the
//! real path, rejects escapes (`..`, absolute paths, symlinks pointing outside
//! the grant) and refuses a small set of sensitive credential locations even
//! when they sit inside a granted folder.

use std::path::{Component, Path, PathBuf};

use tauri::{AppHandle, Manager};

pub struct PathPolicy {
    denied_roots: Vec<PathBuf>,
    denied_names: [&'static str; 7],
}

impl PathPolicy {
    pub fn from_app(app: &AppHandle) -> Self {
        let mut denied_roots = Vec::new();
        if let Ok(dir) = app.path().app_data_dir() {
            denied_roots.push(canonical_or(dir));
        }
        if let Ok(home) = app.path().home_dir() {
            for relative in [
                ".ssh",
                ".aws",
                ".gnupg",
                "Library/Keychains",
                "Library/Cookies",
            ] {
                denied_roots.push(canonical_or(home.join(relative)));
            }
        }
        Self {
            denied_roots,
            denied_names: [
                ".env",
                ".netrc",
                ".git-credentials",
                "id_rsa",
                "id_ed25519",
                "known_hosts",
                ".htpasswd",
            ],
        }
    }

    pub fn is_denied(&self, path: &Path) -> bool {
        if self
            .denied_roots
            .iter()
            .any(|root| !root.as_os_str().is_empty() && path.starts_with(root))
        {
            return true;
        }
        if let Some(name) = path.file_name().and_then(|name| name.to_str()) {
            if self
                .denied_names
                .iter()
                .any(|denied| name.eq_ignore_ascii_case(denied))
            {
                return true;
            }
        }
        false
    }
}

/// Resolves a path for reading inside a granted folder, following symlinks and
/// verifying the result still lives under the grant root.
pub fn resolve_read_path(
    root: &Path,
    relative: &str,
    policy: &PathPolicy,
) -> Result<PathBuf, String> {
    let relative = safe_relative(relative)?;
    let root = std::fs::canonicalize(root).map_err(|_| "授权目录当前不可用".to_owned())?;
    let canonical = std::fs::canonicalize(root.join(&relative))
        .map_err(|_| "文件或目录不存在".to_owned())?;
    if !canonical.starts_with(&root) {
        return Err("路径超出授权目录".to_owned());
    }
    if policy.is_denied(&canonical) {
        return Err("该路径属于受保护的敏感位置".to_owned());
    }
    Ok(canonical)
}

/// Resolves a path for writing inside a granted folder. The parent directory
/// must already exist and stay inside the grant. The result is not
/// canonicalized, so callers can still detect and reject a symlinked target
/// that would otherwise redirect the write outside the grant.
pub fn resolve_write_path(
    root: &Path,
    relative: &str,
    policy: &PathPolicy,
) -> Result<PathBuf, String> {
    let relative = safe_relative(relative)?;
    if relative.as_os_str().is_empty() {
        return Err("必须指定文件名".to_owned());
    }
    let root = std::fs::canonicalize(root).map_err(|_| "授权目录当前不可用".to_owned())?;
    let candidate = root.join(&relative);
    let parent = candidate.parent().ok_or_else(|| "目标路径无效".to_owned())?;
    let parent = std::fs::canonicalize(parent).map_err(|_| "目标目录不存在".to_owned())?;
    if !parent.starts_with(&root) {
        return Err("路径超出授权目录".to_owned());
    }
    let name = candidate.file_name().ok_or_else(|| "目标文件名无效".to_owned())?;
    let target = parent.join(name);
    if policy.is_denied(&target) {
        return Err("该路径属于受保护的敏感位置".to_owned());
    }
    if std::fs::symlink_metadata(&target)
        .map(|metadata| metadata.file_type().is_symlink())
        .unwrap_or(false)
    {
        return Err("目标是符号链接，已拒绝写入".to_owned());
    }
    Ok(target)
}

/// Rejects absolute paths, parent traversal and other non-relative components.
/// An empty result means "the grant root itself".
pub fn safe_relative(relative: &str) -> Result<PathBuf, String> {
    if relative.len() > 1024 {
        return Err("相对路径过长".to_owned());
    }
    let path = Path::new(relative);
    if path.is_absolute() {
        return Err("只允许相对路径".to_owned());
    }
    let mut safe = PathBuf::new();
    for component in path.components() {
        match component {
            Component::Normal(part) => safe.push(part),
            Component::CurDir => {}
            _ => return Err("路径包含不允许的片段".to_owned()),
        }
    }
    Ok(safe)
}

fn canonical_or(path: PathBuf) -> PathBuf {
    std::fs::canonicalize(&path).unwrap_or(path)
}

#[cfg(test)]
mod tests {
    use super::{resolve_read_path, safe_relative, PathPolicy};
    use std::path::PathBuf;

    fn empty_policy() -> PathPolicy {
        PathPolicy {
            denied_roots: Vec::new(),
            denied_names: [".env", ".netrc", ".git-credentials", "id_rsa", "id_ed25519", "known_hosts", ".htpasswd"],
        }
    }

    #[test]
    fn relative_paths_reject_escapes() {
        assert!(safe_relative("notes/a.txt").is_ok());
        assert!(safe_relative("./a.txt").is_ok());
        assert!(safe_relative("").is_ok());
        assert!(safe_relative("../secret").is_err());
        assert!(safe_relative("a/../../secret").is_err());
        assert!(safe_relative("/etc/passwd").is_err());
        assert!(safe_relative(&"x".repeat(1025)).is_err());
    }

    #[test]
    fn reads_stay_inside_the_grant_root() {
        let root = std::env::temp_dir().join(format!("phoebe-paths-{}", std::process::id()));
        let inner = root.join("inner");
        std::fs::create_dir_all(&inner).unwrap();
        std::fs::write(inner.join("note.txt"), "hello").unwrap();
        let policy = empty_policy();

        let resolved = resolve_read_path(&root, "inner/note.txt", &policy).unwrap();
        assert!(resolved.ends_with(PathBuf::from("inner/note.txt")));
        assert!(resolve_read_path(&root, "../escape.txt", &policy).is_err());
        assert!(resolve_read_path(&root, "missing.txt", &policy).is_err());

        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn sensitive_names_are_denied_even_inside_a_grant() {
        let root = std::env::temp_dir().join(format!("phoebe-paths-secret-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(root.join(".env"), "SECRET=1").unwrap();
        let policy = empty_policy();
        assert!(resolve_read_path(&root, ".env", &policy).is_err());
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn symlinks_cannot_escape_the_grant_root() {
        let base = std::env::temp_dir().join(format!("phoebe-paths-link-{}", std::process::id()));
        let root = base.join("grant");
        let outside = base.join("outside");
        std::fs::create_dir_all(&root).unwrap();
        std::fs::create_dir_all(&outside).unwrap();
        std::fs::write(outside.join("secret.txt"), "top secret").unwrap();
        std::os::unix::fs::symlink(&outside, root.join("link")).unwrap();
        let policy = empty_policy();
        // The symlink resolves outside the grant root, so it must be rejected.
        assert!(resolve_read_path(&root, "link/secret.txt", &policy).is_err());
        std::fs::remove_dir_all(&base).unwrap();
    }

    #[test]
    fn write_paths_stay_inside_and_reject_symlinked_targets() {
        use super::resolve_write_path;
        let root = std::env::temp_dir().join(format!("phoebe-paths-write-{}", std::process::id()));
        std::fs::create_dir_all(root.join("sub")).unwrap();
        let policy = empty_policy();
        assert!(resolve_write_path(&root, "sub/new.txt", &policy).is_ok());
        assert!(resolve_write_path(&root, "", &policy).is_err());
        assert!(resolve_write_path(&root, "../escape.txt", &policy).is_err());
        assert!(resolve_write_path(&root, "missing/new.txt", &policy).is_err());
        assert!(resolve_write_path(&root, "sub/.env", &policy).is_err());

        #[cfg(unix)]
        {
            let link = root.join("sub/link.txt");
            std::os::unix::fs::symlink(root.join("sub/new.txt"), &link).unwrap();
            assert!(resolve_write_path(&root, "sub/link.txt", &policy).is_err());
        }
        std::fs::remove_dir_all(&root).unwrap();
    }
}
