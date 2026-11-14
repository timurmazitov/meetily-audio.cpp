//! App-level wiring of the audioCpp transcription provider: model selection
//! from the catalog, provider loading, and the global provider handle the
//! transcription dispatcher uses.
//!
//! The model *selection* rules are pure functions over catalog state, so they
//! are tested without a native library or a Tauri app handle. Only the
//! [`load_native_provider`] path touches the native FFI layer, and the global
//! handle mirrors the Whisper and Parakeet engines.

use std::sync::{Arc, Mutex};

use serde::Serialize;

use super::backend::{autodetect_backend, shipped_variant};
use super::catalog::{ModelInfo, ModelStatus, ModelTask};
use super::engine::NativeAudioCppEngine;
use super::wrapper::AudioCpp;
use crate::audio::transcription::audio_cpp_provider::{AudioCppProvider, ProviderLoadError};

/// The loaded provider, shared by the transcription dispatcher. Mirrors
/// `WHISPER_ENGINE` and `PARAKEET_ENGINE`.
pub static AUDIOCPP_PROVIDER: Mutex<Option<Arc<AudioCppProvider>>> = Mutex::new(None);

/// The currently loaded provider, if any.
pub fn provider() -> Option<Arc<AudioCppProvider>> {
    AUDIOCPP_PROVIDER
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .as_ref()
        .cloned()
}

/// Replace the global provider.
pub fn set_provider(provider: AudioCppProvider) -> Arc<AudioCppProvider> {
    let provider = Arc::new(provider);
    let mut guard = AUDIOCPP_PROVIDER
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    *guard = Some(Arc::clone(&provider));
    provider
}

/// Drop the global provider, releasing the model.
pub fn unload_provider() {
    let mut guard = AUDIOCPP_PROVIDER
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if let Some(provider) = guard.take() {
        provider.unload();
    }
}

/// Why a configured audioCpp model cannot transcribe right now.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub enum ModelNotReady {
    /// No catalogued transcription model is installed.
    NoModelInstalled,
    /// The configured model is a diarization model, not a transcription one.
    WrongTask { model: String, task: ModelTask },
    /// The artifact is absent, downloading, corrupted or errored.
    Status { model: String, status: ModelStatus },
}

