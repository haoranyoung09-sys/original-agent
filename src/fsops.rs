//! Pure workspace file ops for vibe coding tools (no network, no shell).

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};

use crate::workspace::resolve_in_workspace;

const MAX_GREP_HITS: usize = 80;
const MAX_GREP_FILE_BYTES: u64 = 2_000_000;
const MAX_GLOB_RESULTS: usize = 200;
const MAX_TREE_ENTRIES: usize = 120;
const MAX_TREE_DEPTH: usize = 4;

#[derive(Debug, Clone)]
pub struct ReadRange {
    pub offset: Option<usize>,
    pub limit: Option<usize>,
}

fn extension(path: &Path) -> String {
    path.extension()
        .and_then(|v| v.to_str())
        .unwrap_or("")
        .to_ascii_lowercase()
}

fn office_bytes(path: &Path, content: Option<&str>) -> Result<Vec<u8>> {
    use std::io::Write;
    use std::process::{Command, Stdio};
    let mut child = Command::new("python")
        .args(["-I", "-X", "utf8", "-c", include_str!("office.py")])
        .arg(if content.is_some() { "write" } else { "read" })
        .arg(path)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .context("Office documents require Python 3, python-docx and python-pptx")?;
    if let Some(text) = content {
        let mut stdin = child.stdin.take().unwrap();
        let _ = stdin.write_all(text.as_bytes());
    }
    let output = child.wait_with_output()?;
    anyhow::ensure!(
        output.status.success(),
        "Office conversion failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(output.stdout)
}

pub fn read_text(root: &Path, path: &str, range: ReadRange) -> Result<String> {
    let resolved = resolve_in_workspace(root, path)?;
    let content = match extension(&resolved).as_str() {
        "docx" | "pptx" => String::from_utf8(office_bytes(&resolved, None)?)?,
        "png" | "jpg" | "jpeg" | "bmp" | "tif" | "tiff" => {
            let output = std::process::Command::new("tesseract")
                .arg(&resolved)
                .args(["stdout", "-l", "chi_sim+eng"])
                .output()
                .context("Install Tesseract OCR and chi_sim + eng language data")?;
            let diagnostics = String::from_utf8_lossy(&output.stderr);
            anyhow::ensure!(
                output.status.success() && !diagnostics.contains("Failed loading language"),
                "OCR failed: {diagnostics}"
            );
            String::from_utf8(output.stdout)?
        }
        _ => fs::read_to_string(&resolved)
            .with_context(|| format!("read failed {}", resolved.display()))?,
    };
    let lines: Vec<&str> = content.lines().collect();
    let start = range.offset.unwrap_or(1).saturating_sub(1);
    let end = match range.limit {
        Some(lim) => start.saturating_add(lim).min(lines.len()),
        None => lines.len(),
    };
    if start >= lines.len() {
        return Ok(format!(
            "(file has {} lines; offset {} is past EOF)",
            lines.len(),
            start + 1
        ));
    }
    let mut out = String::new();
    for (i, line) in lines[start..end].iter().enumerate() {
        out.push_str(&format!("{:>6}|{}\n", start + i + 1, line));
    }
    Ok(truncate_chars(&out, 80_000))
}

pub fn write_text(root: &Path, path: &str, content: &str) -> Result<String> {
    let resolved = resolve_in_workspace(root, path)?;
    if let Some(parent) = resolved.parent() {
        fs::create_dir_all(parent).with_context(|| format!("mkdir {}", parent.display()))?;
    }
    let bytes = match extension(&resolved).as_str() {
        "docx" | "pptx" => office_bytes(&resolved, Some(content))?,
        _ => content.as_bytes().to_vec(),
    };
    let size = bytes.len();
    crate::changes::apply(vec![(resolved.clone(), bytes)])
        .with_context(|| format!("write failed {}", resolved.display()))?;
    Ok(format!(
        "wrote {} bytes to {}",
        size,
        display_rel(root, &resolved)
    ))
}

pub fn edit_once(root: &Path, path: &str, old_str: &str, new_str: &str) -> Result<String> {
    if old_str.is_empty() {
        bail!("old_str must not be empty");
    }
    let resolved = resolve_in_workspace(root, path)?;
    let content = fs::read_to_string(&resolved)
        .with_context(|| format!("edit read failed {}", resolved.display()))?;
    let matches = content.matches(old_str).count();
    if matches == 0 {
        bail!("old_str not found in file (make sure it matches exactly, including whitespace)");
    }
    if matches > 1 {
        bail!(
            "old_str found {matches} times; provide a unique snippet so exactly one match is replaced"
        );
    }
    let updated = content.replacen(old_str, new_str, 1);
    crate::changes::apply(vec![(resolved.clone(), updated.into_bytes())])
        .with_context(|| format!("edit write failed {}", resolved.display()))?;
    Ok(format!(
        "edited {} (1 replacement)",
        display_rel(root, &resolved)
    ))
}

#[derive(Debug, Clone)]
pub struct FileEdit {
    pub path: String,
    pub old_str: String,
    pub new_str: String,
}

/// Apply multiple unique str-replaces across one or more files.
/// Validates every edit first, then writes — all-or-nothing for the batch.
pub fn multi_edit(root: &Path, edits: &[FileEdit]) -> Result<String> {
    if edits.is_empty() {
        bail!("edits must not be empty");
    }
    if edits.len() > 40 {
        bail!("too many edits in one batch (max 40)");
    }

    use std::collections::BTreeMap;
    let mut by_path: BTreeMap<PathBuf, (String, String)> = BTreeMap::new();
    let mut counts: BTreeMap<PathBuf, usize> = BTreeMap::new();

    for (idx, edit) in edits.iter().enumerate() {
        if edit.old_str.is_empty() {
            bail!("edits[{idx}].old_str must not be empty");
        }
        let resolved = resolve_in_workspace(root, &edit.path)?;
        if !by_path.contains_key(&resolved) {
            let content = fs::read_to_string(&resolved).with_context(|| {
                format!(
                    "multi_edit read failed {} (edits[{idx}])",
                    resolved.display()
                )
            })?;
            by_path.insert(resolved.clone(), (display_rel(root, &resolved), content));
        }
        let entry = by_path.get_mut(&resolved).expect("just inserted");
        let matches = entry.1.matches(&edit.old_str).count();
        if matches == 0 {
            bail!("edits[{idx}] old_str not found in {}", entry.0);
        }
        if matches > 1 {
            bail!(
                "edits[{idx}] old_str found {matches} times in {}; need a unique snippet",
                entry.0
            );
        }
        entry.1 = entry.1.replacen(&edit.old_str, &edit.new_str, 1);
        *counts.entry(resolved).or_insert(0) += 1;
    }

    crate::changes::apply(
        by_path
            .iter()
            .map(|(p, (_, next))| (p.clone(), next.as_bytes().to_vec()))
            .collect(),
    )?;
    let mut report = Vec::new();
    for (path, (rel, _next)) in &by_path {
        report.push(format!(
            "{}: {} replacement(s)",
            rel,
            counts.get(path).copied().unwrap_or(0)
        ));
    }
    Ok(format!(
        "multi_edit ok — {} file(s)\n{}",
        by_path.len(),
        report.join("\n")
    ))
}

pub fn glob_files(root: &Path, pattern: &str) -> Result<String> {
    let pattern = pattern.trim();
    if pattern.is_empty() {
        bail!("pattern must not be empty");
    }
    if pattern.contains("..") {
        bail!("pattern must not contain '..'");
    }
    let mut matches = Vec::new();
    walk_collect(root, root, 0, &mut |path, rel| {
        if matches.len() >= MAX_GLOB_RESULTS {
            return;
        }
        let rel_str = rel.to_string_lossy();
        if glob_match(pattern, &rel_str.replace('\\', "/")) {
            matches.push(rel_str.replace('\\', "/"));
        }
        let _ = path;
    })?;
    matches.sort();
    if matches.is_empty() {
        Ok(format!("(no files matched `{pattern}`)"))
    } else {
        let truncated = matches.len() >= MAX_GLOB_RESULTS;
        let mut out = matches.join("\n");
        if truncated {
            out.push_str(&format!("\n…(truncated at {MAX_GLOB_RESULTS})"));
        }
        Ok(out)
    }
}

pub fn grep_workspace(root: &Path, query: &str, glob_pat: Option<&str>) -> Result<String> {
    let query = query.trim();
    if query.is_empty() {
        bail!("query must not be empty");
    }
    if query.chars().count() > 200 {
        bail!("query too long (max 200 chars)");
    }
    let mut hits = Vec::new();
    let mut files_scanned = 0usize;
    walk_collect(root, root, 0, &mut |path, rel| {
        if hits.len() >= MAX_GREP_HITS {
            return;
        }
        let rel_str = rel.to_string_lossy().replace('\\', "/");
        if let Some(pat) = glob_pat {
            if !glob_match(pat, &rel_str) {
                return;
            }
        }
        if !looks_like_text_file(path) {
            return;
        }
        let meta = match fs::metadata(path) {
            Ok(m) => m,
            Err(_) => return,
        };
        if meta.len() > MAX_GREP_FILE_BYTES {
            return;
        }
        let Ok(content) = fs::read_to_string(path) else {
            return;
        };
        files_scanned += 1;
        for (i, line) in content.lines().enumerate() {
            if hits.len() >= MAX_GREP_HITS {
                break;
            }
            if line.contains(query) {
                let preview: String = line.chars().take(200).collect();
                hits.push(format!("{rel_str}:{}:{preview}", i + 1));
            }
        }
    })?;
    if hits.is_empty() {
        Ok(format!(
            "(no matches for `{query}` in {files_scanned} text file(s))"
        ))
    } else {
        let truncated = hits.len() >= MAX_GREP_HITS;
        let mut out = format!(
            "{} hit(s) in scan of ~{} text file(s)\n{}",
            hits.len(),
            files_scanned,
            hits.join("\n")
        );
        if truncated {
            out.push_str(&format!("\n…(truncated at {MAX_GREP_HITS} hits)"));
        }
        Ok(out)
    }
}

pub fn project_overview(root: &Path) -> Result<String> {
    let mut out = String::new();
    out.push_str(&format!("workspace: {}\n", root.display()));

    let markers = [
        ("Cargo.toml", "Rust (Cargo)"),
        ("package.json", "Node (npm/pnpm/yarn)"),
        ("pyproject.toml", "Python (pyproject)"),
        ("requirements.txt", "Python (requirements)"),
        ("go.mod", "Go"),
        ("pom.xml", "Java (Maven)"),
        ("build.gradle", "Java/Kotlin (Gradle)"),
        ("Gemfile", "Ruby"),
        ("composer.json", "PHP"),
        ("CMakeLists.txt", "C/C++ (CMake)"),
    ];
    let mut kinds = Vec::new();
    for (file, label) in markers {
        if root.join(file).is_file() {
            kinds.push(label);
        }
    }
    if kinds.is_empty() {
        out.push_str("project: (no common manifest detected)\n");
    } else {
        out.push_str(&format!("project: {}\n", kinds.join(", ")));
    }

    let mut entries = Vec::new();
    collect_tree(root, root, 0, &mut entries)?;
    out.push_str("tree:\n");
    if entries.is_empty() {
        out.push_str("  (empty)\n");
    } else {
        for line in &entries {
            out.push_str(line);
            out.push('\n');
        }
        if entries.len() >= MAX_TREE_ENTRIES {
            out.push_str(&format!("  …(truncated at {MAX_TREE_ENTRIES} entries)\n"));
        }
    }

    // Small README / 项目说明 snippets help vibe coding without a shell.
    for name in ["README.md", "README", "项目说明.md", "CONTRIBUTING.md"] {
        let p = root.join(name);
        if p.is_file() {
            if let Ok(text) = fs::read_to_string(&p) {
                let snippet: String = text.chars().take(800).collect();
                out.push_str(&format!("\n--- {name} (first ~800 chars) ---\n{snippet}\n"));
            }
        }
    }
    Ok(truncate_chars(&out, 40_000))
}

/// Suggest a non-destructive verify command for known project types.
pub fn suggest_verify_command(root: &Path) -> Option<&'static str> {
    if root.join("Cargo.toml").is_file() {
        Some("cargo test --locked")
    } else if root.join("package.json").is_file() {
        Some("npm test --silent")
    } else if root.join("pyproject.toml").is_file() || root.join("pytest.ini").is_file() {
        Some("python -m pytest -q")
    } else if root.join("go.mod").is_file() {
        Some("go test ./...")
    } else {
        None
    }
}

