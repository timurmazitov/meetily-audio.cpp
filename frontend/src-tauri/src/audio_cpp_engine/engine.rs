//! The engine seam between the diarizer and the native FFI wrapper.
//!
//! Everything audio.cpp needs from the library goes through the traits in
//! this module: load a model file (family auto-detected, `family_hint =
//! NULL`), ask the model which tasks/modes it supports, and run one offline
//! diarization. Production uses [`NativeAudioCppEngine`] over
//! [`super::wrapper::AudioCpp`]; tests inject a fake so the diarizer contract
//! is exercised without a native library.
//!
//! Threading: the diarizer calls [`AudioCppModel::diarize`] from
//! `tokio::task::spawn_blocking` and guards the model handle with a mutex, so
//! a handle is never touched by two threads at once. The native implementation
//! keeps one diar/offline session and reuses it for sequential requests — one
//! session serviced by one thread at a time, never concurrent.

use std::path::Path;
use std::sync::{Arc, Mutex};

use super::backend::{autodetect_backend, session_config, AudioCppBackend};
use super::error::AudioCppError;
use super::wrapper::{AudioCpp, AudioCppResult, Model, Request, Session, SpeakerTurn};

/// audio.cpp task token for speaker diarization.
pub const DIAR_TASK: &str = "diar";
/// audio.cpp mode token for offline (whole-buffer) requests.
pub const OFFLINE_MODE: &str = "offline";

/// The audio pipeline delivers 16 kHz mono f32; the engine takes that same
/// contract for diarization.
pub const SAMPLE_RATE: i32 = 16_000;

/// Shortest input worth sending to the engine (10 ms at 16 kHz).
pub const MIN_SAMPLES: usize = 160;

/// Output of one offline diarization request.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct DiarizationOutput {
    /// Speaker turns with sample-based timestamps (16 kHz input).
    pub turns: Vec<SpeakerTurn>,
}

/// Loads model files. The diarizer only needs this one operation from the
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

    /// Runs one offline diarization request on the calling thread.
    fn diarize(
        &mut self,
        audio: &[f32],
        sample_rate: i32,
    ) -> Result<DiarizationOutput, AudioCppError>;
}

/// [`AudioCppEngine`] backed by the loaded native library.
pub struct NativeAudioCppEngine {
    library: AudioCpp,
    /// The compute backend every session of every model loaded through
    /// this engine runs on (chosen by `backend::autodetect_backend` and
    /// mapped to a shipped library variant at load time).
    backend: AudioCppBackend,
}

impl NativeAudioCppEngine {
    pub fn new(library: AudioCpp, backend: AudioCppBackend) -> Self {
        Self { library, backend }
    }
}

impl AudioCppEngine for NativeAudioCppEngine {
    fn load_model(&self, path: &Path) -> Result<Box<dyn AudioCppModel>, AudioCppError> {
        // Family-agnostic by design: the GGUF carries its own spec, so the
        // caller never guesses a family (docs/research/audio-cpp-c-api.md).
        let registry = self.library.create_registry()?;
        let model = registry.load_model(path, None)?;
        let backend = self.backend;
        Ok(Box::new(NativeAudioCppModel {
            library: self.library.clone(),
            model,
            backend,
            diar_session: Mutex::new(None),
        }))
    }
}

/// Loads the native audio.cpp library into an [`AudioCppEngine`].
///
/// This is the single place that chooses the compute backend and constructs
/// the engine; the diarization service loads its model through it. The
/// backend decision itself is `backend::autodetect_backend`, which is already
/// limited to the library variants the pinned archives ship.
///
/// Blocking: the dynamic library load and ABI gate are slow. From async code
/// call this inside `tokio::task::spawn_blocking`.
pub fn load_native_engine() -> Result<Arc<dyn AudioCppEngine>, AudioCppError> {
    let backend = autodetect_backend();
    log::info!(
        "audioCpp compute backend: {} ({})",
        backend.label(),
        backend.session_token()
    );
    let library = AudioCpp::load_backend(backend)?;
    Ok(Arc::new(NativeAudioCppEngine::new(library, backend)))
}

/// [`AudioCppModel`] backed by the native library.
struct NativeAudioCppModel {
    library: AudioCpp,
    model: Model,
    backend: AudioCppBackend,
    /// Reused diar/offline session; created lazily on the first request.
    diar_session: Mutex<Option<Session>>,
}

impl NativeAudioCppModel {
    /// Runs one offline request on the lazily-created session for `task`.
    fn run_offline(
        &self,
        session_slot: &Mutex<Option<Session>>,
        task: &str,
        audio: &[f32],
        sample_rate: i32,
    ) -> Result<AudioCppResult, AudioCppError> {
        let mut guard = session_slot
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if guard.is_none() {
            *guard = Some(self.model.create_session(
                task,
                OFFLINE_MODE,
                Some(&session_config(self.backend)),
                None,
            )?);
        }
        let session = guard
            .as_ref()
            .expect("session is created immediately above");

        let mut request = Request::new(&self.library)?;
        request.set_audio(audio, sample_rate, 1)?;
        session.run(&request)
    }
}

impl AudioCppModel for NativeAudioCppModel {
    fn family(&self) -> String {
        self.model.family()
    }

    fn supports(&self, task: &str, mode: &str) -> Result<bool, AudioCppError> {
        self.model.supports(task, mode)
    }

    fn diarize(
        &mut self,
        audio: &[f32],
        sample_rate: i32,
    ) -> Result<DiarizationOutput, AudioCppError> {
        let result = self.run_offline(&self.diar_session, DIAR_TASK, audio, sample_rate)?;

        Ok(DiarizationOutput {
            turns: result.speaker_turns()?,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn task_and_mode_tokens_match_the_audio_cpp_vocabulary() {
        // Independent source of truth: audiocpp_model_supports(model,
        // "diar", "offline"|"streaming").
        assert_eq!(DIAR_TASK, "diar");
        assert_eq!(OFFLINE_MODE, "offline");
    }

    #[test]
    fn empty_diarization_output_has_no_turns() {
        let output = DiarizationOutput::default();
        assert!(output.turns.is_empty());
    }
}
