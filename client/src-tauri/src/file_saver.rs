//! File saver — write attachment bytes to Downloads, lookup by attachment id, open with system viewer.
//!
//! Android: bridges to `FileSaverPlugin` Kotlin class which uses MediaStore on
//! API 29+ and the app-external `Downloads/` folder on older devices.
//!
//! Desktop: writes to `$HOME/Downloads/`; opens via the system handler.

use serde::{Deserialize, Serialize};
use tauri::{
    plugin::{Builder, TauriPlugin},
    AppHandle, Manager, Runtime,
};

#[cfg(mobile)]
use tauri::{plugin::PluginHandle, State};

#[derive(Serialize, Deserialize, Debug)]
pub struct SavePathResponse {
    pub path: String,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct IsSavedResponse {
    pub path: Option<String>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct OkResponse {
    pub ok: bool,
}

#[cfg(mobile)]
struct FileSaverHandle<R: Runtime>(PluginHandle<R>);

#[tauri::command]
pub fn fs_save<R: Runtime>(
    app: AppHandle<R>,
    bytes: Vec<u8>,
    file_name: String,
    attachment_id: String,
    mime: String,
) -> Result<SavePathResponse, String> {
    #[cfg(mobile)]
    {
        use base64::Engine as _;
        let h: State<FileSaverHandle<R>> = app.state();
        let result: serde_json::Value = h
            .0
            .run_mobile_plugin(
                "save",
                serde_json::json!({
                    "bytes": base64::engine::general_purpose::STANDARD.encode(&bytes),
                    "fileName": file_name,
                    "attachmentId": attachment_id,
                    "mime": mime,
                }),
            )
            .map_err(|e| format!("plugin: {e}"))?;
        let path = result
            .get("path")
            .and_then(|v| v.as_str())
            .ok_or_else(|| "no path field".to_string())?
            .to_string();
        Ok(SavePathResponse { path })
    }
    #[cfg(not(mobile))]
    {
        let _ = (app, mime);
        let dl_dir = downloads_dir()?;
        std::fs::create_dir_all(&dl_dir).map_err(|e| format!("create dir: {e}"))?;
        let path = pick_unused_filename(&dl_dir, &file_name);
        std::fs::write(&path, &bytes).map_err(|e| format!("write: {e}"))?;
        save_desktop_index(&attachment_id, &path)?;
        Ok(SavePathResponse {
            path: path.to_string_lossy().into_owned(),
        })
    }
}

#[tauri::command]
pub fn fs_is_saved<R: Runtime>(
    app: AppHandle<R>,
    attachment_id: String,
) -> Result<IsSavedResponse, String> {
    #[cfg(mobile)]
    {
        let h: State<FileSaverHandle<R>> = app.state();
        let result: serde_json::Value = h
            .0
            .run_mobile_plugin(
                "isSaved",
                serde_json::json!({ "attachmentId": attachment_id }),
            )
            .map_err(|e| format!("plugin: {e}"))?;
        let path = result
            .get("path")
            .and_then(|v| v.as_str())
            .map(std::string::ToString::to_string);
        Ok(IsSavedResponse { path })
    }
    #[cfg(not(mobile))]
    {
        let _ = app;
        let path = lookup_desktop_index(&attachment_id);
        Ok(IsSavedResponse { path })
    }
}

#[tauri::command]
pub fn fs_open<R: Runtime>(
    app: AppHandle<R>,
    path: String,
    mime: String,
) -> Result<OkResponse, String> {
    #[cfg(mobile)]
    {
        let h: State<FileSaverHandle<R>> = app.state();
        let _: serde_json::Value = h
            .0
            .run_mobile_plugin(
                "openFile",
                serde_json::json!({ "path": path, "mime": mime }),
            )
            .map_err(|e| format!("plugin: {e}"))?;
        Ok(OkResponse { ok: true })
    }
    #[cfg(not(mobile))]
    {
        let _ = (app, mime);
        #[cfg(target_os = "linux")]
        let prog = "xdg-open";
        #[cfg(target_os = "macos")]
        let prog = "open";
        #[cfg(target_os = "windows")]
        let prog = "explorer";
        std::process::Command::new(prog)
            .arg(&path)
            .spawn()
            .map_err(|e| format!("spawn opener: {e}"))?;
        Ok(OkResponse { ok: true })
    }
}

pub fn init<R: Runtime>() -> TauriPlugin<R> {
    Builder::new("file-saver")
        .invoke_handler(tauri::generate_handler![fs_save, fs_is_saved, fs_open])
        .setup(|_app, _api| {
            #[cfg(mobile)]
            {
                let handle = _api
                    .register_android_plugin("com.example.filesaver", "FileSaverPlugin")
                    .expect("failed to register Android FileSaver plugin");
                _app.manage(FileSaverHandle(handle));
            }
            Ok(())
        })
        .build()
}

// ---------- Desktop helpers ----------

#[cfg(not(mobile))]
fn downloads_dir() -> Result<std::path::PathBuf, String> {
    let home = std::env::var("HOME").map_err(|e| format!("HOME: {e}"))?;
    Ok(std::path::PathBuf::from(home).join("Downloads"))
}

#[cfg(not(mobile))]
fn pick_unused_filename(dir: &std::path::Path, name: &str) -> std::path::PathBuf {
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

#[cfg(not(mobile))]
fn desktop_index_path() -> Result<std::path::PathBuf, String> {
    let home = std::env::var("HOME").map_err(|e| format!("HOME: {e}"))?;
    Ok(std::path::PathBuf::from(home).join(".local/share/messenger-tauri/saved_attachments.json"))
}

#[cfg(not(mobile))]
fn load_desktop_index() -> std::collections::HashMap<String, String> {
    let p = match desktop_index_path() {
        Ok(p) => p,
        Err(_) => return std::collections::HashMap::new(),
    };
    let bytes = match std::fs::read(&p) {
        Ok(b) => b,
        Err(_) => return std::collections::HashMap::new(),
    };
    serde_json::from_slice(&bytes).unwrap_or_default()
}

#[cfg(not(mobile))]
fn save_desktop_index(aid: &str, path: &std::path::Path) -> Result<(), String> {
    let p = desktop_index_path()?;
    if let Some(parent) = p.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let mut idx = load_desktop_index();
    idx.insert(aid.to_string(), path.to_string_lossy().into_owned());
    let json = serde_json::to_vec_pretty(&idx).map_err(|e| e.to_string())?;
    std::fs::write(&p, json).map_err(|e| e.to_string())?;
    Ok(())
}

#[cfg(not(mobile))]
fn lookup_desktop_index(aid: &str) -> Option<String> {
    let idx = load_desktop_index();
    if let Some(path) = idx.get(aid) {
        if std::path::Path::new(path).exists() {
            return Some(path.clone());
        }
    }
    None
}
