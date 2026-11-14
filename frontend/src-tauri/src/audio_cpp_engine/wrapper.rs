//! Safe wrappers over the raw audio.cpp bindings in `super::ffi`.
//!
//! This is the module the rest of the app talks to; all interaction with
//! the native library is isolated here and in `super::ffi`.
//!
//! Threading contract (docs/research/audio-cpp-c-api.md):
//! - Handles are not thread-safe individually: one session per thread.
//!   Wrappers are therefore `Send` (a handle may move to its dedicated
//!   thread) but never `Sync`.
//! - Every native call blocks; from async code call them via
//!   `tokio::task::spawn_blocking`.
//! - The thread count is set per backend in [`BackendConfig`].

use std::ffi::{c_char, c_int, CString};
use std::path::{Path, PathBuf};
use std::ptr::NonNull;
use std::sync::Arc;

use super::backend::AudioCppBackend;
use super::error::{AbiVersion, AudioCppError, Status};
use super::ffi::{self, AudiocppApi};

/// Environment variable overriding the audio.cpp library location
/// (development escape hatch; packaging places the library next to the
/// executable).
pub const AUDIOCPP_LIBRARY_ENV: &str = "AUDIOCPP_LIBRARY_PATH";

/// Shared-library file names probed in each search directory.
/// Linux and Windows are the supported platforms of this iteration;
/// macOS is a later stage (docs/adr/0001-audio-cpp-as-third-engine.md).
#[cfg(target_os = "linux")]
pub const LIBRARY_FILE_NAMES: &[&str] = &["libaudiocpp.so", "libaudiocpp.so.0"];
#[cfg(target_os = "windows")]
pub const LIBRARY_FILE_NAMES: &[&str] = &["audiocpp.dll"];
#[cfg(target_os = "macos")]
pub const LIBRARY_FILE_NAMES: &[&str] = &["libaudiocpp.dylib"];

/// Directories probed for the audio.cpp library, in order.
pub fn library_search_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            dirs.push(dir.to_path_buf());
        }
    }
    dirs
}

/// Root of the per-variant library directories placed next to the
/// executable by packaging: `audiocpp-libs/<variant>/libaudiocpp.so*`.
/// `<variant>` is an `AudioCppBackend::session_token()` ("cpu", "vulkan", …)
/// matching the pinned release archive flavours (`lib-<platform>-cpu`,
/// `lib-<platform>-vulkan`).
pub const LIBRARY_VARIANTS_DIR: &str = "audiocpp-libs";

/// Resolves the audio.cpp library for the given backend variant: the
/// `AUDIOCPP_LIBRARY_ENV` override wins, then the variant subdirectory of
/// each search directory, then the flat layout of each search directory
/// (single-variant installs and development trees).
pub fn resolve_library_path_for(backend: AudioCppBackend) -> Result<PathBuf, AudioCppError> {
    resolve_library_path_in(&library_search_dirs(), backend)
}

fn resolve_library_path_in(
    search_dirs: &[PathBuf],
    backend: AudioCppBackend,
) -> Result<PathBuf, AudioCppError> {
    if let Some(env_path) = std::env::var_os(AUDIOCPP_LIBRARY_ENV) {
        let path = PathBuf::from(env_path);
        if path.is_file() {
            return Ok(path);
        }
        return Err(AudioCppError::LibraryLoad {
            path,
            detail: format!("{AUDIOCPP_LIBRARY_ENV} points to a missing file"),
        });
    }

    let variant = backend.session_token();
    let mut searched = Vec::new();
    for dir in search_dirs {
        for name in LIBRARY_FILE_NAMES {
            let candidate = dir.join(LIBRARY_VARIANTS_DIR).join(variant).join(name);
            if candidate.is_file() {
                return Ok(candidate);
            }
            searched.push(candidate);
        }
        for name in LIBRARY_FILE_NAMES {
            let candidate = dir.join(name);
            if candidate.is_file() {
                return Ok(candidate);
            }
            searched.push(candidate);
        }
    }
    Err(AudioCppError::LibraryNotFound {
        searched_paths: searched,
    })
}

