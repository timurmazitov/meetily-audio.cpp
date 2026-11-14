// audio/transcription/diarization_service.rs
//
// Orchestration for audioCpp diarization: resolve the diarization model from
// the catalog (if downloaded), run offline diarization over the whole
// recording, and overlay the speaker turns onto the transcript segments.
//
// The overlay itself is the pure function in `diarization.rs`; this module is
// the thin I/O layer around it. Diarization is strictly optional: when the
// Nemotron model is not installed the transcript is returned unchanged with
// `speakers_available == false`, so the UI can show a hint instead of failing
// the transcription.

use std::path::Path;
use std::sync::Arc;
use std::time::Instant;

use log::{info, warn};

use super::audio_cpp_diarizer::{AudioCppDiarizer, DiarizerLoadError};
use super::diarization::{attribute_segments_to_speakers, SpeakerTurnInterval};
use crate::audio_cpp_engine::backend::{autodetect_backend, shipped_variant};
use crate::audio_cpp_engine::catalog::{ModelInfo, ModelStatus, ModelTask};
use crate::audio_cpp_engine::engine::NativeAudioCppEngine;
use crate::audio_cpp_engine::wrapper::AudioCpp;

/// The audio pipeline delivers 16 kHz mono f32; speaker-turn samples are
/// converted to seconds with this rate.
pub const SAMPLE_RATE: i32 = 16_000;

/// Outcome of an optional diarization pass over a finished transcript.
#[derive(Debug, Clone, PartialEq)]
pub struct DiarizationOutcome {
    /// Speaker label per segment, aligned with the input segment order.
    /// `None` when diarization did not run for that segment.
    pub speakers: Vec<Option<String>>,
    /// True when a diarization model was installed and ran, so the UI can
    /// distinguish "no speakers found" from "diarization unavailable".
    pub speakers_available: bool,
    /// Diagnostic detail when diarization was attempted and failed. The
    /// transcript is still usable; this is for logs and the hint text.
    pub error: Option<String>,
}

impl DiarizationOutcome {
    /// Diarization did not run because no model is installed.
    pub fn unavailable() -> Self {
        Self {
            speakers: Vec::new(),
            speakers_available: false,
            error: None,
        }
    }
}

/// Picks the installed diarization model, if any.
///
/// A model is usable only when it is catalogued for the `diar` task and its
/// artifact is fully installed. Missing/downloading/corrupted artifacts are
/// skipped rather than loaded.
pub fn select_diarization_model(models: &[ModelInfo]) -> Option<&ModelInfo> {
    models.iter().find(|model| {
        model.task == ModelTask::Diar && matches!(model.status, ModelStatus::Available)
    })
}

/// Resolves the installed diarization model from the shared catalog state.
///
/// Returns `None` when the model manager is not initialized (non-audioCpp
/// sessions) or when no diarization artifact is installed.
pub async fn installed_diarization_model() -> Option<ModelInfo> {
    let manager = crate::audio_cpp_engine::commands::model_manager()?;
    let models = manager.discover_models().await.ok()?;
    select_diarization_model(&models).cloned()
}

/// Finds the recording's audio file inside `folder`.
///
/// Mirrors the retranscription lookup: the known names first, then any file
/// with a supported audio extension.
pub fn find_recording_audio(folder: &Path) -> Option<std::path::PathBuf> {
    const CANDIDATES: [&str; 10] = [
        "audio.mp4",
        "audio.m4a",
        "audio.wav",
        "audio.mp3",
        "audio.flac",
        "audio.ogg",
        "recording.mp4",
        "audio.mkv",
        "audio.webm",
        "audio.wma",
    ];
    for name in CANDIDATES {
        let path = folder.join(name);
        if path.exists() {
            return Some(path);
        }
    }

    std::fs::read_dir(folder).ok()?.flatten().find_map(|entry| {
        let path = entry.path();
        let extension = path.extension()?.to_string_lossy().to_lowercase();
        crate::audio::constants::AUDIO_EXTENSIONS
            .contains(&extension.as_str())
            .then_some(path)
    })
}

