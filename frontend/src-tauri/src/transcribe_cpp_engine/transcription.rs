//! App-level wiring of the transcribeCpp transcription provider: model
//! selection from the catalog, the global provider handle the transcription
//! dispatcher uses, and the native loading path.
//!
//! The model *selection* rules are pure functions over catalog state, so they
//! are tested without a native library or a Tauri app handle. Only the
//! [`load_native_provider`] path touches the native FFI layer, and the global
//! handle mirrors `audio_cpp_engine::transcription`.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use serde::Serialize;

use super::backend::ComputeMode;
use super::catalog::{ModelInfo, ModelStatus};
use super::engine::NativeTranscribeCppEngine;
use super::error::TranscribeCppError;
use super::wrapper::TranscribeCpp;
use crate::audio::transcription::transcribe_cpp_provider::{
    ProviderLoadError, TranscribeCppProvider,
};

/// The loaded provider, shared by the transcription dispatcher. Mirrors
/// `AUDIOCPP_PROVIDER`: one process-wide model, reloaded only when the
/// requested model differs.
pub static TRANSCRIBECPP_PROVIDER: Mutex<Option<Arc<TranscribeCppProvider>>> = Mutex::new(None);

/// The currently loaded provider, if any.
pub fn provider() -> Option<Arc<TranscribeCppProvider>> {
    TRANSCRIBECPP_PROVIDER
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .as_ref()
        .cloned()
}

/// Replace the global provider.
pub fn set_provider(provider: TranscribeCppProvider) -> Arc<TranscribeCppProvider> {
    let provider = Arc::new(provider);
    let mut guard = TRANSCRIBECPP_PROVIDER
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    *guard = Some(Arc::clone(&provider));
    provider
}

/// Drop the global provider, releasing the model and its session thread.
pub fn unload_provider() {
    let mut guard = TRANSCRIBECPP_PROVIDER
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if let Some(provider) = guard.take() {
        provider.unload();
    }
}

/// Name of the development staging directory the transcribe-native staging
/// script fills next to `src-tauri` (gitignored; ticket 01). Windows bundles
/// the same directory into the install root via the Tauri resource map, so
/// the flat layout finds it there at runtime; on Linux dev builds the test/
/// dev binary runs from the target directory, so probe it explicitly.
#[cfg(all(target_os = "linux", debug_assertions))]
const DEV_RUNTIME_DIR: &str = "transcribecpp-runtime";

/// Resolves the native library through the wrapper discovery order
/// (env override → `transcribe-libs/<lane>/` → flat next to exe) and, on
/// Linux debug builds, additionally probes the repository-local staging
/// directory next to the crate root. An env override that points at a
/// missing file stays an error; only the not-found search falls back.
///
/// Blocking: gate + dlopen. Call from `tokio::task::spawn_blocking`.
pub fn resolve_library_path() -> Result<PathBuf, TranscribeCppError> {
    match super::wrapper::resolve_library_path() {
        Ok(path) => Ok(path),
        #[cfg(all(target_os = "linux", debug_assertions))]
        Err(primary_error @ TranscribeCppError::LibraryNotFound { .. }) => {
            let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
            let staged = manifest_dir
                .join(DEV_RUNTIME_DIR)
                .join(super::wrapper::LIBRARY_FILE_NAMES[0]);
            if staged.is_file() {
                log::info!(
                    "transcribe.cpp library not found next to the executable; using the staged runtime {}",
                    staged.display()
                );
                return Ok(staged);
            }
            Err(primary_error)
        }
        Err(error) => Err(error),
    }
}

