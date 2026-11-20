//! The audio.cpp session thread: one native model handle serviced by
//! exactly one thread for its whole lifetime.
//!
//! audio.cpp handles are not thread-safe individually
//! (docs/research/audio-cpp-c-api.md): one session is serviced by one thread.
//! `tokio::task::spawn_blocking` is not enough, because its pool may hand a
//! later request to a different thread while a handle is still live. Both
//! app-level consumer of the engine therefore goes through this module:
//!
//! - the diarizer (`audio/transcription/audio_cpp_diarizer.rs`).
//!
//! A [`SessionThread`] is parameterized by the work one request does on the
//! loaded model ([`SessionTask`]); the thread owns the model from load to
//! drop, and requests cross a channel and an awaited oneshot, so the async
//! runtime is never blocked.

use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;

use super::engine::{AudioCppEngine, AudioCppModel, OFFLINE_MODE, SAMPLE_RATE};
use super::error::AudioCppError;

/// The work one session thread performs.
///
/// The associated output type is what a request resolves to; the capability
/// query is made once, right after load, against [`Self::TASK`].
pub trait SessionTask: Send + 'static {
    /// The result of one request.
    type Output: Send + 'static;

    /// The audio.cpp task token this consumer drives (`"diar"`).
    const TASK: &'static str;

    /// Runs one request on the thread that owns the model.
    fn run(
        model: &mut dyn AudioCppModel,
        audio: &[f32],
        sample_rate: i32,
    ) -> Result<Self::Output, AudioCppError>;
}

/// One request handed to the session thread.
struct SessionRequest<O> {
    audio: Vec<f32>,
    reply: tokio::sync::oneshot::Sender<Result<O, AudioCppError>>,
}

/// A loaded model plus the thread that owns it.
///
/// Dropping the sender ends the loop; [`SessionThread::shutdown`] additionally
/// joins the thread so the native model is freed deterministically.
pub struct SessionThread<O: Send + 'static> {
    sender: Sender<SessionRequest<O>>,
    join: Mutex<Option<JoinHandle<()>>>,
    /// Family auto-detected from the model's GGUF metadata.
    pub family: String,
    /// Whether the model can run the task in offline mode.
    pub supports_offline: bool,
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
                "Failed to start audio.cpp session thread: {}",
                self.detail
            )
        } else {
            write!(f, "Failed to load audio.cpp model: {}", self.detail)
        }
    }
}

impl std::error::Error for SessionLoadError {}

impl<O: Send + 'static> SessionThread<O> {
    /// Spawns the session thread and waits for the model load verdict, so a
    /// bad model is reported as a load failure, not a later request error.
    pub fn spawn<T: SessionTask<Output = O>>(
        engine: Arc<dyn AudioCppEngine>,
        path: &Path,
        thread_name: &str,
    ) -> Result<Self, SessionLoadError> {
        let path = path.to_path_buf();
        let (sender, receiver) = mpsc::channel::<SessionRequest<O>>();
        let (ready_tx, ready_rx) = mpsc::channel::<Result<SessionMetadata, String>>();

        let join = std::thread::Builder::new()
            .name(thread_name.to_string())
            .spawn(move || run_session_thread::<T>(engine, path, receiver, ready_tx))
            .map_err(|error| SessionLoadError {
                detail: error.to_string(),
                spawn_failure: true,
            })?;

        let metadata = ready_rx
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
            family: metadata.family,
            supports_offline: metadata.supports_offline,
        })
    }

    /// A `Send` clone of the channel to the session thread, usable while a
    /// provider holds its own lock across an `await` (a `MutexGuard` may not
    /// cross an await point).
    pub fn handle(&self) -> SessionHandle<O> {
        SessionHandle {
            sender: self.sender.clone(),
        }
    }

    /// Ends the thread and joins it, freeing the native model on the thread
    /// that owns it.
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
    /// Sends one request and awaits its result without blocking the runtime.
    pub async fn request(&self, audio: Vec<f32>) -> Result<O, SessionRequestError> {
        let (reply, receiver) = tokio::sync::oneshot::channel();
        self.sender
            .send(SessionRequest { audio, reply })
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
    Engine(AudioCppError),
}

impl std::fmt::Display for SessionRequestError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ThreadGone => write!(
                f,
                "audio.cpp session thread is no longer running; reload the model in Settings"
            ),
            Self::ThreadStopped => {
                write!(
                    f,
                    "audio.cpp session thread stopped before returning a result"
                )
            }
            Self::Engine(error) => write!(f, "{error}"),
        }
    }
}

impl std::error::Error for SessionRequestError {}

struct SessionMetadata {
    family: String,
    supports_offline: bool,
}

fn run_session_thread<T: SessionTask>(
    engine: Arc<dyn AudioCppEngine>,
    path: PathBuf,
    receiver: Receiver<SessionRequest<T::Output>>,
    ready: Sender<Result<SessionMetadata, String>>,
) {
    let mut model = match engine.load_model(&path) {
        Ok(model) => model,
        Err(error) => {
            let _ = ready.send(Err(error.to_string()));
            return;
        }
    };

    let family = model.family();
    let supports_offline = match model.supports(T::TASK, OFFLINE_MODE) {
        Ok(supported) => supported,
        Err(error) => {
            let _ = ready.send(Err(error.to_string()));
            return;
        }
    };
    if ready
        .send(Ok(SessionMetadata {
            family,
            supports_offline,
        }))
        .is_err()
    {
        return;
    }

    while let Ok(request) = receiver.recv() {
        let result = T::run(model.as_mut(), &request.audio, SAMPLE_RATE);
        let _ = request.reply.send(result);
    }

    drop(model);
}
