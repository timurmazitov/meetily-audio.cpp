//! audio.cpp (GigaAM + Nemotron) transcription engine module — FFI layer and
//! model management.
//!
//! Handwritten `extern "C"` binding to the audio.cpp C ABI (pinned release
//! v0.9.4, ABI 0.2.0; there are no official Rust bindings). All
//! interaction with the native library is isolated in `ffi` (raw symbols)
//! and `wrapper` (safe opaque handles); the rest of the app only sees
//! [`AudioCpp`], its handle wrappers, [`AudioCppError`] and the
//! diagnostic command.
//!
//! Model management (`catalog`, `models`) is deliberately independent of the
//! FFI layer: it lists, downloads, validates and deletes model files without
//! loading the native library, so models can be managed before transcription
//! support exists.
//!
//! Threading contract (docs/research/audio-cpp-c-api.md):
//! - Handles are not thread-safe: one session is serviced by one thread.
//!   Wrappers implement `Send` manually and are never `Sync`.
//! - Every native call blocks; from async code call them via
//!   `tokio::task::spawn_blocking`.
//! - The thread count is set per backend via [`BackendConfig`], never
//!   through a global OpenMP setting.

mod ffi;

pub mod catalog;
pub mod commands;
pub mod engine;
pub mod error;
pub mod models;
pub mod transcription;
pub mod wrapper;

pub use catalog::{ModelInfo, ModelSpec, ModelStatus, ModelTask};
pub use commands::*;
pub use engine::{
    AudioCppEngine, AudioCppModel, DiarizationOutput, NativeAudioCppEngine, TranscriptionOutput,
    ASR_TASK, DIAR_TASK, OFFLINE_MODE,
};
pub use transcription::{
    load_native_provider, provider as audio_cpp_provider, select_transcription_model,
    set_provider as set_audio_cpp_provider, unload_provider as unload_audio_cpp_provider,
    ModelNotReady,
};
pub use error::{AbiVersion, AudioCppError, Status};
pub use models::{AudioCppModelManager, CancelDownloadOutcome};
pub use wrapper::{
    AudioCpp, AudioCppResult, BackendConfig, Model, Options, Registry, Request, Segment, Session,
    SpeakerTurn,
};