impl std::fmt::Display for ModelNotReady {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NoModelInstalled => write!(
                f,
                "No audioCpp transcription model is installed. Download GigaAM v3 in Settings → Transcript Model → audioCpp models."
            ),
            Self::WrongTask { model, task } => write!(
                f,
                "audioCpp model '{}' is a {} model and cannot transcribe audio. Select a transcription model such as GigaAM v3 in Settings.",
                model,
                task.label().to_lowercase()
            ),
            Self::Status { model, status } => match status {
                ModelStatus::Missing => write!(
                    f,
                    "audioCpp model '{}' is not downloaded. Download it in Settings → Transcript Model → audioCpp models.",
                    model
                ),
                ModelStatus::Downloading { progress } => write!(
                    f,
                    "audioCpp model '{}' is still downloading ({}%). Please wait for it to finish.",
                    model, progress
                ),
                ModelStatus::Corrupted { .. } => write!(
                    f,
                    "audioCpp model '{}' is corrupted. Delete it and download it again in Settings.",
                    model
                ),
                ModelStatus::Error(detail) => write!(
                    f,
                    "audioCpp model '{}' has an error: {}. Download it again in Settings.",
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
/// The configured model wins when it is a catalogued, installed transcription
/// model. Otherwise the first installed transcription model is used, so a
/// deleted or renamed model does not block recording. Missing, downloading and
/// corrupted artifacts are reported instead of being loaded.
pub fn select_transcription_model(
    models: &[ModelInfo],
    configured: &str,
) -> Result<ModelInfo, ModelNotReady> {
    let transcription_models: Vec<&ModelInfo> = models
        .iter()
        .filter(|model| model.task == ModelTask::Asr)
        .collect();

    if let Some(model) = transcription_models
        .iter()
        .find(|model| model.name == configured)
    {
        return ensure_ready(model);
    }

    // A configured model that exists in the catalog but is not a
    // transcription model is a configuration mistake, not a missing file.
    if let Some(other) = models.iter().find(|model| model.name == configured) {
        return Err(ModelNotReady::WrongTask {
            model: other.name.clone(),
            task: other.task,
        });
    }

    if let Some(installed) = transcription_models
        .iter()
        .find(|model| matches!(model.status, ModelStatus::Available))
    {
        log::info!(
            "audioCpp model '{}' is unavailable; using installed model '{}'",
            configured,
            installed.name
        );
        return Ok((*installed).clone());
    }

    match transcription_models.first() {
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

/// Loads the native library and the selected model into a provider.
///
/// The compute backend is chosen here, once, from the app's hardware
/// autodetect (priority CUDA → Vulkan → CPU) and mapped to the shipped
/// library variant (`backend` module). Downloaded models are
/// backend-agnostic, so the same GGUF loads on any variant.
///
/// Blocking: the library load and model load are slow. From async code call
/// this inside `tokio::task::spawn_blocking`.
pub fn load_native_provider(
    library: AudioCpp,
    model: &ModelInfo,
) -> Result<AudioCppProvider, ProviderLoadError> {
    let desired = autodetect_backend();
    let variant = shipped_variant(desired);
    log::info!(
        "audioCpp compute backend: detected {}, running {} ({})",
        desired.label(),
        variant.label(),
        variant.session_token()
    );
    let engine = Arc::new(NativeAudioCppEngine::new(library, variant));
    AudioCppProvider::load_blocking(engine, model.name.clone(), &model.path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio_cpp_engine::catalog;
    use crate::audio_cpp_engine::catalog::{ModelSpec, ModelTask};
    use std::path::PathBuf;

    fn info(spec: &ModelSpec, status: ModelStatus) -> ModelInfo {
        spec.model_info(std::path::Path::new("/models/audiocpp"), status)
    }

    fn gigaam(status: ModelStatus) -> ModelInfo {
        info(
            catalog::find_model("gigaam-v3-e2e-ctc-f16").expect("gigaam catalogued"),
            status,
        )
    }

    fn nemotron(status: ModelStatus) -> ModelInfo {
        info(
            catalog::find_model("nemotron-3-diarization-bf16").expect("nemotron catalogued"),
            status,
        )
    }

    #[test]
    fn an_installed_configured_model_is_selected() {
        let models = vec![gigaam(ModelStatus::Available), nemotron(ModelStatus::Missing)];

        let selected = select_transcription_model(&models, "gigaam-v3-e2e-ctc-f16").unwrap();

        assert_eq!(selected.name, "gigaam-v3-e2e-ctc-f16");
        assert_eq!(selected.task, ModelTask::Asr);
        assert_eq!(
            selected.path,
            PathBuf::from("/models/audiocpp/gigaam-v3-e2e-ctc-f16.gguf")
        );
    }

    #[test]
    fn a_missing_configured_model_reports_that_it_needs_downloading() {
        let models = vec![gigaam(ModelStatus::Missing)];

        let error = select_transcription_model(&models, "gigaam-v3-e2e-ctc-f16").unwrap_err();

        assert!(matches!(error, ModelNotReady::Status { .. }));
        let text = error.to_string();
        assert!(text.contains("not downloaded"), "text: {text}");
        assert!(text.contains("Settings"), "text: {text}");
    }

    #[test]
    fn a_corrupted_configured_model_explains_delete_and_redownload() {
        let models = vec![gigaam(ModelStatus::Corrupted {
            file_size: 3,
            expected_min_size: 442_520_704,
        })];

        let text = select_transcription_model(&models, "gigaam-v3-e2e-ctc-f16")
            .unwrap_err()
            .to_string();

        assert!(text.contains("corrupted"), "text: {text}");
        assert!(text.contains("Delete"), "text: {text}");
    }

    #[test]
    fn a_downloading_configured_model_reports_progress() {
        let models = vec![gigaam(ModelStatus::Downloading { progress: 42 })];

        let text = select_transcription_model(&models, "gigaam-v3-e2e-ctc-f16")
            .unwrap_err()
            .to_string();

        assert!(text.contains("42%"), "text: {text}");
    }

    #[test]
    fn an_errored_model_reports_the_error_and_a_retry() {
        let models = vec![gigaam(ModelStatus::Error("network reset".to_string()))];

        let text = select_transcription_model(&models, "gigaam-v3-e2e-ctc-f16")
            .unwrap_err()
            .to_string();

        assert!(text.contains("network reset"), "text: {text}");
        assert!(text.contains("again"), "text: {text}");
    }

    #[test]
    fn a_diarization_model_is_rejected_as_a_transcription_model() {
        let models = vec![nemotron(ModelStatus::Available)];

        let error = select_transcription_model(&models, "nemotron-3-diarization-bf16").unwrap_err();

        match &error {
            ModelNotReady::WrongTask { task, .. } => assert_eq!(*task, ModelTask::Diar),
            other => panic!("expected WrongTask, got {other:?}"),
        }
        let text = error.to_string();
        assert!(text.contains("cannot transcribe"), "text: {text}");
        assert!(text.contains("GigaAM"), "text: {text}");
    }

    #[test]
    fn an_unknown_configured_model_falls_back_to_an_installed_transcription_model() {
        let models = vec![gigaam(ModelStatus::Available), nemotron(ModelStatus::Available)];

        let selected = select_transcription_model(&models, "removed-model").unwrap();

        assert_eq!(selected.name, "gigaam-v3-e2e-ctc-f16");
    }

    #[test]
    fn an_empty_configuration_falls_back_to_an_installed_transcription_model() {
        let models = vec![gigaam(ModelStatus::Available)];

        assert_eq!(
            select_transcription_model(&models, "").unwrap().name,
            "gigaam-v3-e2e-ctc-f16"
        );
    }

    #[test]
    fn with_nothing_installed_the_message_points_at_the_model_manager() {
        let models = vec![gigaam(ModelStatus::Missing), nemotron(ModelStatus::Missing)];

        let error = select_transcription_model(&models, "gigaam-v3-e2e-ctc-f16").unwrap_err();

        assert_eq!(error, ModelNotReady::Status {
            model: "gigaam-v3-e2e-ctc-f16".to_string(),
            status: ModelStatus::Missing,
        });

        let empty: Vec<ModelInfo> = Vec::new();
        let text = select_transcription_model(&empty, "gigaam-v3-e2e-ctc-f16")
            .unwrap_err()
            .to_string();
        assert!(
            text.contains("No audioCpp transcription model is installed"),
            "text: {text}"
        );
    }

    #[test]
    fn an_unconfigured_but_downloading_model_blocks_recording_instead_of_loading_a_partial_file() {
        // The fallback only accepts a fully installed model; a download in
        // flight is reported with its progress, never loaded.
        let models = vec![gigaam(ModelStatus::Downloading { progress: 12 })];

        let text = select_transcription_model(&models, "")
            .unwrap_err()
            .to_string();

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
            let models = vec![gigaam(status)];
            assert!(
                select_transcription_model(&models, "gigaam-v3-e2e-ctc-f16").is_err(),
                "a model that is not fully installed must not be selected"
            );
        }
    }
}
