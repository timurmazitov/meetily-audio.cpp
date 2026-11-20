//! transcribe.cpp (ggml) transcription engine module — model management
//! and the native FFI layer.
//!
//! Handwritten `extern "C"` binding to the transcribe.cpp C ABI (pinned
//! release v0.4.2, header hash `f99ad4169374b8f3` — there are no official
//! Rust bindings). All interaction with the native library is isolated in
//! `ffi` (raw symbols) and `wrapper` (safe opaque handles + the
//! contract.json ABI gate); the rest of the app only sees
//! [`TranscribeCpp`], its handle wrappers, [`TranscribeCppError`], the
//! seam traits in [`engine`], and the [`transcribecpp_diagnostics`]
//! command. This is the only place with unsafe FFI for this engine.
//!
//! Model management (`catalog`, `models`, `commands`, `transcription`) is
//! deliberately independent of the FFI layer: it lists, downloads,
//! validates and deletes model files without loading the native library,
//! so models can be managed before transcription support exists.
//!
//! Naming convention (mirrors `audio_cpp_engine`): Tauri commands and events
//! use the `transcribecpp_` / `transcribecpp-` prefix; the provider config
//! string is `transcribeCpp`; the models live under
//! `<app_data>/models/transcribecpp`.
//!
//! Threading contract (transcribe.h v0.4.2 preamble):
//! - Model queries are thread-safe; sessions are NOT: one session is
//!   serviced by one thread at a time, and the model must outlive its
//!   sessions. Wrappers implement `Send` manually and are never `Sync`.
//! - Every native call blocks; from async code call them via
//!   `tokio::task::spawn_blocking`.
//! - A long-lived loaded model must live on its own thread;
//!   [`session_thread`] is the one implementation of that contract.

mod ffi;

pub mod backend;
pub mod catalog;
pub mod commands;
pub mod diagnostics;
pub mod engine;
pub mod error;
pub mod models;
pub mod session_thread;
pub mod transcription;
pub mod wrapper;

pub use backend::{select_device, ComputeMode, DeviceQuery, DeviceSelection};
pub use catalog::{ModelInfo, ModelSpec, ModelStatus};
pub use commands::*;
pub use diagnostics::{transcribecpp_diagnostics, TranscribeCppDiagnostics};
pub use engine::{
    NativeTranscribeCppEngine, TranscribeCppEngine, TranscribeCppModel, TranscriptionOutput,
    MIN_SAMPLES, SAMPLE_RATE,
};
pub use error::{
    Status, TranscribeCppError, TRANSCRIBECPP_PINNED_HEADER_HASH, TRANSCRIBECPP_PINNED_VERSION,
};
pub use models::{TranscribeCppModelManager, CancelDownloadOutcome};
pub use session_thread::{
    SessionHandle, SessionLoadError, SessionRequestError, SessionTask, SessionThread,
};
pub use transcription::{
    load_native_engine, load_native_provider, provider, resolve_library_path,
    select_requested_transcription_model, select_transcription_model, set_provider,
    unload_provider, ModelNotReady, TRANSCRIBECPP_PROVIDER,
};
pub use wrapper::{
    AbiContract, Model, RunOutput, Segment, Session, TranscribeCpp, CONTRACT_FILE_NAME,
    LIBRARY_FILE_NAMES, LIBRARY_LANES_DIR, TRANSCRIBECPP_LIBRARY_ENV,
};