fn c_string(value: &str, context: &'static str) -> Result<CString, AudioCppError> {
    CString::new(value).map_err(|_| AudioCppError::Native {
        status: Status::InvalidArgument,
        detail: Some(format!("string contains an interior NUL byte: {value:?}")),
        context,
    })
}

/// A loaded, ABI-verified audio.cpp library. Cheap to clone; the
/// underlying library stays loaded until all clones and handles are gone.
#[derive(Clone)]
pub struct AudioCpp {
    api: Arc<AudiocppApi>,
    path: PathBuf,
}

impl AudioCpp {
    /// Finds and loads the audio.cpp library for the backend chosen by the
    /// app's hardware autodetect, gating on `audiocpp_abi_version()`. An
    /// incompatible ABI is rejected with [`AudioCppError::IncompatibleAbi`]
    /// before any other call is made.
    pub fn load() -> Result<AudioCpp, AudioCppError> {
        Self::load_backend(super::backend::autodetect_backend())
    }

    /// Finds and loads the given library variant (the `cpu`/`vulkan`/…
    /// flavour of the pinned release archives).
    pub fn load_backend(variant: AudioCppBackend) -> Result<AudioCpp, AudioCppError> {
        let path = resolve_library_path_for(variant)?;
        Self::load_from(&path)
    }

    /// Loads the audio.cpp library from an explicit path, gating on the
    /// ABI version.
    pub fn load_from(path: &Path) -> Result<AudioCpp, AudioCppError> {
        // SAFETY: we only call entry points after the ABI gate below
        // confirmed the library speaks the ABI we bind against.
        let api = unsafe { AudiocppApi::load(path) }?;
        let found = api.abi_version();
        if !found.is_compatible() {
            return Err(AudioCppError::IncompatibleAbi { found });
        }
        Ok(AudioCpp {
            api: Arc::new(api),
            path: path.to_path_buf(),
        })
    }

    /// The file this library was loaded from.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// ABI version reported by the library.
    pub fn abi_version(&self) -> AbiVersion {
        self.api.abi_version()
    }

    /// audio.cpp's own build version, e.g. "0.9.4".
    pub fn build_version(&self) -> String {
        self.api.build_version()
    }

    /// The task tokens this build knows ("asr", "diar", ...). Global
    /// query; no registry or model needed.
    pub fn supported_tasks(&self) -> Vec<String> {
        let count = unsafe { (self.api.task_count)() };
        let mut tasks = Vec::with_capacity(count);
        for index in 0..count {
            let ptr = unsafe { (self.api.task_name)(index) };
            if !ptr.is_null() {
                let name = unsafe { std::ffi::CStr::from_ptr(ptr) }
                    .to_string_lossy()
                    .into_owned();
                tasks.push(name);
            }
        }
        tasks
    }

    /// Creates the model registry (`config_path = NULL` for the default
    /// loader set). Blocking; call via `spawn_blocking` from async code.
    pub fn create_registry(&self) -> Result<Registry, AudioCppError> {
        let mut raw: *mut ffi::audiocpp_registry = std::ptr::null_mut();
        let status = unsafe { (self.api.registry_create)(std::ptr::null(), &mut raw) };
        self.api.check(status, "audiocpp_registry_create")?;
        let ptr = NonNull::new(raw).ok_or(AudioCppError::Native {
            status: Status::Runtime,
            detail: Some("audiocpp_registry_create returned a null registry".to_string()),
            context: "audiocpp_registry_create",
        })?;
        Ok(Registry {
            ptr,
            api: Arc::clone(&self.api),
        })
    }
}

/// Opaque wrapper over `audiocpp_registry`. Not thread-safe.
pub struct Registry {
    ptr: NonNull<ffi::audiocpp_registry>,
    api: Arc<AudiocppApi>,
}

// SAFETY: the handle may move to the dedicated thread that uses it; the
// library itself is loaded process-wide. It must not be shared (no Sync).
unsafe impl Send for Registry {}

