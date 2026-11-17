//! Transcript export: native save dialog plus file write.

use log::info;
use tauri::{AppHandle, Runtime};
use tauri_plugin_dialog::DialogExt;

/// Open a native save dialog and write the transcript text to the selected
/// file as UTF-8 (no BOM). Returns the chosen file path, or None when the
/// user cancels the dialog.
#[tauri::command]
pub async fn export_transcript_file_command<R: Runtime>(
    app: AppHandle<R>,
    content: String,
    default_file_name: String,
) -> Result<Option<String>, String> {
    info!("Opening save dialog for transcript export");

    let app_clone = app.clone();
    let file_path = tokio::task::spawn_blocking(move || {
        app_clone
            .dialog()
            .file()
            .set_file_name(&default_file_name)
            .add_filter("Text Files", &["txt"])
            .blocking_save_file()
    })
    .await
    .map_err(|e| format!("Save dialog task failed: {}", e))?;

    match file_path {
        Some(path) => {
            let path_str = path.to_string();
            std::fs::write(&path_str, content.as_bytes())
                .map_err(|e| format!("Failed to write transcript file: {}", e))?;
            info!("Transcript exported to {}", path_str);
            Ok(Some(path_str))
        }
        None => {
            info!("User cancelled transcript export");
            Ok(None)
        }
    }
}
