//! Tauri commands for the audio.cpp engine module.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use serde::Serialize;
use tauri::{command, AppHandle, Emitter, Manager, Runtime};

use super::error::AudioCppError;
use super::models::{AudioCppModelManager, CancelDownloadOutcome};
use super::wrapper::AudioCpp;
use crate::audio_cpp_engine::{ModelInfo, ModelStatus};

/// Diagnostic report on the audio.cpp native library: whether it loaded,
/// its ABI/build version, and the model families and tasks it supports.
#[derive(Debug, Clone, Serialize)]
pub struct AudioCppDiagnostics {
    pub available: bool,
    pub library_path: Option<PathBuf>,
    pub abi_version: Option<String>,
    pub build_version: Option<String>,
    pub families: Vec<String>,
    pub tasks: Vec<String>,
    pub error: Option<String>,
}

impl AudioCppDiagnostics {
    /// Report for a library that could not be loaded or interrogated; the
    /// error text explains why in user-facing terms.
    pub fn unavailable(error: &AudioCppError) -> Self {
        AudioCppDiagnostics {
            available: false,
            library_path: None,
            abi_version: None,
            build_version: None,
            families: Vec::new(),
            tasks: Vec::new(),
            error: Some(error.to_string()),
        }
    }

    fn from_engine(engine: &AudioCpp) -> Self {
        match engine.create_registry() {
            Ok(registry) => AudioCppDiagnostics {
                available: true,
                library_path: Some(engine.path().to_path_buf()),
                abi_version: Some(engine.abi_version().to_string()),
                build_version: Some(engine.build_version()),
                families: registry.families(),
                tasks: engine.supported_tasks(),
                error: None,
            },
            Err(error) => {
                // The library itself loaded and passed the ABI gate; keep
                // that visible even though the registry query failed.
                let mut report = AudioCppDiagnostics::unavailable(&error);
                report.library_path = Some(engine.path().to_path_buf());
                report.abi_version = Some(engine.abi_version().to_string());
                report.build_version = Some(engine.build_version());
                report
            }
        }
    }
}

/// Diagnostic command: loads the audio.cpp library (if present) and
/// reports its ABI version and supported model families. A missing or
/// ABI-incompatible library is reported in-band (`available: false` with
/// an explanatory `error`), never as a command failure or a crash.
#[command]
pub async fn audiocpp_diagnostics() -> Result<AudioCppDiagnostics, String> {
    // Library loading and registry queries block; keep them off the
    // async runtime (docs/research/audio-cpp-c-api.md).
    tokio::task::spawn_blocking(collect_diagnostics)
        .await
        .map_err(|e| format!("audio.cpp diagnostics task failed: {e}"))
}

fn collect_diagnostics() -> AudioCppDiagnostics {
    match AudioCpp::load() {
        Ok(engine) => AudioCppDiagnostics::from_engine(&engine),
        Err(error) => AudioCppDiagnostics::unavailable(&error),
    }
}

// Model management commands.
//
// They only touch files under the app-data models directory and never load
// the native library, so they work even when `libaudiocpp` is absent.

/// Global audio.cpp model manager, created during app setup.
pub static AUDIOCPP_MODEL_MANAGER: Mutex<Option<Arc<AudioCppModelManager>>> = Mutex::new(None);

/// Shared with Whisper and Parakeet: `<app_data_dir>/models`.
static MODELS_DIR: Mutex<Option<PathBuf>> = Mutex::new(None);

/// Tauri event names for the audio.cpp model manager.
pub const DOWNLOAD_PROGRESS_EVENT: &str = "audiocpp-model-download-progress";
pub const DOWNLOAD_COMPLETE_EVENT: &str = "audiocpp-model-download-complete";
pub const DOWNLOAD_ERROR_EVENT: &str = "audiocpp-model-download-error";

