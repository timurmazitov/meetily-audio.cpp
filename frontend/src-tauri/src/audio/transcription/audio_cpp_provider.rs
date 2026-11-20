// audio/transcription/audio_cpp_provider.rs
//
// audioCpp transcription provider: wraps the audio.cpp engine seam behind the
// existing `TranscriptionProvider` trait.
//
// Design notes:
// - Models are loaded family-agnostically: the GGUF embeds its own spec, so
//   the provider never guesses a family and only asks the model what it can do
//   (`audiocpp_model_supports(model, "asr", "offline")`).
// - audio.cpp handles are not thread-safe individually: one session is
//   serviced by exactly one thread for its whole lifetime. The provider
//   therefore owns a dedicated session thread; the model and its session are
//   created on that thread and never touched from anywhere else. Requests
//   arrive over a channel and are awaited without blocking the async runtime.
//   (`spawn_blocking` is not enough here: its pool may hand a later request to
//   a different thread, which the library forbids for a live handle.)
// - Input is the pipeline contract: 16 kHz mono f32. GigaAM resamples
//   internally, so the provider does not resample again.
// - Transcript timestamps stay owned by the worker: every provider result is
//   attributed to the VAD speech segment that produced it, exactly as for
//   Whisper and Parakeet.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use log::info;
use serde::Serialize;

use super::engine::AUDIO_CPP_PROVIDER;
use super::provider::{TranscriptResult, TranscriptionError, TranscriptionProvider};
use crate::audio_cpp_engine::engine::{
    AudioCppEngine, AudioCppModel, TranscriptionOutput, ASR_TASK, MIN_SAMPLES, OFFLINE_MODE,
};
use crate::audio_cpp_engine::error::AudioCppError;
use crate::audio_cpp_engine::session_thread::{SessionLoadError, SessionTask, SessionThread};

/// Failure of a model load, with a user-facing message for each case.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub enum ProviderLoadError {
    /// The native engine refused or failed to load the model file.
    Engine(String),
    /// The model loaded but cannot run offline speech recognition.
    UnsupportedCapability { model: String, family: String },
}

impl std::fmt::Display for ProviderLoadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Engine(detail) => write!(f, "Failed to load audio.cpp model: {detail}"),
            Self::UnsupportedCapability { model, family } => write!(
                f,
                "Model '{}' (family '{}') cannot transcribe audio offline. Choose a transcription model such as GigaAM v3 in Settings.",
                model, family
            ),
        }
    }
}

impl std::error::Error for ProviderLoadError {}

/// Maps a session-thread load failure onto the provider's user-facing error.
fn load_error(error: SessionLoadError) -> ProviderLoadError {
    ProviderLoadError::Engine(error.detail)
}

/// The transcription work a [`SessionThread`] runs: `asr` / `offline`.
struct AsrTask;

impl SessionTask for AsrTask {
    type Output = TranscriptionOutput;

    const TASK: &'static str = ASR_TASK;

    fn run(
        model: &mut dyn AudioCppModel,
        audio: &[f32],
        sample_rate: i32,
    ) -> Result<TranscriptionOutput, AudioCppError> {
        model.transcribe_asr(audio, sample_rate)
    }
}

type Session = SessionThread<TranscriptionOutput>;

/// audioCpp provider backed by the audio.cpp engine.
///
/// The engine model and its session live on a dedicated thread
/// ([`SessionThread`]); this struct holds only the channel to it.
pub struct AudioCppProvider {
    model_name: String,
    family: String,
    session: Mutex<Option<Session>>,
    loaded: AtomicBool,
}

impl std::fmt::Debug for AudioCppProvider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AudioCppProvider")
            .field("model_name", &self.model_name)
            .field("family", &self.family)
            .field("model_loaded", &self.model_is_loaded())
            .finish()
    }
}

