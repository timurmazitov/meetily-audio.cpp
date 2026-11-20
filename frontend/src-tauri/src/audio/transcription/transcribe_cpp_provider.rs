// audio/transcription/transcribe_cpp_provider.rs
//
// transcribeCpp transcription provider: wraps the transcribe.cpp engine
// seam behind the existing `TranscriptionProvider` trait.
//
// Design notes:
// - The transcribe.cpp catalog is ASR-only (every catalogued GGUF can
//   transcribe offline), so unlike audio.cpp there is no family/capability
//   probe: a model that loads can transcribe, and `ModelNotReady` guards
//   the on-disk states before a load is ever attempted.
// - transcribe.cpp sessions are not thread-safe and the 0.x library
//   forbids concurrent compute across sessions of one model: the provider
//   therefore owns a dedicated session thread; the model and its session
//   are created on that thread and never touched from anywhere else.
//   Requests arrive over a channel and are awaited without blocking the
//   async runtime. (`spawn_blocking` is not enough here: its pool may
//   hand a later request to a different thread, which the library forbids
//   for a live handle.)
// - Input is the pipeline contract: 16 kHz mono f32 (transcribe.cpp v0.4.2
//   accepts no other rate; no resampler is linked into the library).
// - Language hints are forwarded to `transcribe_run` as-is (models that do
//   not consume a hint ignore it natively). Transcription is always
//   same-language: the translate-only mode the legacy Whisper engine had is
//   gone with it.
// - Transcript timestamps stay owned by the worker: every provider result
//   is attributed to the VAD speech segment that produced it.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use log::info;
use serde::Serialize;

use super::engine::TRANSCRIBE_CPP_PROVIDER;
use super::provider::{TranscriptResult, TranscriptionError, TranscriptionProvider};
use crate::transcribe_cpp_engine::engine::{
    TranscribeCppEngine, TranscribeCppModel, TranscriptionOutput, MIN_SAMPLES,
};
use crate::transcribe_cpp_engine::error::TranscribeCppError;
use crate::transcribe_cpp_engine::session_thread::{SessionLoadError, SessionTask, SessionThread};

/// Failure of a model load, with a user-facing message for each case.
///
/// There is no capability variant: the catalog is ASR-only, so a model
/// that loads can transcribe (compare audio.cpp, where diarization models
/// share the catalog and must be refused).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub enum ProviderLoadError {
    /// The native engine refused or failed to load the model file.
    Engine(String),
}

impl std::fmt::Display for ProviderLoadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Engine(detail) => write!(f, "Failed to load transcribe.cpp model: {detail}"),
        }
    }
}

impl std::error::Error for ProviderLoadError {}

/// Maps a session-thread load failure onto the provider's user-facing error.
fn load_error(error: SessionLoadError) -> ProviderLoadError {
    ProviderLoadError::Engine(error.detail)
}

/// The transcription work a [`SessionThread`] runs: one offline ASR run.
struct AsrTask;

impl SessionTask for AsrTask {
    type Output = TranscriptionOutput;

    fn run(
        model: &mut dyn TranscribeCppModel,
        audio: &[f32],
        sample_rate: i32,
        language: Option<&str>,
    ) -> Result<TranscriptionOutput, TranscribeCppError> {
        model.transcribe_asr(audio, sample_rate, language)
    }
}

type Session = SessionThread<TranscriptionOutput>;

/// transcribeCpp provider backed by the transcribe.cpp engine.
///
/// The engine model and its session live on a dedicated thread
/// ([`SessionThread`]); this struct holds only the channel to it.
pub struct TranscribeCppProvider {
    model_name: String,
    /// The runtime backend the model actually bound to ("cpu", "vulkan",
    /// ...), for detecting a Vulkan→CPU fallback (ticket 06 warns).
    backend: String,
    session: Mutex<Option<Session>>,
    loaded: AtomicBool,
}

impl std::fmt::Debug for TranscribeCppProvider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TranscribeCppProvider")
            .field("model_name", &self.model_name)
            .field("backend", &self.backend)
            .field("model_loaded", &self.model_is_loaded())
            .finish()
    }
}

