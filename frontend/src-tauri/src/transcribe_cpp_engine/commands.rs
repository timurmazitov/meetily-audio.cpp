//! Tauri commands for the transcribe.cpp model manager.
//!
//! These commands only touch files under the app-data models directory and
//! never load the native library, so the model manager works even when
//! `libtranscribe` is absent (and before the FFI layer exists at all).

use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Instant;

use tauri::{command, AppHandle, Emitter, Manager, Runtime};

use super::catalog::{ModelInfo, ModelStatus};
use super::models::{CancelDownloadOutcome, TranscribeCppModelManager};

/// Global transcribe.cpp model manager, created during app setup.
pub static TRANSCRIBECPP_MODEL_MANAGER: Mutex<Option<Arc<TranscribeCppModelManager>>> =
    Mutex::new(None);

/// Shared with Whisper, Parakeet and audio.cpp: `<app_data_dir>/models`.
static MODELS_DIR: Mutex<Option<PathBuf>> = Mutex::new(None);

/// Tauri event names for the transcribe.cpp model manager. The payload shape
/// mirrors the audio.cpp events (`modelName` camelCase, snake_case sizes and
/// speed) so the frontend progress pipelines stay uniform.
pub const DOWNLOAD_PROGRESS_EVENT: &str = "transcribecpp-model-download-progress";
pub const DOWNLOAD_COMPLETE_EVENT: &str = "transcribecpp-model-download-complete";
pub const DOWNLOAD_ERROR_EVENT: &str = "transcribecpp-model-download-error";

/// Remember the app-data models directory and create it when missing.
/// Called during app setup before [`transcribecpp_model_init`].
pub fn set_models_directory<R: Runtime>(app: &AppHandle<R>) {
    let app_data_dir = match app.path().app_data_dir() {
        Ok(dir) => dir,
        Err(error) => {
            log::error!(
                "Failed to get app data dir for transcribe.cpp models: {}",
                error
            );
            return;
        }
    };

    let models_dir = app_data_dir.join("models");
    if !models_dir.exists() {
        if let Err(error) = std::fs::create_dir_all(&models_dir) {
            log::error!(
                "Failed to create transcribe.cpp models directory {}: {}",
                models_dir.display(),
                error
            );
            return;
        }
    }

    log::info!(
        "transcribe.cpp models directory set to: {}",
        models_dir.display()
    );
    let mut guard = MODELS_DIR.lock().unwrap();
    *guard = Some(models_dir);
}

fn get_models_directory() -> Option<PathBuf> {
    MODELS_DIR.lock().unwrap().clone()
}

fn manager() -> Option<Arc<TranscribeCppModelManager>> {
    TRANSCRIBECPP_MODEL_MANAGER
        .lock()
        .unwrap()
        .as_ref()
        .cloned()
}

/// The initialized model manager, for other native modules (the transcription
/// dispatcher resolves the configured model through the same catalog state the
/// settings UI shows).
pub fn model_manager() -> Option<Arc<TranscribeCppModelManager>> {
    manager()
}

/// Initialize the transcribe.cpp model manager. Idempotent: a second call is
/// a no-op so the frontend can safely call it on every settings visit.
/// Does not load the native library.
#[command]
pub async fn transcribecpp_model_init() -> Result<(), String> {
    {
        let guard = TRANSCRIBECPP_MODEL_MANAGER.lock().unwrap();
        if guard.is_some() {
            return Ok(());
        }
    }

    let manager = TranscribeCppModelManager::new(get_models_directory())
        .map_err(|error| format!("Failed to initialize transcribe.cpp model manager: {error}"))?;

    log::info!(
        "transcribe.cpp model manager initialized at {}",
        manager.models_directory().display()
    );
    let mut guard = TRANSCRIBECPP_MODEL_MANAGER.lock().unwrap();
    *guard = Some(Arc::new(manager));
    Ok(())
}

/// Catalogued transcribe.cpp models with their current installation state.
#[command]
pub async fn transcribecpp_get_available_models() -> Result<Vec<ModelInfo>, String> {
    let manager = manager()
        .ok_or_else(|| "transcribe.cpp model manager not initialized".to_string())?;
    manager
        .discover_models()
        .await
        .map_err(|error| format!("Failed to discover transcribe.cpp models: {error}"))
}

/// Whether at least one catalogued model is installed.
#[command]
pub async fn transcribecpp_has_available_models() -> Result<bool, String> {
    let manager = manager()
        .ok_or_else(|| "transcribe.cpp model manager not initialized".to_string())?;
    let models = manager
        .discover_models()
        .await
        .map_err(|error| format!("Failed to discover transcribe.cpp models: {error}"))?;
    Ok(models
        .iter()
        .any(|model| matches!(model.status, ModelStatus::Available)))
}

