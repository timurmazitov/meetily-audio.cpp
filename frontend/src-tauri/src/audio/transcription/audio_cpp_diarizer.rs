// audio/transcription/audio_cpp_diarizer.rs
//
// audioCpp diarization: runs Nemotron-3-Diarization over a whole recording
// after transcription and returns speaker turns (GLOSSARY: «поворот
// спикера»), which the caller overlays onto transcript segments.
//
// Design notes:
// - The model and its session live on a dedicated thread for their whole
//   lifetime; audio.cpp handles are not thread-safe individually. The
//   thread machinery is `audio_cpp_engine::session_thread`.
// - Input is the pipeline contract: 16 kHz mono f32, exactly what the
//   recording decoder produces.
// - Capability gate: the model must support `diar` / `offline`.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use log::info;

use crate::audio_cpp_engine::engine::{
    AudioCppEngine, AudioCppModel, DiarizationOutput, DIAR_TASK, MIN_SAMPLES, OFFLINE_MODE,
};
use crate::audio_cpp_engine::error::AudioCppError;
use crate::audio_cpp_engine::session_thread::{SessionLoadError, SessionTask, SessionThread};
use crate::audio_cpp_engine::wrapper::SpeakerTurn;

/// Failure of a diarization model load, with a user-facing message.
#[derive(Debug, Clone, PartialEq)]
pub enum DiarizerLoadError {
    /// The native engine refused or failed to load the model file.
    Engine(String),
    /// The model loaded but cannot run offline diarization.
    UnsupportedCapability { model: String, family: String },
}

impl std::fmt::Display for DiarizerLoadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Engine(detail) => write!(f, "Failed to load audio.cpp model: {detail}"),
            Self::UnsupportedCapability { model, family } => write!(
                f,
                "Model '{}' (family '{}') cannot diarize audio offline. Choose a diarization model such as Nemotron-3-Diarization in Settings.",
                model, family
            ),
        }
    }
}

impl std::error::Error for DiarizerLoadError {}

/// Maps a session-thread load failure onto the diarizer's error.
fn load_error(error: SessionLoadError) -> DiarizerLoadError {
    DiarizerLoadError::Engine(error.detail)
}

/// Why one diarization request failed.
#[derive(Debug, Clone, PartialEq)]
pub enum DiarizationError {
    /// The diarization model is not loaded (or was unloaded).
    ModelNotLoaded,
    /// The engine run failed; the detail is diagnostic.
    EngineFailed(String),
}

impl std::fmt::Display for DiarizationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ModelNotLoaded => write!(f, "Diarization model is not loaded"),
            Self::EngineFailed(detail) => write!(f, "Diarization failed: {detail}"),
        }
    }
}

impl std::error::Error for DiarizationError {}

/// The diarization work a [`SessionThread`] runs: `diar` / `offline`.
struct DiarTask;

impl SessionTask for DiarTask {
    type Output = DiarizationOutput;

    const TASK: &'static str = DIAR_TASK;

    fn run(
        model: &mut dyn AudioCppModel,
        audio: &[f32],
        sample_rate: i32,
    ) -> Result<DiarizationOutput, AudioCppError> {
        model.diarize(audio, sample_rate)
    }
}

type Session = SessionThread<DiarizationOutput>;

/// audioCpp diarizer backed by the audio.cpp engine.
///
/// The engine model and its session live on a dedicated thread
/// ([`SessionThread`]); this struct holds only the channel to it.
pub struct AudioCppDiarizer {
    model_name: String,
    session: Mutex<Option<Session>>,
    loaded: AtomicBool,
}

impl std::fmt::Debug for AudioCppDiarizer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AudioCppDiarizer")
            .field("model_name", &self.model_name)
            .field("model_loaded", &self.model_is_loaded())
            .finish()
    }
}

