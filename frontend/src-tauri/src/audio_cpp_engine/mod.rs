//! audio.cpp (Nemotron diarization) engine module — FFI layer and model
//! management.
//!
//! Handwritten `extern "C"` binding to the audio.cpp C ABI (pinned release
//! v0.9.5, ABI 0.3.0; minimum 0.2.x — there are no official Rust bindings). All
//! interaction with the native library is isolated in `ffi` (raw symbols) and
//! `wrapper` (safe opaque handles); the rest of the app only sees
//! [`AudioCpp`], its handle wrappers, [`AudioCppError`] and the diagnostic
//! command.
//!
//! audio.cpp serves one purpose: speaker diarization (the Nemotron model)
//! over a finished transcript. The ASR path (GigaAM) was removed with the
//! transcribe.cpp consolidation, so nothing here can transcribe audio.
//!
//! Model management (`catalog`, `models`) is deliberately independent of the
//! FFI layer: it lists, downloads, validates and deletes model files without
//! loading the native library, so models can be managed before a diarization
//! pass exists.
//!
//! Threading contract (docs/research/audio-cpp-c-api.md):
//! - Handles are not thread-safe: one session is serviced by one thread.
//!   Wrappers implement `Send` manually and are never `Sync`.
//! - Every native call blocks; from async code call them via
//!   `tokio::task::spawn_blocking`.
//! - The thread count is set per backend via [`BackendConfig`], never
//!   through a global OpenMP setting.
//! - A long-lived loaded model must live on its own thread; [`session_thread`]
//!   is the one implementation of that contract.

mod ffi;

pub mod backend;
pub mod catalog;
pub mod commands;
pub mod engine;
pub mod error;
pub mod models;
pub mod session_thread;
pub mod wrapper;

pub use backend::{autodetect_backend, session_config, AudioCppBackend};
pub use catalog::{ModelInfo, ModelSpec, ModelStatus, ModelTask};
pub use commands::*;
pub use engine::{
    load_native_engine, AudioCppEngine, AudioCppModel, DiarizationOutput, NativeAudioCppEngine,
    DIAR_TASK, MIN_SAMPLES, OFFLINE_MODE, SAMPLE_RATE,
};
pub use error::{AbiVersion, AudioCppError, Status};
pub use models::{AudioCppModelManager, CancelDownloadOutcome};
pub use session_thread::{SessionHandle, SessionLoadError, SessionTask, SessionThread};
pub use wrapper::{
    AudioCpp, AudioCppResult, BackendConfig, Model, Options, Registry, Request, Session,
    SpeakerTurn,
};