/// Loads the native transcribe.cpp library into an engine on `mode`.
///
/// This is the single place that constructs the engine, shared by every
/// provider load. Ticket 06 replaces the hard-coded [`ComputeMode::Auto`]
/// with the user's stored selection.
///
/// Blocking: the dynamic library load, ABI gate and backend registration
/// all block. From async code call this inside `tokio::task::spawn_blocking`.
pub fn load_native_engine(
    mode: ComputeMode,
) -> Result<Arc<dyn super::engine::TranscribeCppEngine>, TranscribeCppError> {
    let path = resolve_library_path()?;
    log::info!("loading transcribe.cpp library from {}", path.display());
    let library = TranscribeCpp::load_from(&path)?;
    log::info!(
        "transcribe.cpp runtime {} (backends: {})",
        library.version(),
        library.contract().backends.join(", ")
    );
    Ok(Arc::new(NativeTranscribeCppEngine::new(library, mode)?))
}

/// Loads the native library and the selected model into a provider on the
/// Auto compute mode (ticket 06 adds the user selection).
///
/// Blocking: the library load and model load are slow. From async code call
/// this inside `tokio::task::spawn_blocking`.
pub fn load_native_provider(
    model: &ModelInfo,
) -> Result<TranscribeCppProvider, ProviderLoadError> {
    let engine = load_native_engine(ComputeMode::Auto)
        .map_err(|error| ProviderLoadError::Engine(error.to_string()))?;
    TranscribeCppProvider::load_blocking(engine, model.name.clone(), &model.path)
}

/// Why a configured transcribe.cpp model cannot transcribe right now.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub enum ModelNotReady {
    /// No catalogued model is installed.
    NoModelInstalled,
    /// The requested model is not in the catalog at all (e.g. it was removed
    /// between listing and running a batch job).
    NotFound { model: String },
    /// The artifact is absent, downloading, corrupted or errored.
    Status { model: String, status: ModelStatus },
}

impl std::fmt::Display for ModelNotReady {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NoModelInstalled => write!(
                f,
                "No Transcribe.cpp model is installed. Download Parakeet Ultra in Settings → Transcript Model → Transcribe.cpp models."
            ),
            Self::NotFound { model } => write!(
                f,
                "Transcribe.cpp model '{}' is not available. Download Parakeet Ultra in Settings → Transcript Model → Transcribe.cpp models.",
                model
            ),
            Self::Status { model, status } => match status {
                ModelStatus::Missing => write!(
                    f,
                    "Transcribe.cpp model '{}' is not downloaded. Download it in Settings → Transcript Model → Transcribe.cpp models.",
                    model
                ),
                ModelStatus::Downloading { progress } => write!(
                    f,
                    "Transcribe.cpp model '{}' is still downloading ({}%). Please wait for it to finish.",
                    model, progress
                ),
                ModelStatus::Corrupted { .. } => write!(
                    f,
                    "Transcribe.cpp model '{}' is corrupted. Delete it and download it again in Settings.",
                    model
                ),
                ModelStatus::Error(detail) => write!(
                    f,
                    "Transcribe.cpp model '{}' has an error: {}. Download it again in Settings.",
                    model, detail
                ),
                ModelStatus::Available => Ok(()),
            },
        }
    }
}

impl std::error::Error for ModelNotReady {}

/// Picks the catalogued model to transcribe with.
///
/// The configured model wins when it is a catalogued, installed model.
/// Otherwise the first installed model is used, so a deleted or renamed model
/// does not block recording. Missing, downloading and corrupted artifacts are
/// reported instead of being loaded.
pub fn select_transcription_model(
    models: &[ModelInfo],
    configured: &str,
) -> Result<ModelInfo, ModelNotReady> {
    if let Some(model) = models.iter().find(|model| model.name == configured) {
        return ensure_ready(model);
    }

    if let Some(installed) = models
        .iter()
        .find(|model| matches!(model.status, ModelStatus::Available))
    {
        log::info!(
            "transcribe.cpp model '{}' is unavailable; using installed model '{}'",
            configured,
            installed.name
        );
        return Ok((*installed).clone());
    }

    match models.first() {
        Some(model) => ensure_ready(model),
        None => Err(ModelNotReady::NoModelInstalled),
    }
}

