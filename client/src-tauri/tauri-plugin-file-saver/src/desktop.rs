//! Desktop fallback — write to `$HOME/Downloads/` and keep an attachment-id
//! → path index in `$HOME/.local/share/messenger-tauri/saved_attachments.json`
//! so `fs_is_saved` can answer for prior runs.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

pub fn downloads_dir() -> Result<PathBuf, String> {
    let home = std::env::var("HOME").map_err(|e| format!("HOME: {e}"))?;
    Ok(PathBuf::from(home).join("Downloads"))
}

pub fn pick_unused_filename(dir: &Path, name: &str) -> PathBuf {
    let p = dir.join(name);
    if !p.exists() {
        return p;
    }
    let (stem, ext) = match name.rfind('.') {
        Some(i) if i != 0 => (&name[..i], &name[i..]),
        _ => (name, ""),
    };
    for n in 1..1000 {
        let candidate = dir.join(format!("{stem} ({n}){ext}"));
        if !candidate.exists() {
            return candidate;
        }
    }
    p
}

fn index_path() -> Result<PathBuf, String> {
    let home = std::env::var("HOME").map_err(|e| format!("HOME: {e}"))?;
    Ok(PathBuf::from(home).join(".local/share/messenger-tauri/saved_attachments.json"))
}

fn load_index() -> HashMap<String, String> {
    let p = match index_path() {
        Ok(p) => p,
        Err(_) => return HashMap::new(),
    };
    let bytes = match std::fs::read(&p) {
        Ok(b) => b,
        Err(_) => return HashMap::new(),
    };
    serde_json::from_slice(&bytes).unwrap_or_default()
}

pub fn save_index(aid: &str, path: &Path) -> Result<(), String> {
    let p = index_path()?;
    if let Some(parent) = p.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let mut idx = load_index();
    idx.insert(aid.to_string(), path.to_string_lossy().into_owned());
    let json = serde_json::to_vec_pretty(&idx).map_err(|e| e.to_string())?;
    std::fs::write(&p, json).map_err(|e| e.to_string())?;
    Ok(())
}

pub fn lookup_index(aid: &str) -> Option<String> {
    let idx = load_index();
    if let Some(path) = idx.get(aid) {
        if Path::new(path).exists() {
            return Some(path.clone());
        }
    }
    None
}