impl TranscribeCppProvider {
    /// Loads `path` and prepares it for offline transcription.
    ///
    /// The model is created on the session thread that will own it, so the
    /// native handle never migrates between threads.
    ///
    /// Blocking: the native model load is slow and must run off the async
    /// runtime (callers use [`Self::load`] for that).
    pub fn load_blocking(
        engine: Arc<dyn TranscribeCppEngine>,
        model_name: impl Into<String>,
        path: &Path,
    ) -> Result<Self, ProviderLoadError> {
        let model_name = model_name.into();
        let session =
            Session::spawn::<AsrTask>(engine, path, "transcribecpp-session").map_err(load_error)?;

        info!(
            "transcribeCpp model '{}' loaded (backend '{}')",
            model_name, session.backend
        );

        Ok(Self {
            model_name,
            backend: session.backend.clone(),
            session: Mutex::new(Some(session)),
            loaded: AtomicBool::new(true),
        })
    }

    /// Async wrapper around [`Self::load_blocking`].
    pub async fn load(
        engine: Arc<dyn TranscribeCppEngine>,
        model_name: impl Into<String>,
        path: PathBuf,
    ) -> Result<Self, ProviderLoadError> {
        let model_name = model_name.into();
        tokio::task::spawn_blocking(move || Self::load_blocking(engine, model_name, &path))
            .await
            .map_err(|error| ProviderLoadError::Engine(format!("load task failed: {error}")))?
    }

    /// The runtime backend the model bound to ("cpu", "vulkan", ...).
    pub fn backend(&self) -> &str {
        &self.backend
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
            info!("transcribeCpp model '{}' unloaded", self.model_name);
        }
    }

    async fn transcribe_on_session_thread(
        &self,
        audio: Vec<f32>,
        language: Option<String>,
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
            .request(audio, language)
            .await
            .map_err(|error| TranscriptionError::EngineFailed(error.to_string()))
    }
}