impl Registry {
    /// Model families this registry can serve.
    pub fn families(&self) -> Vec<String> {
        let count = unsafe { (self.api.registry_family_count)(self.ptr.as_ptr()) };
        let mut families = Vec::with_capacity(count);
        for index in 0..count {
            let mut raw: *const c_char = std::ptr::null();
            let status = unsafe { (self.api.registry_family)(self.ptr.as_ptr(), index, &mut raw) };
            if status == 0 && !raw.is_null() {
                let name = unsafe { std::ffi::CStr::from_ptr(raw) }
                    .to_string_lossy()
                    .into_owned();
                families.push(name);
            }
        }
        families
    }

    /// Loads a model, auto-detecting its family from the file
    /// (`family_hint = NULL`). Blocking and potentially slow; call via
    /// `spawn_blocking` from async code.
    pub fn load_model(
        &self,
        path: &Path,
        family_hint: Option<&str>,
    ) -> Result<Model, AudioCppError> {
        let path_c = c_string(&path.to_string_lossy(), "audiocpp_model_load")?;
        let hint_c = family_hint
            .map(|hint| c_string(hint, "audiocpp_model_load"))
            .transpose()?;
        let config = ffi::audiocpp_model_config {
            family_hint: hint_c
                .as_ref()
                .map_or(std::ptr::null(), |hint| hint.as_ptr()),
            config_id: std::ptr::null(),
            weight_id: std::ptr::null(),
            model_spec_override: std::ptr::null(),
        };
        let mut raw: *mut ffi::audiocpp_model = std::ptr::null_mut();
        let status = unsafe {
            (self.api.model_load)(
                self.ptr.as_ptr(),
                path_c.as_ptr(),
                &config,
                std::ptr::null(),
                &mut raw,
            )
        };
        self.api.check(status, "audiocpp_model_load")?;
        let ptr = NonNull::new(raw).ok_or(AudioCppError::Native {
            status: Status::Runtime,
            detail: Some("audiocpp_model_load returned a null model".to_string()),
            context: "audiocpp_model_load",
        })?;
        Ok(Model {
            ptr,
            api: Arc::clone(&self.api),
        })
    }
}

impl Drop for Registry {
    fn drop(&mut self) {
        unsafe { (self.api.registry_free)(self.ptr.as_ptr()) }
    }
}

/// Opaque wrapper over `audiocpp_model`. Not thread-safe.
pub struct Model {
    ptr: NonNull<ffi::audiocpp_model>,
    api: Arc<AudiocppApi>,
}

// SAFETY: see `Registry`; one model per thread.
unsafe impl Send for Model {}

impl Model {
    /// Family of the loaded model, e.g. "gigaam_asr".
    pub fn family(&self) -> String {
        let ptr = unsafe { (self.api.model_family)(self.ptr.as_ptr()) };
        if ptr.is_null() {
            return String::new();
        }
        unsafe { std::ffi::CStr::from_ptr(ptr) }
            .to_string_lossy()
            .into_owned()
    }

    /// Human-readable model description.
    pub fn description(&self) -> String {
        let ptr = unsafe { (self.api.model_description)(self.ptr.as_ptr()) };
        if ptr.is_null() {
            return String::new();
        }
        unsafe { std::ffi::CStr::from_ptr(ptr) }
            .to_string_lossy()
            .into_owned()
    }

    /// Capability query: `task` is a token from
    /// [`AudioCpp::supported_tasks`], `mode` is "offline" or "streaming".
    pub fn supports(&self, task: &str, mode: &str) -> Result<bool, AudioCppError> {
        let task_c = c_string(task, "audiocpp_model_supports")?;
        let mode_c = c_string(mode, "audiocpp_model_supports")?;
        let supported = unsafe {
            (self.api.model_supports)(self.ptr.as_ptr(), task_c.as_ptr(), mode_c.as_ptr())
        };
        Ok(supported == 1)
    }

