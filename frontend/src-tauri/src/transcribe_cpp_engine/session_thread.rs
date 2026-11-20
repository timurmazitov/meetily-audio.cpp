//! The transcribe.cpp session thread: one native model handle serviced by
//! exactly one thread for its whole lifetime.
//!
//! transcribe.cpp sessions are not thread-safe and the 0.x library
//! forbids concurrent compute across sessions of one model
//! (transcribe.h threading contract). `tokio::task::spawn_blocking` is not
//! enough, because its pool may hand a later request to a different
//! thread while a handle is still live. App-level consumers of the engine
//! therefore go through this module (ticket 05's transcription provider):
//!
//! A [`SessionThread`] is parameterized by the work one request does on
//! the loaded model ([`SessionTask`]); the thread owns the model from load
//! to drop, and requests cross a channel and an awaited oneshot, so the
//! async runtime is never blocked.

use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;

use super::engine::{TranscribeCppEngine, TranscribeCppModel, SAMPLE_RATE};
use super::error::TranscribeCppError;

/// The work one session thread performs.
///
/// The associated output type is what a request resolves to.
pub trait SessionTask: Send + 'static {
    /// The result of one request.
    type Output: Send + 'static;

    /// Runs one request on the thread that owns the model. `language` is
    /// an optional BCP-47-ish hint.
    fn run(
        model: &mut dyn TranscribeCppModel,
        audio: &[f32],
        sample_rate: i32,
        language: Option<&str>,
    ) -> Result<Self::Output, TranscribeCppError>;
}

/// One request handed to the session thread.
struct SessionRequest<O> {
    audio: Vec<f32>,
    language: Option<String>,
    reply: tokio::sync::oneshot::Sender<Result<O, TranscribeCppError>>,
}

/// A loaded model plus the thread that owns it.
///
/// Dropping the sender ends the loop; [`SessionThread::shutdown`] additionally
/// joins the thread so the native model is freed deterministically.
pub struct SessionThread<O: Send + 'static> {
    sender: Sender<SessionRequest<O>>,
    join: Mutex<Option<JoinHandle<()>>>,
    /// The runtime backend the model actually bound to ("cpu", "vulkan",
    /// ...). Compare against the requested mode to detect a fallback.
    pub backend: String,
}

/// Why a session thread could not be started or could not load its model.
#[derive(Debug, Clone, PartialEq)]
pub struct SessionLoadError {
    /// The native engine refused or failed to load the model file.
    pub detail: String,
    /// True when the OS refused to spawn the thread.
    pub spawn_failure: bool,
}

impl std::fmt::Display for SessionLoadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.spawn_failure {
            write!(
                f,
                "Failed to start transcribe.cpp session thread: {}",
                self.detail
            )
        } else {
            write!(f, "Failed to load transcribe.cpp model: {}", self.detail)
        }
    }
}

impl std::error::Error for SessionLoadError {}

impl<O: Send + 'static> SessionThread<O> {
    /// Spawns the session thread and waits for the model load verdict, so
    /// a bad model is reported as a load failure, not a later request
    /// error.
    pub fn spawn<T: SessionTask<Output = O>>(
        engine: Arc<dyn TranscribeCppEngine>,
        path: &Path,
        thread_name: &str,
    ) -> Result<Self, SessionLoadError> {
        let path = path.to_path_buf();
        let (sender, receiver) = mpsc::channel::<SessionRequest<O>>();
        let (ready_tx, ready_rx) = mpsc::channel::<Result<String, String>>();

        let join = std::thread::Builder::new()
            .name(thread_name.to_string())
            .spawn(move || run_session_thread::<T>(engine, path, receiver, ready_tx))
            .map_err(|error| SessionLoadError {
                detail: error.to_string(),
                spawn_failure: true,
            })?;

        let backend = ready_rx
            .recv()
            .map_err(|_| SessionLoadError {
                detail: "session thread stopped during model load".to_string(),
                spawn_failure: false,
            })?
            .map_err(|detail| SessionLoadError {
                detail,
                spawn_failure: false,
            })?;

        Ok(Self {
            sender,
            join: Mutex::new(Some(join)),
            backend,
        })
    }

    /// A `Send` clone of the channel to the session thread, usable while a
    /// provider holds its own lock across an `await` (a `MutexGuard` may
    /// not cross an await point).
    pub fn handle(&self) -> SessionHandle<O> {
        SessionHandle {
            sender: self.sender.clone(),
        }
    }

    /// Ends the thread and joins it, freeing the native model on the
    /// thread that owns it.
    pub fn shutdown(self) {
        let SessionThread { sender, join, .. } = self;
        drop(sender);
        let join = join
            .into_inner()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if let Some(join) = join {
            let _ = join.join();
        }
    }
}