/// Remember the app-data models directory and create it when missing.
/// Called during app setup before [`audiocpp_model_init`].
pub fn set_models_directory<R: Runtime>(app: &AppHandle<R>) {
    let app_data_dir = match app.path().app_data_dir() {
        Ok(dir) => dir,
        Err(error) => {
            log::error!("Failed to get app data dir for audio.cpp models: {}", error);
            return;
        }
    };

    let models_dir = app_data_dir.join("models");
    if !models_dir.exists() {
        if let Err(error) = std::fs::create_dir_all(&models_dir) {
            log::error!(
                "Failed to create audio.cpp models directory {}: {}",
                models_dir.display(),
                error
            );
            return;
        }
    }

    log::info!(
        "audio.cpp models directory set to: {}",
        models_dir.display()
    );
    let mut guard = MODELS_DIR.lock().unwrap();
    *guard = Some(models_dir);
}

fn get_models_directory() -> Option<PathBuf> {
    MODELS_DIR.lock().unwrap().clone()
}

fn manager() -> Option<Arc<AudioCppModelManager>> {
    AUDIOCPP_MODEL_MANAGER.lock().unwrap().as_ref().cloned()
}

/// The initialized model manager, for other native modules (the transcription
/// dispatcher resolves the configured model through the same catalog state the
/// settings UI shows).
pub fn model_manager() -> Option<Arc<AudioCppModelManager>> {
    manager()
}

/// Initialize the audio.cpp model manager. Idempotent: a second call is a
/// no-op so the frontend can safely call it on every settings visit.
#[command]
pub async fn audiocpp_model_init() -> Result<(), String> {
    {
        let guard = AUDIOCPP_MODEL_MANAGER.lock().unwrap();
        if guard.is_some() {
            return Ok(());
        }
    }

    let manager = AudioCppModelManager::new(get_models_directory())
        .map_err(|error| format!("Failed to initialize audio.cpp model manager: {error}"))?;

    log::info!(
        "audio.cpp model manager initialized at {}",
        manager.models_directory().display()
    );
    let mut guard = AUDIOCPP_MODEL_MANAGER.lock().unwrap();
    *guard = Some(Arc::new(manager));
    Ok(())
}

/// Catalogued audio.cpp models with their current installation state.
#[command]
pub async fn audiocpp_get_available_models() -> Result<Vec<ModelInfo>, String> {
    let manager = manager().ok_or_else(|| "audio.cpp model manager not initialized".to_string())?;
    manager
        .discover_models()
        .await
        .map_err(|error| format!("Failed to discover audio.cpp models: {error}"))
}

/// Whether at least one catalogued model is installed.
#[command]
pub async fn audiocpp_has_available_models() -> Result<bool, String> {
    let manager = manager().ok_or_else(|| "audio.cpp model manager not initialized".to_string())?;
    let models = manager
        .discover_models()
        .await
        .map_err(|error| format!("Failed to discover audio.cpp models: {error}"))?;
    Ok(models
        .iter()
        .any(|model| matches!(model.status, ModelStatus::Available)))
}

/// Whether at least one installed model can transcribe.
///
/// Readiness for recording is stricter than "a model is downloaded": a
/// downloaded diarization model cannot produce a transcript, so the recording
/// gate must not accept it.
#[command]
pub async fn audiocpp_has_transcription_model() -> Result<bool, String> {
    let manager = manager().ok_or_else(|| "audio.cpp model manager not initialized".to_string())?;
    let models = manager
        .discover_models()
        .await
        .map_err(|error| format!("Failed to discover audio.cpp models: {error}"))?;
    Ok(models.iter().any(|model| {
        model.task == crate::audio_cpp_engine::ModelTask::Asr
            && matches!(model.status, ModelStatus::Available)
    }))
}

/// The directory holding the audio.cpp artifacts.
#[command]
pub async fn audiocpp_get_models_directory() -> Result<String, String> {
    let manager = manager().ok_or_else(|| "audio.cpp model manager not initialized".to_string())?;
    Ok(manager.models_directory().to_string_lossy().to_string())
}