    /// Creates a task session. `backend = None` means CPU / device 0 /
    /// 1 thread (NULL config). Blocking; call via `spawn_blocking`.
    pub fn create_session(
        &self,
        task: &str,
        mode: &str,
        backend: Option<&BackendConfig>,
        options: Option<&Options>,
    ) -> Result<Session, AudioCppError> {
        let task_c = c_string(task, "audiocpp_session_create")?;
        let mode_c = c_string(mode, "audiocpp_session_create")?;
        let backend_name_c = backend
            .and_then(|b| b.backend.as_deref())
            .map(|name| c_string(name, "audiocpp_session_create"))
            .transpose()?;
        let raw_backend = backend.map(|b| ffi::audiocpp_backend_config {
            backend: backend_name_c
                .as_ref()
                .map_or(std::ptr::null(), |name| name.as_ptr()),
            device: b.device,
            threads: b.threads,
        });
        let options_ptr = options.map_or(std::ptr::null(), |o| o.ptr.as_ptr());

        let mut raw: *mut ffi::audiocpp_session = std::ptr::null_mut();
        let status = unsafe {
            (self.api.session_create)(
                self.ptr.as_ptr(),
                task_c.as_ptr(),
                mode_c.as_ptr(),
                raw_backend
                    .as_ref()
                    .map_or(std::ptr::null(), |config| config),
                options_ptr,
                &mut raw,
            )
        };
        self.api.check(status, "audiocpp_session_create")?;
        let ptr = NonNull::new(raw).ok_or(AudioCppError::Native {
            status: Status::Runtime,
            detail: Some("audiocpp_session_create returned a null session".to_string()),
            context: "audiocpp_session_create",
        })?;
        Ok(Session {
            ptr,
            api: Arc::clone(&self.api),
        })
    }
}

impl Drop for Model {
    fn drop(&mut self) {
        unsafe { (self.api.model_free)(self.ptr.as_ptr()) }
    }
}

/// Backend selection for a session. The audio.cpp library never touches
/// the global OpenMP thread count; `threads` here is the only knob.
#[derive(Debug, Clone, Default)]
pub struct BackendConfig {
    /// "cpu", "cuda", "vulkan", "best", ...; None = library default.
    pub backend: Option<String>,
    pub device: i32,
    /// Worker threads for the backend. 0 lets the library pick.
    pub threads: i32,
}

/// Opaque wrapper over `audiocpp_session`. Not thread-safe: one session
/// is serviced by exactly one thread for its whole lifetime.
pub struct Session {
    ptr: NonNull<ffi::audiocpp_session>,
    api: Arc<AudiocppApi>,
}

// SAFETY: the session moves to the one thread that runs it.
unsafe impl Send for Session {}

impl Session {
    /// Runs the request to completion. Blocking and potentially very
    /// slow: from async code this must run inside
    /// `tokio::task::spawn_blocking`, on the session's dedicated thread.
    pub fn run(&self, request: &Request) -> Result<AudioCppResult, AudioCppError> {
        let mut raw: *mut ffi::audiocpp_result = std::ptr::null_mut();
        let status =
            unsafe { (self.api.session_run)(self.ptr.as_ptr(), request.ptr.as_ptr(), &mut raw) };
        self.api.check(status, "audiocpp_session_run")?;
        let ptr = NonNull::new(raw).ok_or(AudioCppError::Native {
            status: Status::Runtime,
            detail: Some("audiocpp_session_run returned a null result".to_string()),
            context: "audiocpp_session_run",
        })?;
        Ok(AudioCppResult {
            ptr,
            api: Arc::clone(&self.api),
        })
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        unsafe { (self.api.session_free)(self.ptr.as_ptr()) }
    }
}

/// Opaque wrapper over `audiocpp_options` (string→string option map).
pub struct Options {
    ptr: NonNull<ffi::audiocpp_options>,
    api: Arc<AudiocppApi>,
}

unsafe impl Send for Options {}

impl Options {
    pub fn new(engine: &AudioCpp) -> Result<Options, AudioCppError> {
        let raw = unsafe { (engine.api.options_create)() };
        let ptr = NonNull::new(raw).ok_or(AudioCppError::Native {
            status: Status::OutOfMemory,
            detail: Some("audiocpp_options_create returned null".to_string()),
            context: "audiocpp_options_create",
        })?;
        Ok(Options {
            ptr,
            api: Arc::clone(&engine.api),
        })
    }

