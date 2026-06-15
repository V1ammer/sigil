pub mod commands;
pub mod stream;
pub mod transcription;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_android_keystore::init())
        .plugin(tauri_plugin_file_saver::init())
        .manage(stream::StreamState::default())
        .register_asynchronous_uri_scheme_protocol("stream", |ctx, request, responder| {
            stream::handle(ctx, request, responder);
        })
        .invoke_handler(tauri::generate_handler![
            commands::age_encrypt_bootstrap,
            commands::age_decrypt_bootstrap,
            commands::take_shared_attachments,
            stream::stream_prepare,
            transcription::transcription_list_models,
            transcription::transcription_list_downloaded,
            transcription::transcription_download_model,
            transcription::transcription_download_progress,
            transcription::transcription_delete_model,
            transcription::transcription_get_active,
            transcription::transcription_set_active,
            transcription::transcription_transcribe,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
