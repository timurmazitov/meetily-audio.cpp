// audio/transcription/mod.rs
//
// Transcription module: Provider abstraction, engine management, and worker pool.

pub mod provider;
pub mod whisper_provider;
pub mod parakeet_provider;
pub mod audio_cpp_provider;
pub mod engine;
pub mod worker;

// Re-export commonly used types
pub use provider::{TranscriptionError, TranscriptionProvider, TranscriptResult};
pub use whisper_provider::WhisperProvider;
pub use parakeet_provider::ParakeetProvider;
pub use audio_cpp_provider::{
    AudioCppProvider, ProviderLoadError, PROVIDER_ID as AUDIO_CPP_PROVIDER_ID,
    TRANSLATION_WHISPER_ONLY_MESSAGE,
};
pub use engine::{
    TranscriptionEngine,
    validate_transcription_model_ready,
    get_or_init_transcription_engine,
    get_or_init_whisper,
    get_or_init_audio_cpp_provider,
    AUDIO_CPP_PROVIDER
};
pub use worker::{
    start_transcription_task,
    reset_speech_detected_flag,
    TranscriptUpdate
};