    pub fn set(&mut self, key: &str, value: &str) -> Result<(), AudioCppError> {
        let key_c = c_string(key, "audiocpp_options_set")?;
        let value_c = c_string(value, "audiocpp_options_set")?;
        let status =
            unsafe { (self.api.options_set)(self.ptr.as_ptr(), key_c.as_ptr(), value_c.as_ptr()) };
        self.api.check(status, "audiocpp_options_set")
    }
}

impl Drop for Options {
    fn drop(&mut self) {
        unsafe { (self.api.options_free)(self.ptr.as_ptr()) }
    }
}

/// Opaque wrapper over `audiocpp_request`. Not thread-safe.
pub struct Request {
    ptr: NonNull<ffi::audiocpp_request>,
    api: Arc<AudiocppApi>,
}

unsafe impl Send for Request {}

impl Request {
    pub fn new(engine: &AudioCpp) -> Result<Request, AudioCppError> {
        let raw = unsafe { (engine.api.request_create)() };
        let ptr = NonNull::new(raw).ok_or(AudioCppError::Native {
            status: Status::OutOfMemory,
            detail: Some("audiocpp_request_create returned null".to_string()),
            context: "audiocpp_request_create",
        })?;
        Ok(Request {
            ptr,
            api: Arc::clone(&engine.api),
        })
    }

    /// Interleaved float PCM; the samples are copied into the request.
    pub fn set_audio(
        &mut self,
        samples: &[f32],
        sample_rate: i32,
        channels: i32,
    ) -> Result<(), AudioCppError> {
        let status = unsafe {
            (self.api.request_set_audio)(
                self.ptr.as_ptr(),
                samples.as_ptr(),
                samples.len(),
                sample_rate,
                channels,
            )
        };
        self.api.check(status, "audiocpp_request_set_audio")
    }

    /// Flat string→string request option (`--request-option`).
    pub fn set_option(&mut self, key: &str, value: &str) -> Result<(), AudioCppError> {
        let key_c = c_string(key, "audiocpp_request_set_option")?;
        let value_c = c_string(value, "audiocpp_request_set_option")?;
        let status = unsafe {
            (self.api.request_set_option)(self.ptr.as_ptr(), key_c.as_ptr(), value_c.as_ptr())
        };
        self.api.check(status, "audiocpp_request_set_option")
    }
}

impl Drop for Request {
    fn drop(&mut self) {
        unsafe { (self.api.request_free)(self.ptr.as_ptr()) }
    }
}

/// A transcript segment with sample-based timestamps and confidence.
#[derive(Debug, Clone, PartialEq)]
pub struct Segment {
    pub start_sample: i64,
    pub end_sample: i64,
    pub confidence: f32,
    pub text: String,
}

/// A speaker turn with sample-based timestamps, speaker id and confidence.
#[derive(Debug, Clone, PartialEq)]
pub struct SpeakerTurn {
    pub start_sample: i64,
    pub end_sample: i64,
    pub speaker_id: String,
    pub confidence: f32,
    pub text: String,
}

/// Opaque wrapper over `audiocpp_result`. Not thread-safe.
pub struct AudioCppResult {
    ptr: NonNull<ffi::audiocpp_result>,
    api: Arc<AudiocppApi>,
}

unsafe impl Send for AudioCppResult {}

impl AudioCppResult {
    /// Full transcript text, or `None` when the task produced no text.
    pub fn text(&self) -> Result<Option<String>, AudioCppError> {
        let mut raw_text: *const c_char = std::ptr::null();
        let mut _language: *const c_char = std::ptr::null();
        let status =
            unsafe { (self.api.result_text)(self.ptr.as_ptr(), &mut raw_text, &mut _language) };
        if status == Status::NotAvailable as c_int {
            return Ok(None);
        }
        self.api.check(status, "audiocpp_result_text")?;
        if raw_text.is_null() {
            return Ok(None);
        }
        Ok(Some(
            unsafe { std::ffi::CStr::from_ptr(raw_text) }
                .to_string_lossy()
                .into_owned(),
        ))
    }