impl Drop for TranscribeCppProvider {
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
impl TranscriptionProvider for TranscribeCppProvider {
    async fn transcribe(
        &self,
        audio: Vec<f32>,
        language: Option<String>,
    ) -> std::result::Result<TranscriptResult, TranscriptionError> {
        if audio.len() < MIN_SAMPLES {
            return Err(TranscriptionError::AudioTooShort {
                samples: audio.len(),
                minimum: MIN_SAMPLES,
            });
        }

        if !self.model_is_loaded() {
            return Err(TranscriptionError::ModelNotLoaded);
        }

        // "auto" means autodetect, which is what a NULL language hint is in
        // the native run params; anything else travels to transcribe_run
        // and models that do not consume hints ignore it natively.
        let hint = language.filter(|language| language != "auto");

        // The model and its session live on one dedicated thread for their
        // whole lifetime; await its result without blocking the async runtime.
        let output = self.transcribe_on_session_thread(audio, hint).await?;

        Ok(TranscriptResult {
            text: transcript_text(&output),
            // transcribe.cpp reports no per-segment confidence in v0.4.2.
            confidence: None,
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
        TRANSCRIBE_CPP_PROVIDER
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

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

    use super::*;
    use crate::transcribe_cpp_engine::engine::SAMPLE_RATE;
    use crate::transcribe_cpp_engine::error::Status;
    use crate::transcribe_cpp_engine::wrapper::Segment;

    /// What the fake model observed, shared with the test.
    #[derive(Default)]
    struct Calls {
        sample_rates: Vec<i32>,
        audio_lengths: Vec<usize>,
        /// Language hints passed to each run, in order (None = autodetect).
        languages: Vec<Option<String>>,
        /// Thread ids of every native call, in order. transcribe.cpp
        /// sessions are not thread-safe: one session must be serviced by
        /// one thread.
        threads: Vec<std::thread::ThreadId>,
        /// Thread the model was loaded on.
        load_thread: Option<std::thread::ThreadId>,
    }

    /// Fake engine behind the FFI seam: no native library in tests.
    struct FakeEngine {
        backend: String,
        load_error: Option<TranscribeCppError>,
        run_error: Option<TranscribeCppError>,
        output: TranscriptionOutput,
        loads: AtomicUsize,
        ran: Arc<AtomicBool>,
        calls: Arc<Mutex<Calls>>,
    }

    impl FakeEngine {
        fn new(backend: &str) -> Arc<Self> {
            Arc::new(Self {
                backend: backend.to_string(),
                load_error: None,
                run_error: None,
                output: TranscriptionOutput {
                    backend: backend.to_string(),
                    text: " привет мир ".to_string(),
                    segments: vec![Segment {
                        start_ms: 0,
                        end_ms: 1_000,
                        text: "привет мир".to_string(),
                        speaker_id: 0,
                    }],
                    detected_language: Some("ru".to_string()),
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

    impl TranscribeCppEngine for FakeEngine {
        fn load_model(
            &self,
            _path: &Path,
        ) -> Result<Box<dyn TranscribeCppModel>, TranscribeCppError> {
            self.loads.fetch_add(1, Ordering::SeqCst);
            self.calls.lock().unwrap().load_thread = Some(std::thread::current().id());
            if let Some(error) = &self.load_error {
                return Err(TranscribeCppError::Native {
                    status: error_status(error),
                    detail: Some("fake load failure".to_string()),
                    context: "fake",
                });
            }
            Ok(Box::new(FakeModel {
                backend: self.backend.clone(),
                run_error: self.run_error.as_ref().map(error_display),
                output: self.output.clone(),
                ran: Arc::clone(&self.ran),
                calls: Arc::clone(&self.calls),
            }))
        }
    }

    struct FakeModel {
        backend: String,
        run_error: Option<String>,
        output: TranscriptionOutput,
        ran: Arc<AtomicBool>,
        calls: Arc<Mutex<Calls>>,
    }

    impl TranscribeCppModel for FakeModel {
        fn backend(&self) -> String {
            self.backend.clone()
        }

        fn transcribe_asr(
            &mut self,
            audio: &[f32],
            sample_rate: i32,
            language: Option<&str>,
        ) -> Result<TranscriptionOutput, TranscribeCppError> {
            let mut calls = self.calls.lock().unwrap();
            calls.sample_rates.push(sample_rate);
            calls.audio_lengths.push(audio.len());
            calls.languages.push(language.map(str::to_string));
            calls.threads.push(std::thread::current().id());
            drop(calls);

            self.ran.store(true, Ordering::SeqCst);
            if let Some(detail) = &self.run_error {
                return Err(TranscribeCppError::Native {
                    status: Status::Backend,
                    detail: Some(detail.clone()),
                    context: "transcribe_run",
                });
            }
            Ok(self.output.clone())
        }
    }

    fn error_status(error: &TranscribeCppError) -> Status {
        match error {
            TranscribeCppError::Native { status, .. } => *status,
            _ => Status::Backend,
        }
    }

    fn error_display(error: &TranscribeCppError) -> String {
        error.to_string()
    }

    fn engine_failure(detail: &str) -> TranscribeCppError {
        TranscribeCppError::Native {
            status: Status::Backend,
            detail: Some(detail.to_string()),
            context: "transcribe_run",
        }
    }

    async fn provider(engine: Arc<FakeEngine>) -> TranscribeCppProvider {
        TranscribeCppProvider::load(
            engine,
            "parakeet-ultra-Q8_0",
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
        let engine = FakeEngine::new("cpu");
        let provider = provider(Arc::clone(&engine)).await;

        let result = provider
            .transcribe(audio(SAMPLE_RATE as usize), None)
            .await
            .expect("transcription succeeds");

        assert_eq!(result.text, "привет мир");
        assert_eq!(
            result.confidence, None,
            "transcribe.cpp reports no confidence"
        );
        assert!(!result.is_partial);
        assert!(engine.ran());
        assert_eq!(provider.provider_name(), "transcribeCpp");
        assert_eq!(provider.backend(), "cpu");
        assert_eq!(
            provider.get_current_model().await.as_deref(),
            Some("parakeet-ultra-Q8_0")
        );
        assert!(provider.is_model_loaded().await);
    }

    #[tokio::test]
    async fn reports_the_pipeline_contract_sample_rate_and_full_chunk() {
        let engine = FakeEngine::new("cpu");
        let provider = provider(Arc::clone(&engine)).await;

        provider
            .transcribe(audio(8_000), Some("auto".to_string()))
            .await
            .unwrap();

        let calls = engine.calls();
        assert_eq!(calls.sample_rates, vec![SAMPLE_RATE]);
        assert_eq!(calls.audio_lengths, vec![8_000]);
        // "auto" means autodetect on the native side: no hint travels.
        assert_eq!(calls.languages, vec![None]);
    }

    #[tokio::test]
    async fn a_language_hint_travels_to_the_native_run() {
        let engine = FakeEngine::new("cpu");
        let provider = provider(Arc::clone(&engine)).await;

        provider
            .transcribe(audio(1_600), Some("ru".to_string()))
            .await
            .unwrap();

        assert_eq!(engine.calls().languages, vec![Some("ru".to_string())]);
    }

    #[tokio::test]
    async fn engine_load_failures_surface_a_user_facing_message() {
        let mut engine = FakeEngine::new("cpu");
        Arc::get_mut(&mut engine).unwrap().load_error =
            Some(engine_failure("GGUF header is truncated"));

        let error = TranscribeCppProvider::load(
            engine,
            "parakeet-ultra-Q8_0",
            PathBuf::from("/models/broken.gguf"),
        )
        .await
        .unwrap_err();

        let text = error.to_string();
        assert!(
            text.contains("Failed to load transcribe.cpp model"),
            "text: {text}"
        );
        assert!(text.contains("fake load failure"), "text: {text}");
        assert!(text.contains("TRANSCRIBE_ERR_BACKEND"), "text: {text}");
    }

    #[tokio::test]
    async fn engine_run_failures_become_engine_failed_with_the_native_detail() {
        let mut engine = FakeEngine::new("cpu");
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
        let engine = FakeEngine::new("cpu");
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
        let engine = FakeEngine::new("cpu");
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
    async fn unloading_joins_the_session_thread_instead_of_leaking_it() {
        // The session thread owns the native model: unload must end and join
        // it, or a reloaded model would double the process memory. The
        // timeout proves the join cannot hang on a live handle clone.
        let engine = FakeEngine::new("cpu");
        let provider = provider(Arc::clone(&engine)).await;
        provider
            .transcribe(audio(1_600), None)
            .await
            .expect("first request runs");

        tokio::time::timeout(
            std::time::Duration::from_secs(10),
            tokio::task::spawn_blocking(move || provider.unload()),
        )
        .await
        .expect("unload must not hang")
        .expect("unload task");
    }

    #[tokio::test]
    async fn segment_text_is_used_when_the_engine_reports_no_full_transcript() {
        let mut engine = FakeEngine::new("cpu");
        {
            let inner = Arc::get_mut(&mut engine).unwrap();
            inner.output.text = "  ".to_string();
            inner.output.segments = vec![
                Segment {
                    start_ms: 0,
                    end_ms: 500,
                    text: "добрый ".to_string(),
                    speaker_id: 0,
                },
                Segment {
                    start_ms: 500,
                    end_ms: 1_000,
                    text: "день".to_string(),
                    speaker_id: 0,
                },
            ];
        }

        let provider = provider(Arc::clone(&engine)).await;
        let result = provider
            .transcribe(audio(SAMPLE_RATE as usize), None)
            .await
            .unwrap();

        assert_eq!(result.text, "добрый день");
        assert_eq!(result.confidence, None);
    }

    #[tokio::test]
    async fn a_silent_run_yields_empty_text_and_no_confidence() {
        let mut engine = FakeEngine::new("cpu");
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
        // transcribe.h threading contract: sessions are not thread-safe and
        // concurrent compute across sessions of one model is forbidden.
        // One session for this provider must therefore stay on exactly one
        // thread, including the model load itself.
        let engine = FakeEngine::new("cpu");
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
        let engine = FakeEngine::new("cpu");
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
