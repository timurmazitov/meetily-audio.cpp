//! The engine seam between the transcription provider and the native FFI
//! wrapper.
//!
//! Everything the provider (ticket 05) and diarization/fallback logic
//! (ticket 06) need from transcribe.cpp goes through the traits in this
//! module: load a model file and run one offline transcription on the
//! 16 kHz mono f32 contract. Production uses [`NativeTranscribeCppEngine`]
//! over [`super::wrapper::TranscribeCpp`]; tests inject a fake so the
//! provider contract is exercised without a native library.
//!
//! Threading: the library forbids concurrent compute across sessions of
//! one model (transcribe.h preamble) and a session is single-threaded.
//! The long-lived model therefore lives on a dedicated thread — see
//! [`super::session_thread`]; the traits only require `Send`, never
//! `Sync`, on the model.

use std::path::Path;

use super::backend::{fix_process_backends, select_device, ComputeMode, DeviceSelection};
use super::error::TranscribeCppError;
use super::wrapper::{Model, Segment, Session, TranscribeCpp};

/// The sample rate transcribe.cpp v1 accepts: 16 kHz mono float32 PCM,
/// no resampler is linked into the library.
pub const SAMPLE_RATE: i32 = 16_000;

/// Shortest input worth sending to the engine (10 ms at 16 kHz).
pub const MIN_SAMPLES: usize = 160;

/// Output of one offline transcription request. Fields mirror the
/// audio.cpp seam so ticket 05's provider can map both engines.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct TranscriptionOutput {
    /// The runtime backend the model actually ended up on ("cpu",
    /// "vulkan", "metal", ...), for detecting a Vulkan→CPU fallback.
    pub backend: String,
    /// Full transcript text; empty when the task produced no speech.
    pub text: String,
    /// Engine segments with millisecond timestamps.
    pub segments: Vec<Segment>,
    /// Language the model detected, when any.
    pub detected_language: Option<String>,
}

/// Loads model files. The provider only needs this one operation from the
/// library, so a fake can stand in for the whole FFI layer in tests.
pub trait TranscribeCppEngine: Send + Sync {
    fn load_model(&self, path: &Path) -> Result<Box<dyn TranscribeCppModel>, TranscribeCppError>;
}

/// A loaded model file. Not `Sync`: callers serialize access and keep the
/// handle on the thread that owns it (`super::session_thread`).
pub trait TranscribeCppModel: Send {
    /// The runtime backend actually bound to the model ("cpu", "vulkan",
    /// ...), from `transcribe_model_backend`.
    fn backend(&self) -> String;

    /// Runs one offline speech-recognition request on the calling thread.
    /// `audio` is 16 kHz mono f32 ([`SAMPLE_RATE`]); `language` is an
    /// optional BCP-47-ish hint (None = autodetect).
    fn transcribe_asr(
        &mut self,
        audio: &[f32],
        sample_rate: i32,
        language: Option<&str>,
    ) -> Result<TranscriptionOutput, TranscribeCppError>;
}

/// [`TranscribeCppEngine`] backed by the loaded native library.
pub struct NativeTranscribeCppEngine {
    library: TranscribeCpp,
    selection: DeviceSelection,
}

impl NativeTranscribeCppEngine {
    /// Prepares an engine over an ABI-gated library: fixes the backend
    /// mask and registers the ggml modules shipped next to the library
    /// (once per process), then picks the device for `mode`. Blocking;
    /// call via `spawn_blocking` from async code.
    ///
    /// The mask is fixed process-wide at the first registration, so a later
    /// load with a different saved mode registers under the mask already in
    /// force (only a restart changes it) while still selecting the device for
    /// the requested `mode`: a request outside the fixed mask degrades to CPU
    /// through [`select_device`], the same path a missing driver takes.
    pub fn new(library: TranscribeCpp, mode: ComputeMode) -> Result<Self, TranscribeCppError> {
        // The mask must be fixed before any device query or model load
        // (transcribe.h: "call once, first").
        let registered = fix_process_backends(mode).effective();
        library.init_backends(registered.allowed_mask())?;
        let selection = select_device(mode, &library);
        Ok(Self {
            library,
            selection,
        })
    }