/// Whether at least one installed model can transcribe.
///
/// The transcribe.cpp catalog is ASR-only, so this matches
/// [`transcribecpp_has_available_models`] today; it exists as the recording
/// readiness gate so the dispatcher call site mirrors audio.cpp and can
/// tighten later without a frontend rename.
#[command]
pub async fn transcribecpp_has_transcription_model() -> Result<bool, String> {
    let manager = manager()
        .ok_or_else(|| "transcribe.cpp model manager not initialized".to_string())?;
    let models = manager
        .discover_models()
        .await
        .map_err(|error| format!("Failed to discover transcribe.cpp models: {error}"))?;
    Ok(models
        .iter()
        .any(|model| matches!(model.status, ModelStatus::Available)))
}

/// The directory holding the transcribe.cpp artifacts.
#[command]
pub async fn transcribecpp_get_models_directory() -> Result<String, String> {
    let manager = manager()
        .ok_or_else(|| "transcribe.cpp model manager not initialized".to_string())?;
    Ok(manager.models_directory().to_string_lossy().to_string())
}

/// Rolling download-rate estimator for progress events.
///
/// The shared downloader reports whole percentages only, so the rate is the
/// percent delta since the previous report scaled by the catalogued model size
/// and divided by the elapsed time. The first report has no rate.
struct DownloadSpeed {
    total_mb: f64,
    last_percent: u8,
    last_report: Instant,
}

impl DownloadSpeed {
    fn new(total_mb: f64, started: Instant) -> Self {
        DownloadSpeed {
            total_mb,
            last_percent: 0,
            last_report: started,
        }
    }

    /// Record a report and return the rate in MiB/s since the previous one.
    fn observe(&mut self, percent: u8, now: Instant) -> f64 {
        let elapsed = now
            .saturating_duration_since(self.last_report)
            .as_secs_f64();
        let delta_percent = percent.saturating_sub(self.last_percent);
        self.last_percent = self.last_percent.max(percent);
        self.last_report = now;

        if elapsed <= 0.0 || delta_percent == 0 {
            return 0.0;
        }

        (f64::from(delta_percent) / 100.0 * self.total_mb) / elapsed
    }
}

/// Payload of [`DOWNLOAD_PROGRESS_EVENT`]. Field names and the
/// `downloading`/`completed` status mirror the audio.cpp, Parakeet and
/// builtin-ai events so the frontend progress pipelines stay uniform.
fn download_progress_payload(
    model_name: &str,
    progress: u8,
    total_mb: f64,
    speed_mbps: f64,
) -> serde_json::Value {
    serde_json::json!({
        "modelName": model_name,
        "progress": progress,
        "downloaded_mb": f64::from(progress) / 100.0 * total_mb,
        "total_mb": total_mb,
        "speed_mbps": speed_mbps,
        "status": if progress == 100 { "completed" } else { "downloading" },
    })
}

/// Download a catalogued model from its pinned upstream revision, emitting
/// progress and terminal events. Cancellation resolves `Ok(())` and is
/// reported by the cancel command, matching the other providers.
#[command]
pub async fn transcribecpp_download_model<R: Runtime>(
    app_handle: AppHandle<R>,
    model_name: String,
) -> Result<(), String> {
    let manager = manager()
        .ok_or_else(|| "transcribe.cpp model manager not initialized".to_string())?;

    // An unknown model is rejected by the manager, which owns the error
    // event; its progress payload only needs a size for catalogued models.
    let total_mb = super::catalog::find_model(&model_name).map_or(0.0, |spec| f64::from(spec.size_mb));

    let app_for_progress = app_handle.clone();
    let progress_model = model_name.clone();
    // Owned by the progress closure; `Mutex` supplies the interior mutability
    // a `Fn` closure needs, so no `Arc` is required.
    let speed = Mutex::new(DownloadSpeed::new(total_mb, Instant::now()));
    let progress: Option<Box<dyn Fn(u8) + Send>> = Some(Box::new(move |reported| {
        let speed_mbps = speed
            .lock()
            .map_or(0.0, |mut speed| speed.observe(reported, Instant::now()));
        if let Err(error) = app_for_progress.emit(
            DOWNLOAD_PROGRESS_EVENT,
            download_progress_payload(&progress_model, reported, total_mb, speed_mbps),
        ) {
            log::error!(
                "Failed to emit transcribe.cpp download progress event: {}",
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
                    "Failed to emit transcribe.cpp download complete event: {}",
                    error
                );
            }
            Ok(())
        }
        Err(error) if super::models::is_download_cancelled(&error) => {
            // Cancellation is owned by the cancel command; do not emit a
            // terminal event that would overwrite its state.
            log::info!("transcribe.cpp model download cancelled: {}", model_name);
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
                    "Failed to emit transcribe.cpp download error event: {}",
                    emit_error
                );
            }
            Err(format!("Failed to download transcribe.cpp model: {error}"))
        }
    }
}