fn collect_tree(root: &Path, dir: &Path, depth: usize, out: &mut Vec<String>) -> Result<()> {
    if depth > MAX_TREE_DEPTH || out.len() >= MAX_TREE_ENTRIES {
        return Ok(());
    }
    let mut kids: Vec<_> = fs::read_dir(dir)
        .with_context(|| format!("list {}", dir.display()))?
        .filter_map(|e| e.ok())
        .collect();
    kids.sort_by_key(|e| e.file_name());
    for entry in kids {
        if out.len() >= MAX_TREE_ENTRIES {
            break;
        }
        let name = entry.file_name();
        let name_str = name.to_string_lossy();
        if should_skip_name(&name_str) {
            continue;
        }
        let path = entry.path();
        let rel = path.strip_prefix(root).unwrap_or(&path);
        let rel_s = rel.to_string_lossy().replace('\\', "/");
        let indent = "  ".repeat(depth);
        if path.is_dir() {
            out.push(format!("{indent}{rel_s}/"));
            collect_tree(root, &path, depth + 1, out)?;
        } else {
            out.push(format!("{indent}{rel_s}"));
        }
    }
    Ok(())
}

fn walk_collect<F>(root: &Path, dir: &Path, depth: usize, visit: &mut F) -> Result<()>
where
    F: FnMut(&Path, &Path),
{
    if depth > 12 {
        return Ok(());
    }
    let rd = match fs::read_dir(dir) {
        Ok(rd) => rd,
        Err(_) => return Ok(()),
    };
    for entry in rd.filter_map(|e| e.ok()) {
        let name = entry.file_name();
        let name_str = name.to_string_lossy();
        if should_skip_name(&name_str) {
            continue;
        }
        let path = entry.path();
        let rel = path.strip_prefix(root).unwrap_or(&path);
        if path.is_dir() {
            walk_collect(root, &path, depth + 1, visit)?;
        } else {
            visit(&path, rel);
        }
    }
    Ok(())
}