    /// The device selection this engine loads models on (ticket 06: use
    /// [`DeviceSelection::fell_back_to_cpu`] plus [`TranscribeCppModel::backend`]
    /// to warn about a Vulkan→CPU fallback).
    pub fn selection(&self) -> &DeviceSelection {
        &self.selection
    }

    /// The library this engine loads from.
    pub fn library(&self) -> &TranscribeCpp {
        &self.library
    }
}

impl TranscribeCppEngine for NativeTranscribeCppEngine {
    fn load_model(&self, path: &Path) -> Result<Box<dyn TranscribeCppModel>, TranscribeCppError> {
        // Pin the selected device when one matched the mode; otherwise let
        // the library apply its automatic policy for the backend.
        let device = match self.selection.device_index {
            Some(index) => unsafe { self.library.device_handle(index) },
            None => std::ptr::null_mut(),
        };
        let model = self
            .library
            .load_model(path, self.selection.backend_request, device)?;
        Ok(Box::new(NativeTranscribeCppModel {
            model,
            session: None,
        }))
    }
}

/// [`TranscribeCppModel`] backed by the native library. Kept `Send` (moves
/// to the session thread) and never `Sync`; one lazily-created session is
/// reused for sequential runs, matching the library's single-threaded
/// session contract.
struct NativeTranscribeCppModel {
    model: Model,
    /// Reused offline session; created lazily on the first request on the
    /// owning thread. `&mut self` is the serialization point.
    session: Option<Session>,
}

impl TranscribeCppModel for NativeTranscribeCppModel {
    fn backend(&self) -> String {
        self.model.backend()
    }

    fn transcribe_asr(
        &mut self,
        audio: &[f32],
        sample_rate: i32,
        language: Option<&str>,
    ) -> Result<TranscriptionOutput, TranscribeCppError> {
        if sample_rate != SAMPLE_RATE {
            return Err(TranscribeCppError::Native {
                status: super::error::Status::SampleRate,
                detail: Some(format!(
                    "transcribe.cpp accepts 16 kHz mono f32 only, got {sample_rate} Hz"
                )),
                context: "transcribe_run",
            });
        }
        let session = match self.session.as_ref() {
            Some(session) => session,
            None => self.session.insert(self.model.create_session()?),
        };
        let output = session.run(audio, language)?;
        Ok(TranscriptionOutput {
            backend: self.model.backend(),
            text: output.text,
            segments: output.segments,
            detected_language: output.detected_language,
        })
    }
}

// Segment / RunOutput are re-exported from `wrapper` at the module root.

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sample_rate_and_minimum_match_the_native_contract() {
        // Independent source of truth: transcribe.h v0.4.2 run docs
        // ("only 16 kHz mono float32 PCM") and the worker's 10 ms floor.
        assert_eq!(SAMPLE_RATE, 16_000);
        assert_eq!(MIN_SAMPLES, 160);
    }

    #[test]
    fn engine_and_model_traits_are_shaped_for_the_session_thread() {
        // The session thread moves the model to its own thread: the model
        // must be Send but the API deliberately does not require Sync.
        fn assert_engine_bounds<E: TranscribeCppEngine>() {}
        fn assert_send<T: Send>() {}
        assert_engine_bounds::<NativeTranscribeCppEngine>();
        assert_send::<Box<dyn TranscribeCppModel>>();
    }

    #[test]
    fn empty_transcription_output_is_empty_not_panicking() {
        let output = TranscriptionOutput::default();
        assert!(output.text.is_empty());
        assert!(output.segments.is_empty());
        assert!(output.backend.is_empty());
        assert_eq!(output.detected_language, None);
    }
}