    /// Transcript segments (chunked output of long recordings).
    pub fn segments(&self) -> Result<Vec<Segment>, AudioCppError> {
        let count = unsafe { (self.api.result_segment_count)(self.ptr.as_ptr()) };
        let mut segments = Vec::with_capacity(count);
        for index in 0..count {
            let mut start: i64 = 0;
            let mut end: i64 = 0;
            let mut confidence: f32 = 0.0;
            let mut raw_text: *const c_char = std::ptr::null();
            let status = unsafe {
                (self.api.result_segment)(
                    self.ptr.as_ptr(),
                    index,
                    &mut start,
                    &mut end,
                    &mut confidence,
                    &mut raw_text,
                )
            };
            self.api.check(status, "audiocpp_result_segment")?;
            let text = if raw_text.is_null() {
                String::new()
            } else {
                unsafe { std::ffi::CStr::from_ptr(raw_text) }
                    .to_string_lossy()
                    .into_owned()
            };
            segments.push(Segment {
                start_sample: start,
                end_sample: end,
                confidence,
                text,
            });
        }
        Ok(segments)
    }

    /// Speaker turns from a diarization run.
    pub fn speaker_turns(&self) -> Result<Vec<SpeakerTurn>, AudioCppError> {
        let count = unsafe { (self.api.result_speaker_turn_count)(self.ptr.as_ptr()) };
        let mut turns = Vec::with_capacity(count);
        for index in 0..count {
            let mut start: i64 = 0;
            let mut end: i64 = 0;
            let mut raw_speaker: *const c_char = std::ptr::null();
            let mut confidence: f32 = 0.0;
            let mut raw_text: *const c_char = std::ptr::null();
            let status = unsafe {
                (self.api.result_speaker_turn)(
                    self.ptr.as_ptr(),
                    index,
                    &mut start,
                    &mut end,
                    &mut raw_speaker,
                    &mut confidence,
                    &mut raw_text,
                )
            };
            self.api.check(status, "audiocpp_result_speaker_turn")?;
            let speaker_id = if raw_speaker.is_null() {
                String::new()
            } else {
                unsafe { std::ffi::CStr::from_ptr(raw_speaker) }
                    .to_string_lossy()
                    .into_owned()
            };
            let text = if raw_text.is_null() {
                String::new()
            } else {
                unsafe { std::ffi::CStr::from_ptr(raw_text) }
                    .to_string_lossy()
                    .into_owned()
            };
            turns.push(SpeakerTurn {
                start_sample: start,
                end_sample: end,
                speaker_id,
                confidence,
                text,
            });
        }
        Ok(turns)
    }
}