fn should_skip_name(name: &str) -> bool {
    matches!(
        name,
        ".git"
            | "target"
            | "node_modules"
            | ".original-agent"
            | "vendor"
            | "dist"
            | "build"
            | ".venv"
            | "__pycache__"
            | ".idea"
            | ".vscode"
    ) || (name.starts_with('.')
        && !matches!(
            name,
            ".env.example" | ".gitignore" | ".editorconfig" | ".gitattributes" | ".github"
        ))
}

fn looks_like_text_file(path: &Path) -> bool {
    match path
        .extension()
        .and_then(|e| e.to_str())
        .map(|s| s.to_ascii_lowercase())
    {
        Some(ext) => matches!(
            ext.as_str(),
            "rs" | "toml"
                | "md"
                | "txt"
                | "json"
                | "yml"
                | "yaml"
                | "js"
                | "ts"
                | "tsx"
                | "jsx"
                | "py"
                | "go"
                | "java"
                | "kt"
                | "c"
                | "h"
                | "cpp"
                | "hpp"
                | "cs"
                | "rb"
                | "php"
                | "sh"
                | "ps1"
                | "css"
                | "html"
                | "svg"
                | "sql"
                | "ini"
                | "cfg"
                | "lock"
        ),
        None => matches!(
            path.file_name().and_then(|n| n.to_str()),
            Some("Dockerfile" | "Makefile" | "LICENSE" | "README" | "Cargo.toml" | "Cargo.lock")
        ),
    }
}