/// Download a catalogued model from its pinned upstream revision, emitting
/// progress and terminal events. Cancellation resolves `Ok(())` and is
/// reported by the cancel command, matching the other providers.
#[command]
pub async fn audiocpp_download_model<R: Runtime>(
    app_handle: AppHandle<R>,
    model_name: String,
) -> Result<(), String> {
    let manager = manager().ok_or_else(|| "audio.cpp model manager not initialized".to_string())?;

    let app_for_progress = app_handle.clone();
    let progress_model = model_name.clone();
    let progress: Option<Box<dyn Fn(u8) + Send>> = Some(Box::new(move |reported| {
        if let Err(error) = app_for_progress.emit(
            DOWNLOAD_PROGRESS_EVENT,
            serde_json::json!({
                "modelName": progress_model,
                "progress": reported,
                "status": if reported == 100 { "completed" } else { "downloading" }
            }),
        ) {
            log::error!(
                "Failed to emit audio.cpp download progress event: {}",
                error
            );
        }
    }));

    match manager.download_model(&model_name, progress).await {
        Ok(()) => {
            if let Err(error) = app_handle.emit(
                DOWNLOAD_COMPLETE_EVENT,
                serde_json::json!({ "modelName": model_name }),
            ) {
                log::error!(
                    "Failed to emit audio.cpp download complete event: {}",
                    error
                );
            }
            Ok(())
        }
        Err(error) if crate::audio_cpp_engine::models::is_download_cancelled(&error) => {
            // Cancellation is owned by the cancel command; do not emit a
            // terminal event that would overwrite its state.
            log::info!("audio.cpp model download cancelled: {}", model_name);
            Ok(())
        }
        Err(error) => {
            if let Err(emit_error) = app_handle.emit(
                DOWNLOAD_ERROR_EVENT,
                serde_json::json!({
                    "modelName": model_name,
                    "error": error.to_string()
                }),
            ) {
                log::error!(
                    "Failed to emit audio.cpp download error event: {}",
                    emit_error
                );
            }
            Err(format!("Failed to download audio.cpp model: {error}"))
        }
    }
}

/// Cancel an ongoing audio.cpp model download.
#[command]
pub async fn audiocpp_cancel_download(model_name: String) -> Result<CancelDownloadOutcome, String> {
    let manager = manager().ok_or_else(|| "audio.cpp model manager not initialized".to_string())?;
    manager
        .cancel_download(&model_name)
        .await
        .map_err(|error| format!("Failed to cancel audio.cpp download: {error}"))
}

/// Delete a downloaded audio.cpp model to free disk space.
#[command]
pub async fn audiocpp_delete_model(model_name: String) -> Result<String, String> {
    let manager = manager().ok_or_else(|| "audio.cpp model manager not initialized".to_string())?;
    manager
        .delete_model(&model_name)
        .await
        .map_err(|error| format!("Failed to delete audio.cpp model: {error}"))
}

/// Open the audio.cpp models folder in the system file explorer.
#[command]
pub async fn open_audiocpp_models_folder() -> Result<(), String> {
    let manager = manager().ok_or_else(|| "audio.cpp model manager not initialized".to_string())?;
    let folder = manager.models_directory().to_path_buf();
    if !folder.exists() {
        std::fs::create_dir_all(&folder)
            .map_err(|error| format!("Failed to create directory: {error}"))?;
    }
    let folder_path = folder.to_string_lossy().to_string();

    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("explorer")
            .arg(&folder_path)
            .spawn()
            .map_err(|error| format!("Failed to open folder: {error}"))?;
    }

    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg(&folder_path)
            .spawn()
            .map_err(|error| format!("Failed to open folder: {error}"))?;
    }

    #[cfg(target_os = "linux")]
    {
        std::process::Command::new("xdg-open")
            .arg(&folder_path)
            .spawn()
            .map_err(|error| format!("Failed to open folder: {error}"))?;
    }

    log::info!("Opened audio.cpp models folder: {}", folder_path);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unavailable_report_carries_error_text() {
        let error = AudioCppError::LibraryNotFound {
            searched_paths: vec![PathBuf::from("/opt/meetily/libaudiocpp.so")],
        };
        let report = AudioCppDiagnostics::unavailable(&error);
        assert!(!report.available);
        assert!(report.library_path.is_none());
        assert!(report.abi_version.is_none());
        assert!(report.families.is_empty());
        assert!(report.tasks.is_empty());
        let text = report.error.expect("error text");
        assert!(text.contains("not found"), "text: {text}");
    }
}
