pub use commands::*;
use tauri::{
    plugin::{Builder, TauriPlugin},
    Manager, Runtime,
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
        .setup(|app, api| {
            #[cfg(mobile)]
            {
                // Register the Kotlin plugin class so its @Command methods are available.
                let handle = api
                    .register_android_plugin(
                        "com.example.keystore",
                        "KeystorePlugin",
                    )
                    .expect("failed to register Android keystore plugin");
                let keystore = mobile::KeystoreMobile::new(handle);
                app.manage(keystore);
            }
            #[cfg(not(mobile))]
            {
                let _ = (app, api);
            }
            Ok(())
        })
        .build()
}
