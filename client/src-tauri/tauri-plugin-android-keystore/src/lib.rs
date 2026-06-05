pub use commands::*;
use tauri::{
    AppHandle,
    plugin::{Builder, TauriPlugin},
    Runtime,
};

mod commands;
#[cfg(mobile)]
mod mobile;
#[cfg(desktop)]
mod desktop;

/// Initialize the keystore plugin.
pub fn init<R: Runtime>() -> TauriPlugin<R> {
    Builder::new("android-keystore")
        .invoke_handler(tauri::generate_handler![
            commands::set,
            commands::get,
            commands::delete,
        ])
        .setup(|app: &AppHandle<R>, _api| {
            #[cfg(mobile)]
            {
                let keystore = mobile::KeystoreMobile::new();
                app.manage(keystore);
            }
            Ok(())
        })
        .build()
}