impl AudioCppProvider {
    /// Loads `path` and prepares it for offline transcription.
    ///
    /// The model is created on the session thread that will own it, so the
    /// native handle never migrates between threads.
    ///
    /// Blocking: the native model load is slow and must run off the async
    /// runtime (callers use [`Self::load`] for that).
    pub fn load_blocking(
        engine: Arc<dyn AudioCppEngine>,
        model_name: impl Into<String>,
        path: &Path,
    ) -> Result<Self, ProviderLoadError> {
        let model_name = model_name.into();
        let session =
            Session::spawn::<AsrTask>(engine, path, "audiocpp-session").map_err(load_error)?;

        // Family-agnostic capability gate: never assume a family, ask the
        // model what it supports, on the thread that owns it.
        let family = session.family.clone();
        if !session.supports_offline {
            // No provider is constructed, so release the model here.
            session.shutdown();
            return Err(ProviderLoadError::UnsupportedCapability {
                model: model_name,
                family,
            });
        }

        info!(
            "audioCpp model '{}' loaded (family '{}'), supports {} / {}",
            model_name, family, ASR_TASK, OFFLINE_MODE
        );

        Ok(Self {
            model_name,
            family,
            session: Mutex::new(Some(session)),
            loaded: AtomicBool::new(true),
        })
    }

    /// Async wrapper around [`Self::load_blocking`].
    pub async fn load(
        engine: Arc<dyn AudioCppEngine>,
        model_name: impl Into<String>,
        path: PathBuf,
    ) -> Result<Self, ProviderLoadError> {
        let model_name = model_name.into();
        tokio::task::spawn_blocking(move || Self::load_blocking(engine, model_name, &path))
            .await
            .map_err(|error| ProviderLoadError::Engine(format!("load task failed: {error}")))?
    }

    /// Family auto-detected from the loaded model's GGUF metadata.
    pub fn family(&self) -> &str {
        &self.family
    }

    /// The configured model id (stable catalog name).
    pub fn model_name(&self) -> &str {
        &self.model_name
    }

    /// Whether the engine handle is still held.
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
            info!("audioCpp model '{}' unloaded", self.model_name);
        }
    }

    async fn transcribe_on_session_thread(
        &self,
        audio: Vec<f32>,
    ) -> Result<TranscriptionOutput, TranscriptionError> {
        // Clone the Send handle rather than holding the std mutex across the
        // await.
        let handle = {
            let guard = self
                .session
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            match guard.as_ref() {
                Some(session) => session.handle(),
                None => return Err(TranscriptionError::ModelNotLoaded),
            }
        };

        handle
            .request(audio)
            .await
            .map_err(|error| TranscriptionError::EngineFailed(error.to_string()))
    }
}