/// A `Send` handle to a live session thread.
pub struct SessionHandle<O: Send + 'static> {
    sender: Sender<SessionRequest<O>>,
}

impl<O: Send + 'static> SessionHandle<O> {
    /// Sends one request and awaits its result without blocking the
    /// runtime.
    pub async fn request(
        &self,
        audio: Vec<f32>,
        language: Option<String>,
    ) -> Result<O, SessionRequestError> {
        let (reply, receiver) = tokio::sync::oneshot::channel();
        self.sender
            .send(SessionRequest {
                audio,
                language,
                reply,
            })
            .map_err(|_| SessionRequestError::ThreadGone)?;

        receiver
            .await
            .map_err(|_| SessionRequestError::ThreadStopped)?
            .map_err(SessionRequestError::Engine)
    }
}

/// Why a request against a live session thread failed.
#[derive(Debug)]
pub enum SessionRequestError {
    /// The thread is gone (e.g. the model was unloaded).
    ThreadGone,
    /// The thread stopped before returning a result.
    ThreadStopped,
    /// The native engine run failed.
    Engine(TranscribeCppError),
}

impl std::fmt::Display for SessionRequestError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ThreadGone => write!(
                f,
                "transcribe.cpp session thread is no longer running; reload the model in Settings"
            ),
            Self::ThreadStopped => {
                write!(
                    f,
                    "transcribe.cpp session thread stopped before returning a result"
                )
            }
            Self::Engine(error) => write!(f, "{error}"),
        }
    }
}

impl std::error::Error for SessionRequestError {}

