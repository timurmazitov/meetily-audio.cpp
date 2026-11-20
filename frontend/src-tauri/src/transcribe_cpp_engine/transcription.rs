//! Model selection rules for the transcribeCpp provider.
//!
//! The selection rules are pure functions over catalog state, so they are
//! tested without a native library or a Tauri app handle. The transcribe.cpp
//! catalog is ASR-only, so unlike audio.cpp there is no task split: every
//! catalogued model can transcribe, and "ready" is purely an on-disk state.
//!
//! The provider handle and the native loading path are added to this module
//! together with the FFI layer (they touch the native library and are out of
//! scope for the catalog/model-management slice).

use serde::Serialize;

use super::catalog::{ModelInfo, ModelStatus};

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
}