impl AudioCppDiarizer {
    /// Loads `path` and prepares it for offline diarization.
    ///
    /// Blocking: the native model load is slow and must run off the async
    /// runtime (callers use [`Self::load`] for that).
    pub fn load_blocking(
        engine: Arc<dyn AudioCppEngine>,
        model_name: impl Into<String>,
        path: &Path,
    ) -> Result<Self, DiarizerLoadError> {
        let model_name = model_name.into();
        let session = Session::spawn::<DiarTask>(engine, path, "audiocpp-diar-session")
            .map_err(load_error)?;

        // Capture the metadata before the rejection path consumes the
        // session thread.
        let family = session.family.clone();
        if !session.supports_offline {
            session.shutdown();
            return Err(DiarizerLoadError::UnsupportedCapability {
                model: model_name,
                family,
            });
        }

        info!(
            "audioCpp diarization model '{}' loaded (family '{}'), supports {} / {}",
            model_name, session.family, DIAR_TASK, OFFLINE_MODE
        );

        Ok(Self {
            model_name,
            session: Mutex::new(Some(session)),
            loaded: AtomicBool::new(true),
        })
    }

    /// Async wrapper around [`Self::load_blocking`].
    pub async fn load(
        engine: Arc<dyn AudioCppEngine>,
        model_name: impl Into<String>,
        path: PathBuf,
    ) -> Result<Self, DiarizerLoadError> {
        let model_name = model_name.into();
        tokio::task::spawn_blocking(move || Self::load_blocking(engine, model_name, &path))
            .await
            .map_err(|error| DiarizerLoadError::Engine(format!("load task failed: {error}")))?
    }

    /// The configured model id (stable catalog name).
    pub fn model_name(&self) -> &str {
        &self.model_name
    }

    fn model_is_loaded(&self) -> bool {
        self.loaded.load(Ordering::SeqCst)
    }

    /// Drops the session thread and the model it owns, releasing memory.
    pub fn unload(&self) {
        let session = self
            .session
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .take();
        self.loaded.store(false, Ordering::SeqCst);
        if let Some(session) = session {
            session.shutdown();
            info!("audioCpp diarization model '{}' unloaded", self.model_name);
        }
    }

    /// Runs offline diarization over `audio` (16 kHz mono f32).
    ///
    /// The engine call happens on the model's dedicated thread and is
    /// awaited here, so a multi-minute recording never blocks the async
    /// runtime.
    pub async fn diarize(&self, audio: Vec<f32>) -> Result<Vec<SpeakerTurn>, DiarizationError> {
        if audio.len() < MIN_SAMPLES {
            return Err(DiarizationError::EngineFailed(format!(
                "audio is too short: {} samples (minimum {MIN_SAMPLES})",
                audio.len()
            )));
        }

        if !self.model_is_loaded() {
            return Err(DiarizationError::ModelNotLoaded);
        }

        let handle = {
            let guard = self
                .session
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            match guard.as_ref() {
                Some(session) => session.handle(),
                None => return Err(DiarizationError::ModelNotLoaded),
            }
        };

        handle
            .request(audio)
            .await
            .map(|output| output.turns)
            .map_err(|error| DiarizationError::EngineFailed(error.to_string()))
    }
}

