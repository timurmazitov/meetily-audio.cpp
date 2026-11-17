// audio/transcription/mod.rs
//
// Transcription module: Provider abstraction, engine management, and worker pool.

pub mod audio_cpp_diarizer;
pub mod audio_cpp_provider;
pub mod diarization;
pub mod diarization_service;
pub mod engine;
pub mod parakeet_provider;
pub mod provider;
pub mod whisper_provider;
pub mod worker;

// Re-export commonly used types
pub use audio_cpp_diarizer::{AudioCppDiarizer, DiarizationError, DiarizerLoadError};
pub use audio_cpp_provider::{AudioCppProvider, ProviderLoadError};
pub use diarization::{attribute_segments_to_speakers, SpeakerTurnInterval};
pub use diarization_service::{
    apply_speaker_labels, diarize_recording_folder, diarize_segments, diarize_transcript,
    installed_diarization_model, select_diarization_model, DiarizationOutcome,
    DIARIZATION_UNAVAILABLE_MESSAGE, SPEAKER_NAMES_RESET_EVENT, SPEAKER_NAMES_RESET_MESSAGE,
};
pub use engine::{
    get_or_init_audio_cpp_provider, get_or_init_audio_cpp_provider_with_model,
    get_or_init_transcription_engine, get_or_init_whisper, validate_transcription_model_ready,
    TranscriptionEngine, AUDIO_CPP_PROVIDER,
};
pub use parakeet_provider::ParakeetProvider;
pub use provider::{TranscriptResult, TranscriptionError, TranscriptionProvider};
pub use whisper_provider::WhisperProvider;
pub use worker::{reset_speech_detected_flag, start_transcription_task, TranscriptUpdate};