/// Cancel an ongoing transcribe.cpp model download.
#[command]
pub async fn transcribecpp_cancel_download(
    model_name: String,
) -> Result<CancelDownloadOutcome, String> {
    let manager = manager()
        .ok_or_else(|| "transcribe.cpp model manager not initialized".to_string())?;
    manager
        .cancel_download(&model_name)
        .await
        .map_err(|error| format!("Failed to cancel transcribe.cpp download: {error}"))
}

/// Delete a downloaded transcribe.cpp model to free disk space.
#[command]
pub async fn transcribecpp_delete_model(model_name: String) -> Result<String, String> {
    let manager = manager()
        .ok_or_else(|| "transcribe.cpp model manager not initialized".to_string())?;
    manager
        .delete_model(&model_name)
        .await
        .map_err(|error| format!("Failed to delete transcribe.cpp model: {error}"))
}

/// Open the transcribe.cpp models folder in the system file explorer.
#[command]
pub async fn open_transcribecpp_models_folder() -> Result<(), String> {
    let manager = manager()
        .ok_or_else(|| "transcribe.cpp model manager not initialized".to_string())?;
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

    log::info!("Opened transcribe.cpp models folder: {}", folder_path);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Payload field comparison tolerance: the frontend shows one decimal, so
    /// more precision than this is noise.
    fn assert_mb(actual: f64, expected: f64) {
        assert!(
            (actual - expected).abs() < 0.01,
            "expected ~{expected} MiB, got {actual}"
        );
    }

    #[test]
    fn progress_payload_carries_megabytes_and_speed() {
        let payload = download_progress_payload("parakeet-ultra-Q8_0", 42, 706.0, 1.5);

        assert_eq!(payload["modelName"], "parakeet-ultra-Q8_0");
        assert_eq!(payload["progress"], 42);
        assert_eq!(payload["status"], "downloading");
        assert_mb(payload["total_mb"].as_f64().unwrap(), 706.0);
        assert_mb(payload["downloaded_mb"].as_f64().unwrap(), 296.52);
        assert_mb(payload["speed_mbps"].as_f64().unwrap(), 1.5);
    }

    #[test]
    fn progress_payload_marks_completed_at_full_progress() {
        let payload = download_progress_payload("parakeet-ultra-Q8_0", 100, 706.0, 2.0);

        assert_eq!(payload["status"], "completed");
        assert_mb(payload["downloaded_mb"].as_f64().unwrap(), 706.0);
    }

    #[test]
    fn first_progress_report_has_no_speed_yet() {
        let started = std::time::Instant::now();
        let mut speed = DownloadSpeed::new(706.0, started);

        assert_eq!(speed.observe(0, started), 0.0);
    }

    #[test]
    fn speed_is_the_downloaded_rate_between_reports() {
        let started = std::time::Instant::now();
        let mut speed = DownloadSpeed::new(100.0, started);

        // 25% of 100 MiB in two seconds.
        let first = speed.observe(25, started + std::time::Duration::from_secs(2));
        // Another 25 MiB over the next two seconds keeps the same rate.
        let second = speed.observe(50, started + std::time::Duration::from_secs(4));

        assert!((first - 12.5).abs() < 0.001, "first: {first}");
        assert!((second - 12.5).abs() < 0.001, "second: {second}");
    }

    #[test]
    fn event_names_are_prefixed_and_unique() {
        assert_eq!(
            DOWNLOAD_PROGRESS_EVENT,
            "transcribecpp-model-download-progress"
        );
        assert_eq!(
            DOWNLOAD_COMPLETE_EVENT,
            "transcribecpp-model-download-complete"
        );
        assert_eq!(DOWNLOAD_ERROR_EVENT, "transcribecpp-model-download-error");

        let mut names = vec![
            DOWNLOAD_PROGRESS_EVENT,
            DOWNLOAD_COMPLETE_EVENT,
            DOWNLOAD_ERROR_EVENT,
        ];
        names.sort_unstable();
        let unique = names.len();
        names.dedup();
        assert_eq!(names.len(), unique);
    }
}