impl Drop for AudioCppDiarizer {
    fn drop(&mut self) {
        let session = self
            .session
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .take();
        if let Some(session) = session {
            session.shutdown();
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Mutex as StdMutex;

    use super::*;
    use crate::audio_cpp_engine::engine::SAMPLE_RATE;
    use crate::audio_cpp_engine::error::Status;

    /// What the fake model observed, shared with the test.
    #[derive(Default)]
    struct Calls {
        supports: Vec<(String, String)>,
        sample_rates: Vec<i32>,
        audio_lengths: Vec<usize>,
        threads: Vec<std::thread::ThreadId>,
        load_thread: Option<std::thread::ThreadId>,
    }

    /// Fake engine behind the FFI seam: no native library in tests.
    struct FakeEngine {
        family: String,
        supports_diar_offline: bool,
        run_error: Option<String>,
        output: DiarizationOutput,
        ran: Arc<AtomicBool>,
        calls: Arc<StdMutex<Calls>>,
    }

    impl FakeEngine {
        fn new(family: &str) -> Arc<Self> {
            Arc::new(Self {
                family: family.to_string(),
                supports_diar_offline: true,
                run_error: None,
                output: DiarizationOutput {
                    turns: vec![SpeakerTurn {
                        start_sample: 0,
                        end_sample: 16_000,
                        speaker_id: "SPEAKER_00".to_string(),
                        confidence: 0.95,
                        text: String::new(),
                    }],
                },
                ran: Arc::new(AtomicBool::new(false)),
                calls: Arc::new(StdMutex::new(Calls::default())),
            })
        }

        fn ran(&self) -> bool {
            self.ran.load(Ordering::SeqCst)
        }

        fn calls(&self) -> std::sync::MutexGuard<'_, Calls> {
            self.calls.lock().unwrap()
        }
    }

    impl AudioCppEngine for FakeEngine {
        fn load_model(&self, _path: &Path) -> Result<Box<dyn AudioCppModel>, AudioCppError> {
            self.calls.lock().unwrap().load_thread = Some(std::thread::current().id());
            Ok(Box::new(FakeModel {
                family: self.family.clone(),
                supports_diar_offline: self.supports_diar_offline,
                run_error: self.run_error.clone(),
                output: self.output.clone(),
                ran: Arc::clone(&self.ran),
                calls: Arc::clone(&self.calls),
            }))
        }
    }

    struct FakeModel {
        family: String,
        supports_diar_offline: bool,
        run_error: Option<String>,
        output: DiarizationOutput,
        ran: Arc<AtomicBool>,
        calls: Arc<StdMutex<Calls>>,
    }

    impl AudioCppModel for FakeModel {
        fn family(&self) -> String {
            self.family.clone()
        }

        fn supports(&self, task: &str, mode: &str) -> Result<bool, AudioCppError> {
            let mut calls = self.calls.lock().unwrap();
            calls.supports.push((task.to_string(), mode.to_string()));
            calls.threads.push(std::thread::current().id());
            Ok(task == DIAR_TASK && mode == OFFLINE_MODE && self.supports_diar_offline)
        }

        fn diarize(
            &mut self,
            audio: &[f32],
            sample_rate: i32,
        ) -> Result<DiarizationOutput, AudioCppError> {
            let mut calls = self.calls.lock().unwrap();
            calls.sample_rates.push(sample_rate);
            calls.audio_lengths.push(audio.len());
            calls.threads.push(std::thread::current().id());
            drop(calls);

            self.ran.store(true, Ordering::SeqCst);
            if let Some(detail) = &self.run_error {
                return Err(AudioCppError::Native {
                    status: Status::Runtime,
                    detail: Some(detail.clone()),
                    context: "audiocpp_session_run",
                });
            }
            Ok(self.output.clone())
        }
    }

    async fn diarizer(engine: Arc<FakeEngine>) -> AudioCppDiarizer {
        AudioCppDiarizer::load(
            engine,
            "nemotron-3-diarization-bf16",
            PathBuf::from("/models/diar.gguf"),
        )
        .await
        .expect("fake diarization model loads")
    }

    fn audio(samples: usize) -> Vec<f32> {
        vec![0.1; samples]
    }

    #[tokio::test]
    async fn diarizes_audio_into_speaker_turns() {
        let engine = FakeEngine::new("nemotron_3_diar");
        let diarizer = diarizer(Arc::clone(&engine)).await;

        let turns = diarizer
            .diarize(audio(SAMPLE_RATE as usize))
            .await
            .expect("diarization succeeds");

        assert_eq!(turns.len(), 1);
        assert_eq!(turns[0].speaker_id, "SPEAKER_00");
        assert!(engine.ran());
    }

    #[tokio::test]
    async fn reports_the_pipeline_contract_sample_rate_and_full_chunk() {
        let engine = FakeEngine::new("nemotron_3_diar");
        let diarizer = diarizer(Arc::clone(&engine)).await;

        diarizer.diarize(audio(8_000)).await.unwrap();

        let calls = engine.calls();
        assert_eq!(calls.sample_rates, vec![SAMPLE_RATE]);
        assert_eq!(calls.audio_lengths, vec![8_000]);
    }

    #[tokio::test]
    async fn asks_the_model_about_diar_offline() {
        let engine = FakeEngine::new("nemotron_3_diar");
        let diarizer = diarizer(Arc::clone(&engine)).await;

        assert_eq!(diarizer.model_name(), "nemotron-3-diarization-bf16");
        assert_eq!(
            engine.calls().supports,
            vec![(DIAR_TASK.to_string(), OFFLINE_MODE.to_string())]
        );
    }

    #[tokio::test]
    async fn a_model_without_offline_diarization_support_is_rejected_with_an_actionable_message() {
        // The audio.cpp catalog holds only the Nemotron diarization model;
        // any other artifact (e.g. a stray GGUF from an older install) must
        // be rejected by the capability gate, not loaded.
        let mut engine = FakeEngine::new("some_other_family");
        Arc::get_mut(&mut engine).unwrap().supports_diar_offline = false;

        let error = AudioCppDiarizer::load(
            engine,
            "not-a-diarization-model",
            PathBuf::from("/models/not-a-diarization-model.gguf"),
        )
        .await
        .unwrap_err();

        match &error {
            DiarizerLoadError::UnsupportedCapability { model, family } => {
                assert_eq!(model, "not-a-diarization-model");
                assert_eq!(family, "some_other_family");
            }
            other => panic!("expected UnsupportedCapability, got {other:?}"),
        }
        let text = error.to_string();
        assert!(text.contains("Nemotron"), "text: {text}");
    }

    #[tokio::test]
    async fn audio_shorter_than_the_engine_minimum_is_rejected_without_running_the_engine() {
        let engine = FakeEngine::new("nemotron_3_diar");
        let diarizer = diarizer(Arc::clone(&engine)).await;

        let error = diarizer.diarize(audio(MIN_SAMPLES - 1)).await.unwrap_err();

        match error {
            DiarizationError::EngineFailed(detail) => {
                assert!(detail.contains("too short"), "detail: {detail}");
            }
            other => panic!("expected EngineFailed, got {other:?}"),
        }
        assert!(!engine.ran());
    }

    #[tokio::test]
    async fn an_unloaded_model_reports_model_not_loaded() {
        let engine = FakeEngine::new("nemotron_3_diar");
        let diarizer = diarizer(Arc::clone(&engine)).await;

        diarizer.unload();

        assert!(matches!(
            diarizer.diarize(audio(SAMPLE_RATE as usize)).await,
            Err(DiarizationError::ModelNotLoaded)
        ));
        assert!(!engine.ran());
    }

    #[tokio::test]
    async fn engine_run_failures_become_engine_failed_with_the_native_detail() {
        let mut engine = FakeEngine::new("nemotron_3_diar");
        Arc::get_mut(&mut engine).unwrap().run_error = Some("out of memory".to_string());

        let diarizer = diarizer(engine).await;
        let error = diarizer
            .diarize(audio(SAMPLE_RATE as usize))
            .await
            .unwrap_err();

        match error {
            DiarizationError::EngineFailed(detail) => {
                assert!(detail.contains("out of memory"), "detail: {detail}");
            }
            other => panic!("expected EngineFailed, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn every_native_call_runs_on_the_single_thread_that_loaded_the_model() {
        let engine = FakeEngine::new("nemotron_3_diar");
        let diarizer = Arc::new(diarizer(Arc::clone(&engine)).await);

        let first = {
            let diarizer = Arc::clone(&diarizer);
            tokio::spawn(async move { diarizer.diarize(audio(1_600)).await })
        };
        let second = {
            let diarizer = Arc::clone(&diarizer);
            tokio::spawn(async move { diarizer.diarize(audio(1_600)).await })
        };
        first.await.unwrap().unwrap();
        second.await.unwrap().unwrap();

        let calls = engine.calls();
        let load_thread = calls.load_thread.expect("model was loaded");
        assert!(
            calls.threads.iter().all(|thread| *thread == load_thread),
            "all native calls must stay on the load thread; saw {:?} vs {:?}",
            calls.threads,
            load_thread
        );
    }
}
