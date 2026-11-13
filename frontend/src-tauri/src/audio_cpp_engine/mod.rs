//! audio.cpp (GigaAM + Nemotron) transcription engine module — FFI layer.
//!
//! Handwritten `extern "C"` binding to the audio.cpp C ABI (pinned release
//! v0.9.4, ABI 0.2.0; there are no official Rust bindings). All
//! interaction with the native library is isolated in `ffi` (raw symbols)
//! and `wrapper` (safe opaque handles); the rest of the app only sees
//! [`AudioCpp`], its handle wrappers, [`AudioCppError`] and the
//! diagnostic command.
//!
//! Threading contract (docs/research/audio-cpp-c-api.md):
//! - Handles are not thread-safe: one session is serviced by one thread.
//!   Wrappers implement `Send` manually and are never `Sync`.
//! - Every native call blocks; from async code call them via
//!   `tokio::task::spawn_blocking`.
//! - The thread count is set per backend via [`BackendConfig`], never
//!   through a global OpenMP setting.

mod ffi;

pub mod commands;
pub mod error;
pub mod wrapper;

pub use commands::*;
pub use error::{AbiVersion, AudioCppError, Status};
pub use wrapper::{
    AudioCpp, AudioCppResult, BackendConfig, Model, Options, Registry, Request, Segment, Session,
    SpeakerTurn,
};