impl Drop for AudioCppResult {
    fn drop(&mut self) {
        unsafe { (self.api.result_free)(self.ptr.as_ptr()) }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Serializes tests that mutate or read the process environment.
    static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    #[test]
    fn library_file_names_cover_current_platform() {
        // Every platform must probe at least one name; Linux/Windows are
        // the supported ones for this iteration.
        assert!(!LIBRARY_FILE_NAMES.is_empty());
        #[cfg(target_os = "linux")]
        {
            assert!(LIBRARY_FILE_NAMES.contains(&"libaudiocpp.so"));
            assert!(LIBRARY_FILE_NAMES.contains(&"libaudiocpp.so.0"));
        }
        #[cfg(target_os = "windows")]
        {
            assert!(LIBRARY_FILE_NAMES.contains(&"audiocpp.dll"));
        }
    }

    #[test]
    fn search_dirs_are_absolute_and_include_executable_dir() {
        let dirs = library_search_dirs();
        assert!(!dirs.is_empty());
        assert!(dirs.iter().all(|d| d.is_absolute()));
        let exe_dir = std::env::current_exe()
            .expect("test binary path")
            .parent()
            .expect("exe parent dir")
            .to_path_buf();
        assert!(dirs.contains(&exe_dir));
    }

    #[test]
    fn handles_are_send() {
        // Handles may move to the dedicated thread that uses them...
        fn assert_send<T: Send>() {}
        assert_send::<AudioCpp>();
        assert_send::<Registry>();
        assert_send::<Model>();
        assert_send::<Session>();
        assert_send::<Request>();
        assert_send::<Options>();
        assert_send::<AudioCppResult>();
        // ...but they are deliberately NOT Sync: no `unsafe impl Sync`
        // exists anywhere in this module.
    }

    #[test]
    fn env_override_wins_and_missing_override_is_an_error() {
        let _guard = ENV_LOCK.lock().expect("env lock");
        // One test fn on purpose: env mutation must not race with itself.
        let dir = tempfile::tempdir().expect("tempdir");
        let lib = dir.path().join("libaudiocpp.so");
        std::fs::write(&lib, b"not a real library").expect("write fixture");

        std::env::set_var(AUDIOCPP_LIBRARY_ENV, &lib);
        let resolved =
            resolve_library_path_for(AudioCppBackend::Cpu).expect("existing override resolves");
        assert_eq!(resolved, lib);

        let missing = dir.path().join("missing.so");
        std::env::set_var(AUDIOCPP_LIBRARY_ENV, &missing);
        let err =
            resolve_library_path_for(AudioCppBackend::Cpu).expect_err("missing override errors");
        assert!(
            matches!(err, AudioCppError::LibraryLoad { .. }),
            "got {err:?}"
        );

        std::env::remove_var(AUDIOCPP_LIBRARY_ENV);
    }

    #[test]
    fn variant_subdirectory_wins_over_the_flat_layout() {
        let dir = tempfile::tempdir().expect("tempdir");
        let variant_dir = dir.path().join("audiocpp-libs").join("vulkan");
        std::fs::create_dir_all(&variant_dir).expect("variant dir");
        let flat = dir.path().join("libaudiocpp.so");
        let variant = variant_dir.join("libaudiocpp.so");
        std::fs::write(&flat, b"flat").expect("write flat fixture");
        std::fs::write(&variant, b"variant").expect("write variant fixture");

        let resolved =
            resolve_library_path_in(&[dir.path().to_path_buf()], AudioCppBackend::Vulkan)
                .expect("variant library resolves");

        assert_eq!(resolved, variant);
    }

    #[test]
    fn a_single_variant_install_uses_the_flat_layout() {
        let dir = tempfile::tempdir().expect("tempdir");
        let flat = dir.path().join("libaudiocpp.so.0");
        std::fs::write(&flat, b"flat").expect("write fixture");

        let resolved = resolve_library_path_in(&[dir.path().to_path_buf()], AudioCppBackend::Cpu)
            .expect("flat library resolves");

        assert_eq!(resolved, flat);
    }

    #[test]
    fn a_missing_variant_reports_every_searched_path() {
        let dir = tempfile::tempdir().expect("tempdir");

        let error = resolve_library_path_in(&[dir.path().to_path_buf()], AudioCppBackend::Vulkan)
            .expect_err("nothing to load");

        let searched = match error {
            AudioCppError::LibraryNotFound { searched_paths } => searched_paths,
            other => panic!("expected LibraryNotFound, got {other:?}"),
        };
        assert_eq!(searched.len(), LIBRARY_FILE_NAMES.len() * 2);
        assert!(searched
            .iter()
            .any(|path| path.to_string_lossy().contains(LIBRARY_VARIANTS_DIR)));
        assert!(searched
            .iter()
            .any(|path| !path.to_string_lossy().contains(LIBRARY_VARIANTS_DIR)));
    }

    #[test]
    fn library_next_to_executable_is_discovered() {
        let _guard = ENV_LOCK.lock().expect("env lock");
        // Bundler layout (MSI/NSIS): the runtime is staged into the install
        // root next to the executable, so discovery must find it there
        // without any environment override.
        std::env::remove_var(AUDIOCPP_LIBRARY_ENV);
        let exe_dir = std::env::current_exe()
            .expect("test binary path")
            .parent()
            .expect("exe parent dir")
            .to_path_buf();
        let staged = exe_dir.join(LIBRARY_FILE_NAMES[0]);
        assert!(
            !staged.exists(),
            "fixture path already occupied: {staged:?}"
        );
        std::fs::write(&staged, b"not a real library").expect("stage fixture");

        let resolved = resolve_library_path_for(AudioCppBackend::Cpu);
        std::fs::remove_file(&staged).expect("clean up fixture");
        assert_eq!(resolved.expect("exe-adjacent library resolves"), staged);
    }
}
