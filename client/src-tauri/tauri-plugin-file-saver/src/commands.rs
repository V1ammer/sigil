//! Tauri command handlers — Android delegates to the Kotlin plugin via
//! `PluginHandle::run_mobile_plugin`; desktop uses `std::fs`.

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Runtime};

#[cfg(mobile)]
use tauri::{Manager, State};

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
        let h: State<crate::FileSaverHandle<R>> = app.state();
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
        let dir = crate::desktop::downloads_dir()?;
        std::fs::create_dir_all(&dir).map_err(|e| format!("create dir: {e}"))?;
        let path = crate::desktop::pick_unused_filename(&dir, &file_name);
        std::fs::write(&path, &bytes).map_err(|e| format!("write: {e}"))?;
        crate::desktop::save_index(&attachment_id, &path)?;
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
        let h: State<crate::FileSaverHandle<R>> = app.state();
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
        let path = crate::desktop::lookup_index(&attachment_id);
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
        let h: State<crate::FileSaverHandle<R>> = app.state();
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
