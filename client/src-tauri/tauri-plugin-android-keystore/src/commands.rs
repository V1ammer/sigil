use serde::Serialize;
use tauri::{AppHandle, Runtime};

#[derive(Serialize)]
pub struct KeystoreResponse {
    pub ok: bool,
}

#[derive(Serialize)]
pub struct KeystoreGetResponse {
    pub value: Option<String>,
}

/// Store a value in the Android keystore.
#[tauri::command]
pub fn set<R: Runtime>(
    app: AppHandle<R>,
    key: String,
    value: String,
) -> Result<KeystoreResponse, String> {
    #[cfg(mobile)]
    {
        let keystore = app.state::<mobile::KeystoreMobile<R>>();
        keystore.set(&key, value.as_bytes())
    }
    #[cfg(not(mobile))]
    {
        // Desktop fallback: store in a simple in-memory map or just return ok
        let _ = (app, key, value);
        Ok(KeystoreResponse { ok: true })
    }
}

/// Retrieve a value from the Android keystore.
#[tauri::command]
pub fn get<R: Runtime>(
    app: AppHandle<R>,
    key: String,
) -> Result<KeystoreGetResponse, String> {
    #[cfg(mobile)]
    {
        let keystore = app.state::<mobile::KeystoreMobile<R>>();
        keystore.get(&key).map(|opt| {
            use base64::engine::general_purpose::STANDARD as BASE64;
            use base64::Engine;
            KeystoreGetResponse {
                value: opt.map(|v| BASE64.encode(v)),
            }
        })
    }
    #[cfg(not(mobile))]
    {
        let _ = (app, key);
        Ok(KeystoreGetResponse { value: None })
    }
}

/// Delete a value from the Android keystore.
#[tauri::command]
pub fn delete<R: Runtime>(
    app: AppHandle<R>,
    key: String,
) -> Result<KeystoreResponse, String> {
    #[cfg(mobile)]
    {
        let keystore = app.state::<mobile::KeystoreMobile<R>>();
        keystore.delete(&key)
    }
    #[cfg(not(mobile))]
    {
        let _ = (app, key);
        Ok(KeystoreResponse { ok: true })
    }
}