/// Minimal glob: `*` and `?`, `/` separators, `**` = any path segments.
fn glob_match(pattern: &str, path: &str) -> bool {
    let pattern = pattern.replace('\\', "/");
    let path = path.replace('\\', "/");
    glob_match_inner(pattern.as_str(), path.as_str())
}

fn glob_match_inner(pattern: &str, path: &str) -> bool {
    if pattern == "**" {
        return true;
    }
    if let Some(rest) = pattern.strip_prefix("**/") {
        // Match at any path prefix.
        if glob_match_inner(rest, path) {
            return true;
        }
        let mut idx = 0;
        while let Some(slash) = path[idx..].find('/') {
            let next = idx + slash + 1;
            if glob_match_inner(rest, &path[next..]) {
                return true;
            }
            idx = next;
        }
        return false;
    }
    let (pat_head, pat_tail) = split_first_segment(pattern);
    let (path_head, path_tail) = split_first_segment(path);
    match (pat_head, path_head) {
        (None, None) => true,
        (Some(p), Some(h)) => segment_match(p, h) && glob_match_inner(pat_tail, path_tail),
        (Some(p), None) => p.is_empty() && pat_tail.is_empty(),
        (None, Some(_)) => false,
    }
}

fn split_first_segment(s: &str) -> (Option<&str>, &str) {
    if s.is_empty() {
        return (None, "");
    }
    match s.find('/') {
        Some(i) => (Some(&s[..i]), &s[i + 1..]),
        None => (Some(s), ""),
    }
}