/// Applies an outcome's speaker labels to `segments` in order.
pub fn apply_speaker_labels(segments: &mut [crate::api::TranscriptSegment], outcome: &DiarizationOutcome) {
    for (segment, speaker) in segments.iter_mut().zip(outcome.speakers.iter()) {
        segment.speaker = speaker.clone();
    }
}

/// Diarizes `audio` (16 kHz mono f32) and applies the labels to `segments`.
///
/// A missing diarization model is not an error: the segments keep no labels
/// and `speakers_available` is false so the caller can show a hint.
pub async fn diarize_segments(
    segments: &mut [crate::api::TranscriptSegment],
    audio: Vec<f32>,
) -> DiarizationOutcome {
    let model = installed_diarization_model().await;
    let bounds: Vec<(f64, f64)> = segments
        .iter()
        .map(|segment| {
            (
                segment.audio_start_time.unwrap_or(0.0),
                segment.audio_end_time.unwrap_or(0.0),
            )
        })
        .collect();
    let outcome = diarize_transcript(model.as_ref(), audio, &bounds).await;
    apply_speaker_labels(segments, &outcome);
    outcome
}

/// Diarizes a saved recording folder and applies the labels to `segments`.
///
/// Decoding and the engine run happen off the async runtime.
pub async fn diarize_recording_folder(
    folder: &Path,
    segments: &mut [crate::api::TranscriptSegment],
) -> DiarizationOutcome {
    let Some(audio_path) = find_recording_audio(folder) else {
        warn!(
            "No audio file in {}; skipping diarization",
            folder.display()
        );
        return DiarizationOutcome {
            speakers: vec![None; segments.len()],
            speakers_available: false,
            error: Some("recording audio not found".to_string()),
        };
    };

    let audio = match tokio::task::spawn_blocking(move || {
        crate::audio::decoder::decode_audio_file(&audio_path)
            .map(|decoded| decoded.to_whisper_format())
    })
    .await
    {
        Ok(Ok(audio)) => audio,
        Ok(Err(error)) => {
            warn!("Failed to decode recording for diarization: {}", error);
            return DiarizationOutcome {
                speakers: vec![None; segments.len()],
                speakers_available: false,
                error: Some(error.to_string()),
            };
        }
        Err(error) => {
            warn!("Diarization decode task failed: {}", error);
            return DiarizationOutcome {
                speakers: vec![None; segments.len()],
                speakers_available: false,
                error: Some(format!("decode task failed: {error}")),
            };
        }
    };

    diarize_segments(segments, audio).await
}

/// Runs diarization for `audio` (16 kHz mono f32) and overlays the resulting
/// speaker turns onto `segment_bounds` (`(start_sec, end_sec)` per segment).
///
/// When `model` is `None` the segments are returned unattributed with
/// `speakers_available == false`. Failures are reported in `error` and never
/// discard the transcript.
pub async fn diarize_transcript(
    model: Option<&ModelInfo>,
    audio: Vec<f32>,
    segment_bounds: &[(f64, f64)],
) -> DiarizationOutcome {
    let Some(model) = model else {
        return DiarizationOutcome {
            speakers: vec![None; segment_bounds.len()],
            speakers_available: false,
            error: None,
        };
    };

    match run_diarization(&model.path, audio).await {
        Ok(turns) => {
            let turns: Vec<SpeakerTurnInterval> = turns
                .into_iter()
                .map(|turn| {
                    SpeakerTurnInterval::new(
                        turn.start_sample as f64 / SAMPLE_RATE as f64,
                        turn.end_sample as f64 / SAMPLE_RATE as f64,
                        turn.speaker_id,
                    )
                })
                .collect();
            DiarizationOutcome {
                speakers: attribute_segments_to_speakers(segment_bounds, &turns),
                speakers_available: true,
                error: None,
            }
        }
        Err(error) => {
            warn!(
                "audioCpp diarization failed; transcript keeps no speaker labels: {}",
                error
            );
            DiarizationOutcome {
                speakers: vec![None; segment_bounds.len()],
                speakers_available: false,
                error: Some(error),
            }
        }
    }
}