fn run_session_thread<T: SessionTask>(
    engine: Arc<dyn TranscribeCppEngine>,
    path: PathBuf,
    receiver: Receiver<SessionRequest<T::Output>>,
    ready: Sender<Result<String, String>>,
) {
    let mut model = match engine.load_model(&path) {
        Ok(model) => model,
        Err(error) => {
            let _ = ready.send(Err(error.to_string()));
            return;
        }
    };

    let backend = model.backend();
    if ready.send(Ok(backend)).is_err() {
        return;
    }

    while let Ok(request) = receiver.recv() {
        let result = T::run(
            model.as_mut(),
            &request.audio,
            SAMPLE_RATE,
            request.language.as_deref(),
        );
        let _ = request.reply.send(result);
    }

    drop(model);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transcribe_cpp_engine::engine::TranscriptionOutput;

    struct FakeModel {
        calls: Arc<Mutex<Vec<std::thread::ThreadId>>>,
        run_panic: bool,
    }

    impl TranscribeCppModel for FakeModel {
        fn backend(&self) -> String {
            "cpu".to_string()
        }

        fn transcribe_asr(
            &mut self,
            audio: &[f32],
            sample_rate: i32,
            language: Option<&str>,
        ) -> Result<TranscriptionOutput, TranscribeCppError> {
            if self.run_panic {
                panic!("simulated native crash");
            }
            self.calls.lock().unwrap().push(std::thread::current().id());
            Ok(TranscriptionOutput {
                backend: "cpu".into(),
                text: format!("{}@{sample_rate}/{language:?}", audio.len()),
                segments: Vec::new(),
                detected_language: None,
            })
        }
    }

    struct FakeEngine {
        calls: Arc<Mutex<Vec<std::thread::ThreadId>>>,
        load_error: Option<String>,
        run_panic: bool,
    }

    impl TranscribeCppEngine for FakeEngine {
        fn load_model(
            &self,
            _path: &Path,
        ) -> Result<Box<dyn TranscribeCppModel>, TranscribeCppError> {
            self.calls.lock().unwrap().push(std::thread::current().id());
            match &self.load_error {
                Some(detail) => Err(TranscribeCppError::Native {
                    status: crate::transcribe_cpp_engine::error::Status::Gguf,
                    detail: Some(detail.clone()),
                    context: "test",
                }),
                None => Ok(Box::new(FakeModel {
                    calls: Arc::clone(&self.calls),
                    run_panic: self.run_panic,
                })),
            }
        }
    }

    struct AsrTask;

    impl SessionTask for AsrTask {
        type Output = TranscriptionOutput;

        fn run(
            model: &mut dyn TranscribeCppModel,
            audio: &[f32],
            sample_rate: i32,
            language: Option<&str>,
        ) -> Result<Self::Output, TranscribeCppError> {
            model.transcribe_asr(audio, sample_rate, language)
        }
    }

    #[test]
    fn model_load_and_every_run_happen_on_the_session_thread() {
        let calls = Arc::new(Mutex::new(Vec::new()));
        let engine = Arc::new(FakeEngine {
            calls: Arc::clone(&calls),
            load_error: None,
            run_panic: false,
        });
        let thread = SessionThread::<TranscriptionOutput>::spawn::<AsrTask>(
            engine,
            Path::new("unused.gguf"),
            "tcp-session-test",
        )
        .expect("fake loads");
        assert_eq!(thread.backend, "cpu");

        let handle = thread.handle();
        let output = tokio::runtime::Builder::new_current_thread()
            .build()
            .expect("runtime")
            .block_on(handle.request(vec![0.0f32; 160], Some("ru".into())))
            .expect("request");
        assert_eq!(output.text, "160@16000/Some(\"ru\")");

        // The thread loop ends when the LAST sender is gone: the caller's
        // handle clones must be dropped before shutdown() may join.
        drop(handle);
        thread.shutdown();
        let calls = calls.lock().unwrap();
        assert_eq!(calls.len(), 2, "one load + one run");
        assert_eq!(calls[0], calls[1], "load and run share the session thread");
    }

    #[test]
    fn load_failure_surfaces_as_a_session_load_error() {
        let engine = Arc::new(FakeEngine {
            calls: Arc::new(Mutex::new(Vec::new())),
            load_error: Some("corrupt GGUF".into()),
            run_panic: false,
        });
        let error = match SessionThread::<TranscriptionOutput>::spawn::<AsrTask>(
            engine,
            Path::new("bad.gguf"),
            "tcp-session-test",
        ) {
            Ok(_) => panic!("load must fail"),
            Err(error) => error,
        };
        assert!(!error.spawn_failure);
        assert!(error.detail.contains("corrupt GGUF"), "detail: {}", error.detail);
    }

    #[test]
    fn a_crashed_run_ends_the_thread_and_later_requests_report_it() {
        // A native run that dies takes the thread with it: the request
        // whose run panicked resolves as ThreadStopped (its oneshot dies
        // with the unwound loop), and once the thread is fully gone the
        // channel is disconnected, so the next request reports ThreadGone.
        // (The same two outcomes a shutdown-raced request sees in
        // production: the provider drops its handles before shutdown.)
        let engine = Arc::new(FakeEngine {
            calls: Arc::new(Mutex::new(Vec::new())),
            load_error: None,
            run_panic: true,
        });
        let thread = SessionThread::<TranscriptionOutput>::spawn::<AsrTask>(
            engine,
            Path::new("unused.gguf"),
            "tcp-session-test",
        )
        .expect("fake loads");
        let handle = thread.handle();

        let runtime = tokio::runtime::Builder::new_current_thread()
            .build()
            .expect("runtime");
        let error = runtime
            .block_on(handle.request(vec![0.0f32; 160], None))
            .expect_err("the run panics on the session thread");
        assert!(matches!(error, SessionRequestError::ThreadStopped));

        // The thread is dead now (join below proves it, deterministically:
        // join on an exited thread returns immediately). Requesting again
        // must fail — either the queued request dies with the receiver
        // (ThreadStopped) or the disconnected channel refuses it
        // (ThreadGone); both mean "the session thread is no longer there".
        let error = runtime
            .block_on(handle.request(vec![0.0f32; 160], None))
            .expect_err("the thread is gone");
        assert!(
            matches!(
                error,
                SessionRequestError::ThreadGone | SessionRequestError::ThreadStopped
            ),
            "got {error:?}"
        );

        drop(handle);
        thread.shutdown();
    }
}