fn segment_match(pat: &str, name: &str) -> bool {
    let pb: Vec<char> = pat.chars().collect();
    let nb: Vec<char> = name.chars().collect();
    fn rec(pb: &[char], nb: &[char]) -> bool {
        match (pb.first(), nb.first()) {
            (None, None) => true,
            (Some('*'), _) => {
                // * within one path segment
                for i in 0..=nb.len() {
                    if rec(&pb[1..], &nb[i..]) {
                        return true;
                    }
                }
                false
            }
            (Some('?'), Some(_)) => rec(&pb[1..], &nb[1..]),
            (Some(a), Some(b)) if a == b => rec(&pb[1..], &nb[1..]),
            _ => false,
        }
    }
    rec(&pb, &nb)
}

fn display_rel(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}

fn truncate_chars(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let t: String = s.chars().take(max).collect();
        format!("{t}\n…(truncated)")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_root(label: &str) -> PathBuf {
        let n = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("original-fsops-{label}-{n}"));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn read_write_edit_roundtrip() {
        let root = temp_root("rwe");
        write_text(&root, "src/a.rs", "fn main() {\n    println!(\"hi\");\n}\n").unwrap();
        let shown = read_text(
            &root,
            "src/a.rs",
            ReadRange {
                offset: Some(1),
                limit: Some(2),
            },
        )
        .unwrap();
        assert!(shown.contains("1|fn main()"));
        edit_once(&root, "src/a.rs", "println!(\"hi\");", "println!(\"hey\");").unwrap();
        let body = fs::read_to_string(root.join("src/a.rs")).unwrap();
        assert!(body.contains("hey"));
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn multi_edit_two_files_atomic_validate() {
        let root = temp_root("me");
        write_text(&root, "a.txt", "alpha\n").unwrap();
        write_text(&root, "b.txt", "beta\n").unwrap();
        let err = multi_edit(
            &root,
            &[
                FileEdit {
                    path: "a.txt".into(),
                    old_str: "alpha".into(),
                    new_str: "A".into(),
                },
                FileEdit {
                    path: "b.txt".into(),
                    old_str: "MISSING".into(),
                    new_str: "B".into(),
                },
            ],
        );
        assert!(err.is_err());
        assert_eq!(fs::read_to_string(root.join("a.txt")).unwrap(), "alpha\n");

        multi_edit(
            &root,
            &[
                FileEdit {
                    path: "a.txt".into(),
                    old_str: "alpha".into(),
                    new_str: "A".into(),
                },
                FileEdit {
                    path: "b.txt".into(),
                    old_str: "beta".into(),
                    new_str: "B".into(),
                },
            ],
        )
        .unwrap();
        assert_eq!(fs::read_to_string(root.join("a.txt")).unwrap(), "A\n");
        assert_eq!(fs::read_to_string(root.join("b.txt")).unwrap(), "B\n");
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn glob_and_grep_respect_workspace() {
        let root = temp_root("gg");
        write_text(&root, "src/lib.rs", "pub fn vibe() {}\n").unwrap();
        write_text(&root, "src/main.rs", "fn main() { vibe(); }\n").unwrap();
        write_text(&root, "README.md", "hello vibe coding\n").unwrap();
        let g = glob_files(&root, "src/*.rs").unwrap();
        assert!(g.contains("src/lib.rs"));
        assert!(g.contains("src/main.rs"));
        let hits = grep_workspace(&root, "vibe", Some("*.md")).unwrap();
        assert!(hits.contains("README.md"));
        assert!(!hits.contains("lib.rs"));
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn project_overview_detects_cargo() {
        let root = temp_root("po");
        write_text(&root, "Cargo.toml", "[package]\nname=\"x\"\n").unwrap();
        write_text(&root, "src/main.rs", "fn main() {}\n").unwrap();
        let overview = project_overview(&root).unwrap();
        assert!(overview.contains("Rust (Cargo)"));
        assert!(overview.contains("src/main.rs"));
        assert_eq!(suggest_verify_command(&root), Some("cargo test --locked"));
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn glob_match_double_star() {
        assert!(glob_match("**/*.rs", "src/a.rs"));
        assert!(glob_match("src/*.rs", "src/a.rs"));
        assert!(!glob_match("src/*.rs", "lib/a.rs"));
    }
}
