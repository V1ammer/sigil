//! File saver — write attachment bytes to Downloads, lookup by attachment id,
//! open with the system viewer.
//!
//! Android: bridges to the `FileSaverPlugin` Kotlin class which uses MediaStore
//! on API 29+ and the app-external `Downloads/` folder on older devices.
//!
//! Desktop: writes to `$HOME/Downloads/`; opens via the system handler.

use tauri::{
    plugin::{Builder, TauriPlugin},
    Manager, Runtime,
};

mod commands;
#[cfg(not(mobile))]
mod desktop;

#[cfg(mobile)]
use tauri::plugin::PluginHandle;

#[cfg(mobile)]
pub(crate) struct FileSaverHandle<R: Runtime>(pub PluginHandle<R>);

pub fn init<R: Runtime>() -> TauriPlugin<R> {
    Builder::new("file-saver")
        .invoke_handler(tauri::generate_handler![
            commands::fs_save,
            commands::fs_is_saved,
            commands::fs_open,
        ])
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
