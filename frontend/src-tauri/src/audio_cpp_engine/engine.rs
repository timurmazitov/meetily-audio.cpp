//! The engine seam between the transcription provider and the native FFI
//! wrapper.
//!
//! Everything the provider needs from audio.cpp goes through the traits in
//! this module: load a model file (family auto-detected, `family_hint =
//! NULL`), ask the model which tasks/modes it supports, and run one offline
//! transcription. Production uses [`NativeAudioCppEngine`] over
//! [`super::wrapper::AudioCpp`]; tests inject a fake so the provider contract
//! is exercised without a native library.
//!
//! Threading: the provider calls [`AudioCppModel::transcribe_asr`] from
//! `tokio::task::spawn_blocking` and guards the model handle with a mutex, so
//! a handle is never touched by two threads at once. The native implementation
//! keeps one asr/offline session and reuses it for sequential requests — one
//! session serviced by one thread at a time, never concurrent.

use std::path::Path;
use std::sync::Mutex;

use super::error::AudioCppError;
use super::wrapper::{AudioCpp, BackendConfig, Model, Request, Segment, Session};

/// audio.cpp task token for speech recognition.
pub const ASR_TASK: &str = "asr";
/// audio.cpp mode token for offline (whole-buffer) requests.
pub const OFFLINE_MODE: &str = "offline";

/// Output of one offline transcription request.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct TranscriptionOutput {
    /// Family the model auto-detected from its GGUF metadata, e.g.
    /// `gigaam_asr`.
    pub family: String,
    /// Full transcript text; empty when the task produced no speech.
    pub text: String,
    /// Engine segments with sample-based timestamps.
    pub segments: Vec<Segment>,
}

/// Loads model files. The provider only needs this one operation from the
/// library, so a fake can stand in for the whole FFI layer in tests.
pub trait AudioCppEngine: Send + Sync {
    fn load_model(&self, path: &Path) -> Result<Box<dyn AudioCppModel>, AudioCppError>;
}

/// A loaded model file. Handles are not thread-safe; callers serialize access.
pub trait AudioCppModel: Send {
    /// Family auto-detected from the GGUF embedded spec.
    fn family(&self) -> String;

    /// Capability query (`audiocpp_model_supports`): whether this model can
    /// run `task` in `mode`.
    fn supports(&self, task: &str, mode: &str) -> Result<bool, AudioCppError>;

    /// Runs one offline speech-recognition request on the calling thread.
    fn transcribe_asr(
        &mut self,
        audio: &[f32],
        sample_rate: i32,
    ) -> Result<TranscriptionOutput, AudioCppError>;
}

/// [`AudioCppEngine`] backed by the loaded native library.
pub struct NativeAudioCppEngine {
    library: AudioCpp,
}

impl NativeAudioCppEngine {
    pub fn new(library: AudioCpp) -> Self {
        Self { library }
    }
}

impl AudioCppEngine for NativeAudioCppEngine {
    fn load_model(&self, path: &Path) -> Result<Box<dyn AudioCppModel>, AudioCppError> {
        // Family-agnostic by design: the GGUF carries its own spec, so the
        // provider never guesses a family (docs/research/audio-cpp-c-api.md).
        let registry = self.library.create_registry()?;
        let model = registry.load_model(path, None)?;
        Ok(Box::new(NativeAudioCppModel {
            library: self.library.clone(),
            model,
            session: Mutex::new(None),
        }))
    }
}

/// [`AudioCppModel`] backed by the native library.
struct NativeAudioCppModel {
    library: AudioCpp,
    model: Model,
    /// Reused asr/offline session. Guarded so a handle is never used by two
    /// threads at once; created lazily on the first request.
    session: Mutex<Option<Session>>,
}

impl AudioCppModel for NativeAudioCppModel {
    fn family(&self) -> String {
        self.model.family()
    }

    fn supports(&self, task: &str, mode: &str) -> Result<bool, AudioCppError> {
        self.model.supports(task, mode)
    }

    fn transcribe_asr(
        &mut self,
        audio: &[f32],
        sample_rate: i32,
    ) -> Result<TranscriptionOutput, AudioCppError> {
        let family = self.model.family();

        let mut guard = self
            .session
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if guard.is_none() {
            *guard = Some(self.model.create_session(
                ASR_TASK,
                OFFLINE_MODE,
                Some(&BackendConfig::default()),
                None,
            )?);
        }
        let session = guard
            .as_ref()
            .expect("session is created immediately above");

        let mut request = Request::new(&self.library)?;
        request.set_audio(audio, sample_rate, 1)?;
        let result = session.run(&request)?;

        Ok(TranscriptionOutput {
            family,
            text: result.text()?.unwrap_or_default(),
            segments: result.segments()?,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn task_and_mode_tokens_match_the_audio_cpp_vocabulary() {
        // Independent source of truth: docs/models/gigaam_asr.md and
        // audiocpp_model_supports(model, "asr"|"diar", "offline"|"streaming").
        assert_eq!(ASR_TASK, "asr");
        assert_eq!(OFFLINE_MODE, "offline");
    }

    #[test]
    fn empty_transcription_output_is_empty_not_panicking() {
        let output = TranscriptionOutput::default();
        assert!(output.text.is_empty());
        assert!(output.segments.is_empty());
    }
}