impl Drop for AudioCppProvider {
    fn drop(&mut self) {
        // Release the session thread even if the provider was never explicitly
        // unloaded: the thread owns the native model and must end with it.
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

#[async_trait]
impl TranscriptionProvider for AudioCppProvider {
    async fn transcribe(
        &self,
        audio: Vec<f32>,
        language: Option<String>,
    ) -> std::result::Result<TranscriptResult, TranscriptionError> {
        if let Some(ref hint) = language {
            if hint != "auto" {
                // GigaAM is a Russian ASR model and ignores language hints.
                // This runs once per VAD segment, so it is a hot path: keep
                // the diagnostic in the debug-only perf macro.
                perf_debug!(
                    "audioCpp ignores language hint '{}': the model transcribes in its trained language",
                    hint
                );
            }
        }

        if audio.len() < MIN_SAMPLES {
            return Err(TranscriptionError::AudioTooShort {
                samples: audio.len(),
                minimum: MIN_SAMPLES,
            });
        }

        if !self.model_is_loaded() {
            return Err(TranscriptionError::ModelNotLoaded);
        }

        // The model and its session live on one dedicated thread for their
        // whole lifetime; await its result without blocking the async runtime.
        let output = self.transcribe_on_session_thread(audio).await?;

        Ok(TranscriptResult {
            text: transcript_text(&output),
            confidence: average_confidence(&output),
            is_partial: false,
        })
    }

    async fn is_model_loaded(&self) -> bool {
        self.model_is_loaded()
    }

    async fn get_current_model(&self) -> Option<String> {
        self.model_is_loaded().then(|| self.model_name.clone())
    }

    fn provider_name(&self) -> &'static str {
        AUDIO_CPP_PROVIDER
    }
}

/// The transcript text of one engine run: the engine's full text when it has
/// one, otherwise the segment texts in order.
fn transcript_text(output: &TranscriptionOutput) -> String {
    let text = output.text.trim();
    if !text.is_empty() {
        return text.to_string();
    }
    output
        .segments
        .iter()
        .map(|segment| segment.text.trim())
        .filter(|segment| !segment.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

/// Mean segment confidence, or `None` when the engine reported none.
fn average_confidence(output: &TranscriptionOutput) -> Option<f32> {
    let scores: Vec<f32> = output
        .segments
        .iter()
        .map(|segment| segment.confidence)
        .filter(|score| score.is_finite() && *score > 0.0)
        .collect();
    if scores.is_empty() {
        return None;
    }
    let mean = scores.iter().sum::<f32>() / scores.len() as f32;
    Some(mean.clamp(0.0, 1.0))
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

    use super::*;
    use crate::audio_cpp_engine::engine::{AudioCppModel, SAMPLE_RATE};
    use crate::audio_cpp_engine::error::Status;
    use crate::audio_cpp_engine::wrapper::Segment;

    /// What the fake model observed, shared with the test.
    #[derive(Default)]
    struct Calls {
        supports: Vec<(String, String)>,
        sample_rates: Vec<i32>,
        audio_lengths: Vec<usize>,
        /// Thread ids of every native call, in order. audio.cpp handles are
        /// not thread-safe: one session must be serviced by one thread.
        threads: Vec<std::thread::ThreadId>,
        /// Thread the model was loaded on.
        load_thread: Option<std::thread::ThreadId>,
    }

    /// Fake engine behind the FFI seam: no native library in tests.
    struct FakeEngine {
        family: String,
        supports_asr_offline: bool,
        supports_error: bool,
        load_error: Option<AudioCppError>,
        run_error: Option<AudioCppError>,
        output: TranscriptionOutput,
        loads: AtomicUsize,
        ran: Arc<AtomicBool>,
        calls: Arc<Mutex<Calls>>,
    }

    impl FakeEngine {
        fn new(family: &str) -> Arc<Self> {
            Arc::new(Self {
                family: family.to_string(),
                supports_asr_offline: true,
                supports_error: false,
                load_error: None,
                run_error: None,
                output: TranscriptionOutput {
                    family: family.to_string(),
                    text: " привет мир ".to_string(),
                    segments: vec![Segment {
                        start_sample: 0,
                        end_sample: 16_000,
                        confidence: 0.9,
                        text: "привет мир".to_string(),
                    }],
                },
                loads: AtomicUsize::new(0),
                ran: Arc::new(AtomicBool::new(false)),
                calls: Arc::new(Mutex::new(Calls::default())),
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
            self.loads.fetch_add(1, Ordering::SeqCst);
            self.calls.lock().unwrap().load_thread = Some(std::thread::current().id());
            if let Some(error) = &self.load_error {
                return Err(AudioCppError::Native {
                    status: error_status(error),
                    detail: Some("fake load failure".to_string()),
                    context: "fake",
                });
            }
            Ok(Box::new(FakeModel {
                family: self.family.clone(),
                supports_asr_offline: self.supports_asr_offline,
                supports_error: self.supports_error,
                run_error: self.run_error.as_ref().map(error_display),
                output: self.output.clone(),
                ran: Arc::clone(&self.ran),
                calls: Arc::clone(&self.calls),
            }))
        }
    }

    struct FakeModel {
        family: String,
        supports_asr_offline: bool,
        supports_error: bool,
        run_error: Option<String>,
        output: TranscriptionOutput,
        ran: Arc<AtomicBool>,
        calls: Arc<Mutex<Calls>>,
    }

    impl AudioCppModel for FakeModel {
        fn family(&self) -> String {
            self.family.clone()
        }

        fn supports(&self, task: &str, mode: &str) -> Result<bool, AudioCppError> {
            let mut calls = self.calls.lock().unwrap();
            calls.supports.push((task.to_string(), mode.to_string()));
            calls.threads.push(std::thread::current().id());
            drop(calls);
            if self.supports_error {
                return Err(AudioCppError::Native {
                    status: Status::Runtime,
                    detail: None,
                    context: "audiocpp_model_supports",
                });
            }
            Ok(task == ASR_TASK && mode == OFFLINE_MODE && self.supports_asr_offline)
        }

        fn transcribe_asr(
            &mut self,
            audio: &[f32],
            sample_rate: i32,
        ) -> Result<TranscriptionOutput, AudioCppError> {
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

        fn diarize(
            &mut self,
            _audio: &[f32],
            _sample_rate: i32,
        ) -> Result<crate::audio_cpp_engine::DiarizationOutput, AudioCppError> {
            unimplemented!("the transcription provider never diarizes")
        }
    }

    fn error_status(error: &AudioCppError) -> Status {
        match error {
            AudioCppError::Native { status, .. } => *status,
            AudioCppError::IncompatibleAbi { .. } => Status::Runtime,
            _ => Status::LoadFailed,
        }
    }

    fn error_display(error: &AudioCppError) -> String {
        error.to_string()
    }

    fn engine_failure(detail: &str) -> AudioCppError {
        AudioCppError::Native {
            status: Status::Runtime,
            detail: Some(detail.to_string()),
            context: "audiocpp_session_run",
        }
    }

    async fn provider(engine: Arc<FakeEngine>) -> AudioCppProvider {
        AudioCppProvider::load(
            engine,
            "gigaam-v3-e2e-ctc-f16",
            PathBuf::from("/models/m.gguf"),
        )
        .await
        .expect("fake model loads")
    }

    fn audio(samples: usize) -> Vec<f32> {
        vec![0.1; samples]
    }

    #[tokio::test]
    async fn transcribes_audio_through_the_trait_contract() {
        let engine = FakeEngine::new("gigaam_asr");
        let provider = provider(Arc::clone(&engine)).await;

        let result = provider
            .transcribe(audio(SAMPLE_RATE as usize), None)
            .await
            .expect("transcription succeeds");

        assert_eq!(result.text, "привет мир");
        assert_eq!(result.confidence, Some(0.9));
        assert!(!result.is_partial);
        assert!(engine.ran());
    }

    #[tokio::test]
    async fn reports_the_pipeline_contract_sample_rate_and_full_chunk() {
        let engine = FakeEngine::new("gigaam_asr");
        let provider = provider(Arc::clone(&engine)).await;

        provider
            .transcribe(audio(8_000), Some("auto".to_string()))
            .await
            .unwrap();

        let calls = engine.calls();
        assert_eq!(calls.sample_rates, vec![SAMPLE_RATE]);
        assert_eq!(calls.audio_lengths, vec![8_000]);
    }

    #[tokio::test]
    async fn auto_detects_the_family_and_asks_the_model_about_asr_offline() {
        let engine = FakeEngine::new("some_future_asr_family");
        let provider = provider(Arc::clone(&engine)).await;

        assert_eq!(provider.family(), "some_future_asr_family");
        assert_eq!(provider.provider_name(), "audioCpp");
        assert_eq!(
            engine.calls().supports,
            vec![(ASR_TASK.to_string(), OFFLINE_MODE.to_string())]
        );
        assert_eq!(
            provider.get_current_model().await.as_deref(),
            Some("gigaam-v3-e2e-ctc-f16")
        );
        assert!(provider.is_model_loaded().await);
    }

    #[tokio::test]
    async fn a_model_without_offline_asr_is_rejected_with_an_actionable_message() {
        let mut engine = FakeEngine::new("nemotron_3_diar");
        Arc::get_mut(&mut engine).unwrap().supports_asr_offline = false;

        let error = AudioCppProvider::load(
            engine,
            "nemotron-3-diarization-bf16",
            PathBuf::from("/models/diar.gguf"),
        )
        .await
        .unwrap_err();

        assert!(matches!(
            error,
            ProviderLoadError::UnsupportedCapability { .. }
        ));
        let text = error.to_string();
        assert!(text.contains("nemotron_3_diar"), "text: {text}");
        assert!(text.contains("GigaAM"), "text: {text}");
    }

    #[tokio::test]
    async fn a_rejected_model_releases_its_session_thread() {
        // The unsupported path constructs no provider, so it must shut the
        // session thread down itself, or a rejected model would hold its
        // weights until process exit.
        let mut engine = FakeEngine::new("nemotron_3_diar");
        Arc::get_mut(&mut engine).unwrap().supports_asr_offline = false;

        let result = tokio::time::timeout(
            std::time::Duration::from_secs(10),
            AudioCppProvider::load(
                engine,
                "nemotron-3-diarization-bf16",
                PathBuf::from("/models/diar.gguf"),
            ),
        )
        .await
        .expect("a rejected model must not hang on its session thread");

        assert!(matches!(
            result,
            Err(ProviderLoadError::UnsupportedCapability { .. })
        ));
    }

    #[tokio::test]
    async fn a_capability_query_failure_is_reported_as_a_load_failure() {
        let mut engine = FakeEngine::new("gigaam_asr");
        Arc::get_mut(&mut engine).unwrap().supports_error = true;

        let error = AudioCppProvider::load(
            engine,
            "gigaam-v3-e2e-ctc-f16",
            PathBuf::from("/models/m.gguf"),
        )
        .await
        .unwrap_err();

        assert!(matches!(error, ProviderLoadError::Engine(_)));
        assert!(error.to_string().contains("Failed to load audio.cpp model"));
    }

    #[tokio::test]
    async fn engine_load_failures_surface_a_user_facing_message() {
        let mut engine = FakeEngine::new("gigaam_asr");
        Arc::get_mut(&mut engine).unwrap().load_error =
            Some(engine_failure("GGUF header is truncated"));

        let error = AudioCppProvider::load(
            engine,
            "gigaam-v3-e2e-ctc-f16",
            PathBuf::from("/models/broken.gguf"),
        )
        .await
        .unwrap_err();

        let text = error.to_string();
        assert!(
            text.contains("Failed to load audio.cpp model"),
            "text: {text}"
        );
        assert!(text.contains("AUDIOCPP_ERR_RUNTIME"), "text: {text}");
    }

    #[tokio::test]
    async fn engine_run_failures_become_engine_failed_with_the_native_detail() {
        let mut engine = FakeEngine::new("gigaam_asr");
        Arc::get_mut(&mut engine).unwrap().run_error = Some(engine_failure("out of memory"));

        let provider = provider(Arc::clone(&engine)).await;
        let error = provider
            .transcribe(audio(SAMPLE_RATE as usize), None)
            .await
            .unwrap_err();

        match error {
            TranscriptionError::EngineFailed(detail) => {
                assert!(detail.contains("out of memory"), "detail: {detail}");
            }
            other => panic!("expected EngineFailed, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn audio_shorter_than_the_engine_minimum_is_rejected_without_running_the_engine() {
        let engine = FakeEngine::new("gigaam_asr");
        let provider = provider(Arc::clone(&engine)).await;

        for samples in [0, 1, MIN_SAMPLES - 1] {
            let error = provider.transcribe(audio(samples), None).await.unwrap_err();
            match error {
                TranscriptionError::AudioTooShort {
                    samples: reported,
                    minimum,
                } => {
                    assert_eq!(reported, samples);
                    assert_eq!(minimum, MIN_SAMPLES);
                }
                other => panic!("expected AudioTooShort, got {other:?}"),
            }
        }
        assert!(!engine.ran());
    }

    #[tokio::test]
    async fn an_unloaded_model_reports_model_not_loaded_instead_of_failing_a_session() {
        let engine = FakeEngine::new("gigaam_asr");
        let provider = provider(Arc::clone(&engine)).await;

        provider.unload();

        assert!(!provider.is_model_loaded().await);
        assert_eq!(provider.get_current_model().await, None);
        assert!(matches!(
            provider
                .transcribe(audio(SAMPLE_RATE as usize), None)
                .await
                .unwrap_err(),
            TranscriptionError::ModelNotLoaded
        ));
        assert!(!engine.ran());
    }

    #[tokio::test]
    async fn segment_text_is_used_when_the_engine_reports_no_full_transcript() {
        let mut engine = FakeEngine::new("gigaam_asr");
        {
            let inner = Arc::get_mut(&mut engine).unwrap();
            inner.output.text = "  ".to_string();
            inner.output.segments = vec![
                Segment {
                    start_sample: 0,
                    end_sample: 8_000,
                    confidence: 0.8,
                    text: "добрый ".to_string(),
                },
                Segment {
                    start_sample: 8_000,
                    end_sample: 16_000,
                    confidence: 0.6,
                    text: "день".to_string(),
                },
            ];
        }

        let provider = provider(Arc::clone(&engine)).await;
        let result = provider
            .transcribe(audio(SAMPLE_RATE as usize), None)
            .await
            .unwrap();

        assert_eq!(result.text, "добрый день");
        let confidence = result.confidence.expect("confidence from segments");
        assert!((confidence - 0.7).abs() < 1e-5, "confidence: {confidence}");
    }

    #[tokio::test]
    async fn a_silent_run_yields_empty_text_and_no_confidence() {
        let mut engine = FakeEngine::new("gigaam_asr");
        {
            let inner = Arc::get_mut(&mut engine).unwrap();
            inner.output.text = String::new();
            inner.output.segments.clear();
        }

        let provider = provider(Arc::clone(&engine)).await;
        let result = provider
            .transcribe(audio(SAMPLE_RATE as usize), None)
            .await
            .unwrap();

        assert_eq!(result.text, "");
        assert_eq!(result.confidence, None);
        assert!(!result.is_partial);
    }

    #[tokio::test]
    async fn every_native_call_runs_on_the_single_thread_that_loaded_the_model() {
        // audio.cpp: "Handles are not thread-safe individually. Separate handles
        // may be used concurrently from separate threads." One session for this
        // provider must therefore stay on exactly one thread.
        let engine = FakeEngine::new("gigaam_asr");
        let provider = Arc::new(provider(Arc::clone(&engine)).await);

        let first = {
            let provider = Arc::clone(&provider);
            tokio::spawn(async move { provider.transcribe(audio(1_600), None).await })
        };
        let second = {
            let provider = Arc::clone(&provider);
            tokio::spawn(async move { provider.transcribe(audio(1_600), None).await })
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

    #[tokio::test]
    async fn concurrent_requests_are_serialized_on_one_handle() {
        let engine = FakeEngine::new("gigaam_asr");
        let provider = Arc::new(provider(Arc::clone(&engine)).await);

        let first = {
            let provider = Arc::clone(&provider);
            tokio::spawn(async move { provider.transcribe(audio(1_600), None).await })
        };
        let second = {
            let provider = Arc::clone(&provider);
            tokio::spawn(async move { provider.transcribe(audio(1_600), None).await })
        };

        assert!(first.await.unwrap().is_ok());
        assert!(second.await.unwrap().is_ok());
        assert_eq!(engine.calls().sample_rates.len(), 2);
    }
}
