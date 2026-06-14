pub mod commands;
pub mod transcription;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_android_keystore::init())
        .plugin(tauri_plugin_file_saver::init())
        .invoke_handler(tauri::generate_handler![
            commands::age_encrypt_bootstrap,
            commands::age_decrypt_bootstrap,
            commands::take_shared_attachments,
            transcription::transcription_list_models,
            transcription::transcription_list_downloaded,
            transcription::transcription_download_model,
            transcription::transcription_delete_model,
            transcription::transcription_get_active,
            transcription::transcription_set_active,
            transcription::transcription_transcribe,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
