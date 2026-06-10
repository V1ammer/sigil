pub mod commands;
pub mod file_saver;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_android_keystore::init())
        .plugin(file_saver::init())
        .invoke_handler(tauri::generate_handler![
            commands::age_encrypt_bootstrap,
            commands::age_decrypt_bootstrap,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
