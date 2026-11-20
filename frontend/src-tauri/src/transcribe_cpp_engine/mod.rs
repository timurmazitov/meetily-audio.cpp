//! transcribe.cpp transcription engine module — model management today,
//! FFI layer when the native integration lands.
//!
//! The engine is transcribe.cpp (https://github.com/handy-computer/transcribe.cpp,
//! pinned v0.4.2, ggml runtime, ABI gate on `version` + `header_hash`). The
//! FFI files (`ffi`, `wrapper`, `error`, `backend`, `engine`,
//! `session_thread`) are added together with the native library integration;
//! until then this module exposes only the parts that must work WITHOUT the
//! native library.
//!
//! Model management is deliberately independent of the FFI layer:
//! - `catalog` — static catalog of the 9 RU-capable GGUF variants with the
//!   five offered quantizations, pinned exact sizes and SHA-256 digests.
//! - `models` — [`TranscribeCppModelManager`]: disk status, download (via the
//!   shared [`crate::model_downloader`]), cancellation and deletion.
//! - `commands` — Tauri commands for the model manager; they never load the
//!   native library, so the settings UI works before transcription exists.
//! - `transcription` — pure model-selection rules (configured-wins with
//!   fallback; exact match for batch jobs) plus the provider handle helpers
//!   the native loader will use.
//!
//! Naming convention (mirrors `audio_cpp_engine`): Tauri commands and events
//! use the `transcribecpp_` / `transcribecpp-` prefix; the provider config
//! string is `transcribeCpp`; the models live under
//! `<app_data>/models/transcribecpp`.

pub mod catalog;
pub mod commands;
pub mod models;
pub mod transcription;

// The FFI layer (`error`, `ffi`, `wrapper`, `backend`, `engine`,
// `session_thread`) lands with the native library ticket and re-exports from
// here.

pub use catalog::{ModelInfo, ModelSpec, ModelStatus};
pub use commands::*;
pub use models::{TranscribeCppModelManager, CancelDownloadOutcome};
pub use transcription::{
    select_requested_transcription_model, select_transcription_model, ModelNotReady,
};