/// Loads the diarization model on the native engine and runs one offline pass.
///
/// The library and model are released before returning, so a long diarization
/// does not pin the GPU/CPU memory of the transcription model.
async fn run_diarization(path: &Path, audio: Vec<f32>) -> Result<Vec<crate::audio_cpp_engine::wrapper::SpeakerTurn>, String> {
    let path = path.to_path_buf();
    let desired = autodetect_backend();
    let variant = shipped_variant(desired);
    let model_name = path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| "diarization model".to_string());

    let diarizer = tokio::task::spawn_blocking(move || {
        let library = AudioCpp::load_backend(variant).map_err(|error| error.to_string())?;
        let engine: Arc<dyn crate::audio_cpp_engine::engine::AudioCppEngine> =
            Arc::new(NativeAudioCppEngine::new(library, variant));
        AudioCppDiarizer::load_blocking(engine, model_name, &path)
            .map_err(|error: DiarizerLoadError| error.to_string())
    })
    .await
    .map_err(|error| format!("diarization load task failed: {error}"))?
    .map_err(|error: String| error)?;

    let started = Instant::now();
    let turns = diarizer
        .diarize(audio)
        .await
        .map_err(|error| error.to_string())?;
    info!(
        "audioCpp diarization produced {} speaker turns in {} ms",
        turns.len(),
        started.elapsed().as_millis()
    );

    diarizer.unload();
    Ok(turns)
}

fn diarizer_error_message(error: DiarizerLoadError) -> String {
    error.to_string()
}

#[allow(dead_code)]

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio_cpp_engine::catalog;

    fn model(name: &str, status: ModelStatus) -> ModelInfo {
        catalog::find_model(name)
            .expect("catalogued")
            .model_info(Path::new("/models/audiocpp"), status)
    }

    #[test]
    fn an_installed_diarization_model_is_selected() {
        let models = vec![
            model("gigaam-v3-e2e-ctc-f16", ModelStatus::Available),
            model("nemotron-3-diarization-bf16", ModelStatus::Available),
        ];

        let selected = select_diarization_model(&models).unwrap();

        assert_eq!(selected.name, "nemotron-3-diarization-bf16");
        assert_eq!(selected.task, ModelTask::Diar);
    }

    #[test]
    fn a_missing_diarization_model_is_not_selected() {
        let models = vec![
            model("gigaam-v3-e2e-ctc-f16", ModelStatus::Available),
            model("nemotron-3-diarization-bf16", ModelStatus::Missing),
        ];

        assert!(select_diarization_model(&models).is_none());
    }

    #[test]
    fn a_corrupted_diarization_model_is_not_selected() {
        let models = vec![model(
            "nemotron-3-diarization-bf16",
            ModelStatus::Corrupted {
                file_size: 1,
                expected_min_size: 198_720_928,
            },
        )];

        assert!(select_diarization_model(&models).is_none());
    }

    #[test]
    fn a_transcription_model_is_never_used_for_diarization() {
        let models = vec![model("gigaam-v3-e2e-ctc-f16", ModelStatus::Available)];

        assert!(select_diarization_model(&models).is_none());
    }

    #[tokio::test]
    async fn no_model_leaves_every_segment_without_a_speaker_and_is_unavailable() {
        let outcome = diarize_transcript(None, vec![0.0; 160], &[(0.0, 1.0), (1.0, 2.0)]).await;

        assert_eq!(outcome.speakers, vec![None, None]);
        assert!(!outcome.speakers_available);
        assert!(outcome.error.is_none());
    }
}
