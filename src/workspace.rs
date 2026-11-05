//! Workspace path helpers: keep tool I/O inside the agent root.

use std::path::{Component, Path, PathBuf};

use anyhow::{bail, Context, Result};

/// Resolve `user_path` against `root`, forbidding `..` escape and absolute paths outside root.
pub fn resolve_in_workspace(root: &Path, user_path: &str) -> Result<PathBuf> {
    let user = Path::new(user_path);
    if user.components().any(|c| matches!(c, Component::ParentDir)) {
        bail!("路径禁止包含 '..'（不允许逃逸工作区）: {user_path}");
    }

    let joined = if user.is_absolute() {
        user.to_path_buf()
    } else {
        root.join(user)
    };

    // For existing paths, canonicalize; for new files, canonicalize parent then append name.
    let resolved = if joined.exists() {
        std::fs::canonicalize(&joined)
            .with_context(|| format!("无法规范化路径: {}", joined.display()))?
    } else {
        let mut ancestor = joined.as_path();
        let mut missing = Vec::new();
        while !ancestor.exists() {
            missing.push(ancestor.file_name().context("Invalid path")?.to_os_string());
            ancestor = ancestor.parent().context("No existing ancestor")?;
        }
        let mut resolved = std::fs::canonicalize(ancestor)?;
        for part in missing.into_iter().rev() {
            resolved.push(part);
        }
        resolved
    };

    let root_canon = std::fs::canonicalize(root)
        .with_context(|| format!("无法规范化工作区根: {}", root.display()))?;

    if !resolved.starts_with(&root_canon) {
        bail!(
            "路径超出工作区: {} (工作区: {})",
            resolved.display(),
            root_canon.display()
        );
    }

    Ok(resolved)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn rejects_parent_dir() {
        let root = std::env::temp_dir();
        assert!(resolve_in_workspace(&root, "../etc/passwd").is_err());
    }

    #[test]
    fn resolution_has_no_side_effects() {
        let root = std::env::temp_dir();
        let name = format!("original-no-create-{}/nested/file", std::process::id());
        assert!(resolve_in_workspace(&root, &name).is_ok());
        assert!(!root.join(name).parent().unwrap().exists());
    }

    #[test]
    fn accepts_relative() {
        let dir = std::env::temp_dir().join(format!("pi-ds-ws-{}", std::process::id()));
        let _ = fs::create_dir_all(&dir);
        let p = resolve_in_workspace(&dir, "a.txt").expect("ok");
        assert!(p.starts_with(std::fs::canonicalize(&dir).unwrap()));
        let _ = fs::remove_dir_all(&dir);
    }
}