fn ensure_ready(model: &ModelInfo) -> Result<ModelInfo, ModelNotReady> {
    match model.status {
        ModelStatus::Available => Ok(model.clone()),
        ref status => Err(ModelNotReady::Status {
            model: model.name.clone(),
            status: status.clone(),
        }),
    }
}

/// Picks one exact catalogued model for a batch job (retranscription or
/// import), where the user chose the model in the dialog.
///
/// Unlike [`select_transcription_model`] there is no fallback to any
/// installed model: silently transcribing with a different model than the
/// one selected would defeat the comparison the user is making. A model that
/// is not in the catalog or is not fully installed is an error, mirroring the
/// Whisper/Parakeet batch paths.
pub fn select_requested_transcription_model(
    models: &[ModelInfo],
    requested: &str,
) -> Result<ModelInfo, ModelNotReady> {
    match models.iter().find(|model| model.name == requested) {
        Some(model) => ensure_ready(model),
        None => Err(ModelNotReady::NotFound {
            model: requested.to_string(),
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transcribe_cpp_engine::catalog;
    use std::path::PathBuf;

    fn info(name: &str, status: ModelStatus) -> ModelInfo {
        catalog::find_model(name)
            .unwrap_or_else(|| panic!("{name} must be catalogued"))
            .model_info(std::path::Path::new("/models/transcribecpp"), status)
    }

    fn ultra(status: ModelStatus) -> ModelInfo {
        info("parakeet-ultra-Q8_0", status)
    }

    fn gigaam(status: ModelStatus) -> ModelInfo {
        info("gigaam-v3-e2e-ctc-Q8_0", status)
    }

    #[test]
    fn an_installed_configured_model_is_selected() {
        let models = vec![ultra(ModelStatus::Available), gigaam(ModelStatus::Missing)];

        let selected = select_transcription_model(&models, "parakeet-ultra-Q8_0").unwrap();

        assert_eq!(selected.name, "parakeet-ultra-Q8_0");
        assert_eq!(
            selected.path,
            PathBuf::from("/models/transcribecpp/parakeet-ultra-Q8_0.gguf")
        );
    }

    #[test]
    fn a_missing_configured_model_reports_that_it_needs_downloading() {
        let models = vec![ultra(ModelStatus::Missing)];

        let error = select_transcription_model(&models, "parakeet-ultra-Q8_0").unwrap_err();

        assert!(matches!(error, ModelNotReady::Status { .. }));
        let text = error.to_string();
        assert!(text.contains("not downloaded"), "text: {text}");
        assert!(text.contains("Settings"), "text: {text}");
    }

    #[test]
    fn a_corrupted_configured_model_explains_delete_and_redownload() {
        let models = vec![ultra(ModelStatus::Corrupted {
            file_size: 3,
            expected_min_size: 740_363_168,
        })];

        let text = select_transcription_model(&models, "parakeet-ultra-Q8_0")
            .unwrap_err()
            .to_string();

        assert!(text.contains("corrupted"), "text: {text}");
        assert!(text.contains("Delete"), "text: {text}");
    }

    #[test]
    fn a_downloading_configured_model_reports_progress() {
        let models = vec![ultra(ModelStatus::Downloading { progress: 42 })];

        let text = select_transcription_model(&models, "parakeet-ultra-Q8_0")
            .unwrap_err()
            .to_string();

        assert!(text.contains("42%"), "text: {text}");
    }

    #[test]
    fn an_errored_model_reports_the_error_and_a_retry() {
        let models = vec![ultra(ModelStatus::Error("network reset".to_string()))];

        let text = select_transcription_model(&models, "parakeet-ultra-Q8_0")
            .unwrap_err()
            .to_string();

        assert!(text.contains("network reset"), "text: {text}");
        assert!(text.contains("again"), "text: {text}");
    }

    #[test]
    fn an_unknown_configured_model_falls_back_to_an_installed_model() {
        let models = vec![gigaam(ModelStatus::Available), ultra(ModelStatus::Missing)];

        let selected = select_transcription_model(&models, "removed-model").unwrap();

        assert_eq!(selected.name, "gigaam-v3-e2e-ctc-Q8_0");
    }

    #[test]
    fn an_empty_configuration_falls_back_to_an_installed_model() {
        let models = vec![ultra(ModelStatus::Available)];

        assert_eq!(
            select_transcription_model(&models, "").unwrap().name,
            "parakeet-ultra-Q8_0"
        );
    }

    #[test]
    fn with_nothing_installed_the_message_points_at_the_model_manager() {
        let models = vec![ultra(ModelStatus::Missing), gigaam(ModelStatus::Missing)];

        let error = select_transcription_model(&models, "parakeet-ultra-Q8_0").unwrap_err();

        assert_eq!(
            error,
            ModelNotReady::Status {
                model: "parakeet-ultra-Q8_0".to_string(),
                status: ModelStatus::Missing,
            }
        );

        let empty: Vec<ModelInfo> = Vec::new();
        let text = select_transcription_model(&empty, "parakeet-ultra-Q8_0")
            .unwrap_err()
            .to_string();
        assert!(
            text.contains("No Transcribe.cpp model is installed"),
            "text: {text}"
        );
    }

    #[test]
    fn an_unconfigured_but_downloading_model_blocks_recording_instead_of_loading_a_partial_file() {
        // The fallback only accepts a fully installed model; a download in
        // flight is reported with its progress, never loaded.
        let models = vec![ultra(ModelStatus::Downloading { progress: 12 })];

        let text = select_transcription_model(&models, "").unwrap_err().to_string();

        assert!(text.contains("12%"), "text: {text}");
    }

    #[test]
    fn a_downloading_model_never_masquerades_as_available() {
        // Regression guard: the dispatcher must not try to load a partial file.
        for status in [
            ModelStatus::Missing,
            ModelStatus::Downloading { progress: 99 },
            ModelStatus::Corrupted {
                file_size: 1,
                expected_min_size: 2,
            },
            ModelStatus::Error("boom".to_string()),
        ] {
            let models = vec![ultra(status)];
            assert!(
                select_transcription_model(&models, "parakeet-ultra-Q8_0").is_err(),
                "a model that is not fully installed must not be selected"
            );
        }
    }

    #[test]
    fn batch_jobs_select_the_exact_requested_model() {
        let models = vec![ultra(ModelStatus::Available), gigaam(ModelStatus::Available)];

        let selected =
            select_requested_transcription_model(&models, "parakeet-ultra-Q8_0").unwrap();

        assert_eq!(selected.name, "parakeet-ultra-Q8_0");
    }

    #[test]
    fn batch_jobs_reject_a_requested_model_that_is_not_in_the_catalog() {
        let models = vec![ultra(ModelStatus::Available)];

        let error = select_requested_transcription_model(&models, "removed-model").unwrap_err();

        match &error {
            ModelNotReady::NotFound { model } => assert_eq!(model, "removed-model"),
            other => panic!("expected NotFound, got {other:?}"),
        }
        let text = error.to_string();
        assert!(text.contains("removed-model"), "text: {text}");
        assert!(text.contains("not available"), "text: {text}");
    }

    #[test]
    fn batch_jobs_never_fall_back_and_report_the_uninstalled_model_state() {
        // The user asked for this exact model in the dialog: a partial
        // download is reported with its progress, not replaced by another
        // installed model.
        let models = vec![
            ultra(ModelStatus::Downloading { progress: 55 }),
            gigaam(ModelStatus::Available),
        ];

        let text = select_requested_transcription_model(&models, "parakeet-ultra-Q8_0")
            .unwrap_err()
            .to_string();

        assert!(text.contains("55%"), "text: {text}");
    }

    #[test]
    fn a_batch_request_for_an_absent_model_errors_even_when_another_model_is_installed() {
        // Regression (ticket 07): naming model X while only model Y is
        // installed must be a NotFound error — never Y's silent substitution.
        let models = vec![gigaam(ModelStatus::Available)];

        let error = select_requested_transcription_model(&models, "parakeet-ultra-Q8_0")
            .expect_err("the requested model is not installed");

        match &error {
            ModelNotReady::NotFound { model } => assert_eq!(model, "parakeet-ultra-Q8_0"),
            other => panic!("expected NotFound, got {other:?}"),
        }
        let text = error.to_string();
        assert!(text.contains("parakeet-ultra-Q8_0"), "text: {text}");
        assert!(text.contains("not available"), "text: {text}");

        // A corrupted requested model is likewise reported, with the other
        // installed model still untouched.
        let models = vec![
            ultra(ModelStatus::Corrupted {
                file_size: 7,
                expected_min_size: 8,
            }),
            gigaam(ModelStatus::Available),
        ];
        let error = select_requested_transcription_model(&models, "parakeet-ultra-Q8_0")
            .expect_err("a corrupted artifact must not load");
        assert!(
            matches!(&error, ModelNotReady::Status { model, status }
                if model == "parakeet-ultra-Q8_0"
                    && matches!(status, ModelStatus::Corrupted { .. })),
            "error: {error:?}"
        );
        assert!(error.to_string().contains("corrupted"), "text: {error}");
    }

    // The dispatcher (audio::transcription::engine) resolves the configured
    // model through manager.discover_models + select_transcription_model;
    // these cover that composition against real disk states without a native
    // library: recording must fail with a user-facing pointer to the model
    // manager, never attempt a native load.

    #[tokio::test]
    async fn an_empty_models_directory_points_the_user_at_the_model_manager() {
        let root = tempfile::tempdir().expect("tempdir");
        let manager = crate::transcribe_cpp_engine::models::TranscribeCppModelManager::new(
            Some(root.path().to_path_buf()),
        )
        .expect("manager");

        // Nothing downloaded: every catalogued model is Missing, so the
        // configured model reports "not downloaded" with the Settings
        // pointer — the recording gate's user-facing error.
        let models = manager.discover_models().await.expect("discover");
        let error = select_transcription_model(&models, "parakeet-ultra-Q8_0").unwrap_err();

        assert!(
            matches!(&error, ModelNotReady::Status { model, status: ModelStatus::Missing }
                if model == "parakeet-ultra-Q8_0"),
            "error: {error:?}"
        );
        let text = error.to_string();
        assert!(text.contains("not downloaded"), "text: {text}");
        assert!(text.contains("Settings"), "text: {text}");

        // An empty catalog slice (defensive) reports nothing installed.
        let empty: Vec<crate::transcribe_cpp_engine::catalog::ModelInfo> = Vec::new();
        let text = select_transcription_model(&empty, "parakeet-ultra-Q8_0")
            .unwrap_err()
            .to_string();
        assert!(
            text.contains("No Transcribe.cpp model is installed"),
            "text: {text}"
        );
    }

    #[tokio::test]
    async fn a_truncated_artifact_on_disk_is_reported_corrupted_not_loaded() {
        let root = tempfile::tempdir().expect("tempdir");
        let manager = crate::transcribe_cpp_engine::models::TranscribeCppModelManager::new(
            Some(root.path().to_path_buf()),
        )
        .expect("manager");

        // A partial download (right magic, wrong size) must surface as a
        // user-facing corrupted error; the dispatcher never loads it.
        let spec = crate::transcribe_cpp_engine::catalog::find_model("gigaam-v3-ctc-Q4_K_M")
            .expect("catalogued");
        std::fs::write(
            manager.models_directory().join(spec.filename),
            b"GGUF partial",
        )
        .expect("write fixture");

        let models = manager.discover_models().await.expect("discover");
        let error = select_transcription_model(&models, spec.name).unwrap_err();

        assert!(
            matches!(&error, ModelNotReady::Status { model, status }
                if model == spec.name && matches!(status, ModelStatus::Corrupted { file_size: 12, .. })),
            "error: {error:?}"
        );
        let text = error.to_string();
        assert!(text.contains("corrupted"), "text: {text}");
    }
}
