//! In-memory file-tool snapshots with conflict-aware undo.
use anyhow::{bail, Result};
use std::{
    fs,
    path::PathBuf,
    sync::{Mutex, OnceLock},
};
#[derive(Clone)]
struct Change {
    path: PathBuf,
    before: Option<Vec<u8>>,
    after: Vec<u8>,
}
static HISTORY: OnceLock<Mutex<Vec<Vec<Change>>>> = OnceLock::new();
fn history() -> &'static Mutex<Vec<Vec<Change>>> {
    HISTORY.get_or_init(|| Mutex::new(Vec::new()))
}
fn read(path: &PathBuf) -> Result<Option<Vec<u8>>> {
    match fs::read(path) {
        Ok(v) => Ok(Some(v)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e.into()),
    }
}
fn restore(c: &Change) -> Result<()> {
    match &c.before {
        Some(v) => fs::write(&c.path, v)?,
        None => {
            if c.path.exists() {
                fs::remove_file(&c.path)?;
            }
        }
    }
    Ok(())
}
pub fn apply(files: Vec<(PathBuf, Vec<u8>)>) -> Result<()> {
    let mut log = history().lock().unwrap();
    let batch: Vec<Change> = files
        .into_iter()
        .map(|(path, after)| {
            Ok(Change {
                before: read(&path)?,
                path,
                after,
            })
        })
        .collect::<Result<_>>()?;
    for (i, c) in batch.iter().enumerate() {
        if let Err(error) = fs::write(&c.path, &c.after) {
            let mut failures = Vec::new();
            for old in batch[..=i].iter().rev() {
                if let Err(e) = restore(old) {
                    failures.push(format!("{}: {e}", old.path.display()));
                }
            }
            if !failures.is_empty() {
                log.push(batch.clone());
            }
            bail!(
                "Write failed: {error}; rollback errors: {}",
                failures.join("; ")
            );
        }
    }
    log.push(batch);
    Ok(())
}
pub fn undo(all: bool) -> Result<String> {
    let mut log = history().lock().unwrap();
    let start = if all { 0 } else { log.len().saturating_sub(1) };
    let mut latest = std::collections::BTreeMap::new();
    for batch in &log[start..] {
        for c in batch {
            latest.insert(c.path.clone(), c.after.clone());
        }
    }
    for (path, expected) in latest {
        if read(&path)? != Some(expected) {
            bail!(
                "Undo conflict: {} changed outside file tools",
                path.display()
            );
        }
    }
    while log.len() > start {
        for c in log.last().unwrap().iter().rev() {
            restore(c)?;
        }
        log.pop();
    }
    Ok("Undo complete (file-tool changes only)".into())
}
pub fn diff() -> String {
    let log = history().lock().unwrap();
    let mut out = String::new();
    for batch in log.iter() {
        for c in batch {
            let old = String::from_utf8_lossy(c.before.as_deref().unwrap_or_default());
            let new = String::from_utf8_lossy(&c.after);
            out.push_str(&format!(
                "--- {}\n+++ {}\n@@ -1,{} +1,{} @@\n",
                c.path.display(),
                c.path.display(),
                old.lines().count(),
                new.lines().count()
            ));
            for line in old.lines() {
                out.push_str(&format!("-{line}\n"));
            }
            for line in new.lines() {
                out.push_str(&format!("+{line}\n"));
            }
        }
    }
    if out.is_empty() {
        "No recorded changes".into()
    } else {
        out
    }
}

pub fn revision() -> usize {
    history().lock().unwrap().len()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn snapshots_restore_bytes_and_remove_new_files() {
        let path = std::env::temp_dir().join(format!("original-restore-{}", std::process::id()));
        let c = Change {
            path: path.clone(),
            before: Some(b"original\r\n".to_vec()),
            after: b"new".to_vec(),
        };
        fs::write(&path, &c.after).unwrap();
        restore(&c).unwrap();
        assert_eq!(read(&path).unwrap(), c.before);
        restore(&Change { before: None, ..c }).unwrap();
        assert!(!path.exists());
    }
}
