//! Safe wrappers over the raw transcribe.cpp bindings in `super::ffi`.
//!
//! This is the module the rest of the app talks to; all interaction with
//! the native library is isolated here and in `super::ffi`.
//!
//! ABI gate: before ANY call into the library, the `contract.json` sitting
//! next to the library file is read and compared against the version and
//! header hash pinned at compile time (the same rule the official
//! TypeScript binding applies in its loader). A missing, unreadable or
//! mismatching contract is rejected with
//! [`TranscribeCppError::IncompatibleAbi`] — even the `dlopen` itself is
//! deferred until after the gate, so no code from an ABI-incompatible
//! library is ever executed.
//!
//! Threading contract (transcribe.h v0.4.2 preamble):
//! - Model functions are thread-safe; a model may be shared across
//!   threads for queries and session creation.
//! - Session functions are NOT thread-safe: one session is used by at
//!   most one thread at a time. Wrappers are therefore `Send` (a handle
//!   may move to its dedicated thread) but never `Sync`.
//! - The model must outlive every session derived from it.
//! - Every native call blocks; from async code call them via
//!   `tokio::task::spawn_blocking`.

use std::ffi::{c_int, CString};
use std::path::{Path, PathBuf};
use std::ptr::NonNull;
use std::sync::Arc;

use serde::Deserialize;

use super::backend::DeviceQuery;
use super::error::{
    TranscribeCppError, TRANSCRIBECPP_PINNED_HEADER_HASH, TRANSCRIBECPP_PINNED_VERSION,
};
use super::ffi::{self, borrowed_str, TranscribecppApi};

/// Environment variable overriding the transcribe.cpp library location
/// (development escape hatch; packaging places the library next to the
/// executable).
pub const TRANSCRIBECPP_LIBRARY_ENV: &str = "TRANSCRIBECPP_LIBRARY_PATH";

/// Shared-library file names probed in each search directory, matching the
/// names the release bundles ship (`loader.ts` at v0.4.2).
#[cfg(target_os = "linux")]
pub const LIBRARY_FILE_NAMES: &[&str] = &["libtranscribe.so"];
#[cfg(target_os = "windows")]
pub const LIBRARY_FILE_NAMES: &[&str] = &["transcribe.dll"];
#[cfg(target_os = "macos")]
pub const LIBRARY_FILE_NAMES: &[&str] = &["libtranscribe.dylib"];

/// Root of the per-lane library directories placed next to the executable
/// by packaging: `transcribe-libs/<lane>/<libname>`.
pub const LIBRARY_LANES_DIR: &str = "transcribe-libs";

/// Name of the ABI contract file that must sit next to the library.
pub const CONTRACT_FILE_NAME: &str = "contract.json";

/// The lane directory name for this build target, matching the release
/// archive lanes (`transcribe-native-<lane>`): one cpu-vulkan lane per
/// supported OS/arch plus the macOS metal lane.
pub fn lane_dir_name() -> &'static str {
    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
    {
        "linux-x86_64-cpu-vulkan"
    }
    #[cfg(all(target_os = "linux", target_arch = "aarch64"))]
    {
        "linux-aarch64-cpu-vulkan"
    }
    #[cfg(all(target_os = "windows", target_arch = "x86_64"))]
    {
        "windows-x86_64-cpu-vulkan"
    }
    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    {
        "macos-arm64-metal"
    }
    #[cfg(not(any(
        all(target_os = "linux", any(target_arch = "x86_64", target_arch = "aarch64")),
        all(target_os = "windows", target_arch = "x86_64"),
        all(target_os = "macos", target_arch = "aarch64"),
    )))]
    {
        // Unsupported target: no release lane exists, so the lane probe can
        // simply never match. Discovery still falls back to the flat layout.
        "unsupported"
    }
}

/// Directories probed for the transcribe.cpp library, in order.
pub fn library_search_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            dirs.push(dir.to_path_buf());
        }
    }
    dirs
}

/// Resolves the transcribe.cpp library: the `TRANSCRIBECPP_LIBRARY_ENV`
/// override wins, then the lane subdirectory of each search directory
/// (`transcribe-libs/<lane>/`), then the flat layout of each search
/// directory (development trees and the Windows resource-map install
/// root).
pub fn resolve_library_path() -> Result<PathBuf, TranscribeCppError> {
    resolve_library_path_in(&library_search_dirs())
}

fn resolve_library_path_in(search_dirs: &[PathBuf]) -> Result<PathBuf, TranscribeCppError> {
    if let Some(env_path) = std::env::var_os(TRANSCRIBECPP_LIBRARY_ENV) {
        let path = PathBuf::from(env_path);
        if path.is_file() {
            return Ok(path);
        }
        return Err(TranscribeCppError::LibraryLoad {
            path,
            detail: format!("{TRANSCRIBECPP_LIBRARY_ENV} points to a missing file"),
        });
    }

    let lane = lane_dir_name();
    let mut searched = Vec::new();
    for dir in search_dirs {
        for name in LIBRARY_FILE_NAMES {
            let candidate = dir.join(LIBRARY_LANES_DIR).join(lane).join(name);
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
    Err(TranscribeCppError::LibraryNotFound {
        searched_paths: searched,
    })
}

/// The parsed `contract.json` the build system stamps next to the library
/// (`cmake/python-wheel-install.cmake` at v0.4.2): exactly the keys
/// `version`, `header_hash`, `backends`, `lane`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct AbiContract {
    pub version: String,
    pub header_hash: String,
    /// Backend kinds the lane was built with, accelerated first
    /// (e.g. `["vulkan", "cpu"]`).
    pub backends: Vec<String>,
    /// Build lane (e.g. "cpu-vulkan"); informational.
    pub lane: Option<String>,
}

impl AbiContract {
    /// Reads and validates the contract next to `library_path`, BEFORE any
    /// call into the library. Missing, unreadable, malformed, or
    /// mismatching contracts all yield
    /// [`TranscribeCppError::IncompatibleAbi`].
    pub fn load_for(library_path: &Path) -> Result<AbiContract, TranscribeCppError> {
        let dir = library_path.parent().unwrap_or_else(|| Path::new("."));
        let contract_path = dir.join(CONTRACT_FILE_NAME);
        Self::load_from_file(&contract_path)
    }

    fn load_from_file(path: &Path) -> Result<AbiContract, TranscribeCppError> {
        let contents = std::fs::read_to_string(path).map_err(|error| {
            TranscribeCppError::IncompatibleAbi {
                found: format!("{}: {error}", path.display()),
                detail: "the ABI contract file next to the library is missing or unreadable"
                    .to_string(),
            }
        })?;
        let contract: AbiContract =
            serde_json::from_str(&contents).map_err(|error| TranscribeCppError::IncompatibleAbi {
                found: format!("{}: {error}", path.display()),
                detail: "the ABI contract file next to the library is not valid JSON with the \
                         keys version, header_hash, backends, lane"
                    .to_string(),
            })?;
        if contract.version != TRANSCRIBECPP_PINNED_VERSION {
            return Err(TranscribeCppError::IncompatibleAbi {
                found: format!(
                    "version {}, header_hash {}",
                    contract.version, contract.header_hash
                ),
                detail: "library version mismatch".to_string(),
            });
        }
        if contract.header_hash != TRANSCRIBECPP_PINNED_HEADER_HASH {
            return Err(TranscribeCppError::IncompatibleAbi {
                found: format!(
                    "version {}, header_hash {}",
                    contract.version, contract.header_hash
                ),
                detail: "library header hash mismatch".to_string(),
            });
        }
        Ok(contract)
    }
}

fn c_string(value: &str, context: &'static str) -> Result<CString, TranscribeCppError> {
    CString::new(value).map_err(|_| TranscribeCppError::Native {
        status: super::error::Status::InvalidArg,
        detail: Some(format!("string contains an interior NUL byte: {value:?}")),
        context,
    })
}

/// A loaded, contract-verified transcribe.cpp library. Cheap to clone; the
/// underlying library stays loaded until all clones and handles are gone.
#[derive(Clone)]
pub struct TranscribeCpp {
    api: Arc<TranscribecppApi>,
    path: PathBuf,
    contract: AbiContract,
}

impl TranscribeCpp {
    /// Finds and loads the transcribe.cpp library for this build target,
    /// gating on the ABI contract before any native code runs.
    pub fn load() -> Result<TranscribeCpp, TranscribeCppError> {
        let path = resolve_library_path()?;
        Self::load_from(&path)
    }

    /// Loads the transcribe.cpp library from an explicit path, gating on
    /// the `contract.json` next to it. The gate runs before `dlopen`, so a
    /// mismatching library is never even mapped into the process.
    pub fn load_from(path: &Path) -> Result<TranscribeCpp, TranscribeCppError> {
        // The ABI gate first: no native code (not even the loader's
        // constructors) may run from a library that disagrees with the
        // pinned header.
        let contract = AbiContract::load_for(path)?;
        // SAFETY: the contract above confirmed the library reports the
        // exact pinned version and header hash this binding is written
        // against; the symbol set below is verified during load.
        let api = unsafe { TranscribecppApi::load(path) }?;
        // Belt and braces: the runtime version must agree with the contract.
        let runtime_version = api.version();
        if runtime_version != contract.version {
            return Err(TranscribeCppError::IncompatibleAbi {
                found: format!(
                    "contract version {}, library reports {runtime_version}",
                    contract.version
                ),
                detail: "contract.json disagrees with transcribe_version()".to_string(),
            });
        }
        Ok(TranscribeCpp {
            api: Arc::new(api),
            path: path.to_path_buf(),
            contract,
        })
    }

    /// The file this library was loaded from.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// The validated contract the library was gated on.
    pub fn contract(&self) -> &AbiContract {
        &self.contract
    }

    /// transcribe.cpp's own version string, e.g. "0.4.2".
    pub fn version(&self) -> String {
        self.api.version()
    }

    /// Loads the ggml backend modules shipped next to the library, fixing
    /// the allowed-backend mask. Idempotent per directory; must run before
    /// the first model load or device query (transcribe.h: "call once,
    /// first"). Blocking; call via `spawn_blocking` from async code.
    pub fn init_backends(&self, allowed_backends: u32) -> Result<(), TranscribeCppError> {
        // Package-local scan: the directory containing libtranscribe itself,
        // where the release bundle stages the libggml* backend modules.
        let dir_c = c_string(
            &self
                .path
                .parent()
                .unwrap_or_else(|| Path::new("."))
                .to_string_lossy(),
            "transcribe_init_backends_ex",
        )?;
        let mut params = ffi::transcribe_backend_init_params {
            struct_size: 0,
            artifact_dir: std::ptr::null(),
            allowed_backends,
        };
        unsafe { (self.api.backend_init_params_init)(&mut params) };
        params.artifact_dir = dir_c.as_ptr();
        params.allowed_backends = allowed_backends;
        let status = unsafe { (self.api.init_backends_ex)(&params) };
        self.api.check(status, "transcribe_init_backends_ex")
    }

    /// Whether the library can satisfy a backend request
    /// (`transcribe_backend_available`). Call after [`Self::init_backends`].
    pub fn backend_available(&self, kind: c_int) -> bool {
        unsafe { (self.api.backend_available)(kind) }
    }

    /// Number of registered compute devices. Call after
    /// [`Self::init_backends`].
    pub fn device_count(&self) -> i32 {
        unsafe { (self.api.device_count)() }
    }

    /// The device handle at the registry `index` (NULL out of range), for
    /// pinning into model-load params.
    ///
    /// # Safety
    /// Requires [`Self::init_backends`] to have completed; the handle is
    /// runtime-owned, valid for the life of the process, and must not be
    /// freed.
    pub unsafe fn device_handle(&self, index: i32) -> *mut ffi::transcribe_device {
        (self.api.device_get)(index)
    }

    /// Kind of the device at `index` ("cpu", "vulkan", "metal", …); `None`
    /// out of range or when the runtime reports no kind.
    pub fn device_kind(&self, index: i32) -> Option<String> {
        let device = unsafe { (self.api.device_get)(index) };
        if device.is_null() {
            return None;
        }
        let mut info = ffi::transcribe_device_info {
            struct_size: 0,
            name: std::ptr::null(),
            description: std::ptr::null(),
            kind: std::ptr::null(),
            device_id: std::ptr::null(),
            memory_total: 0,
            memory_free: 0,
            device_type: 0,
        };
        unsafe { (self.api.device_info_init)(&mut info) };
        let status = unsafe { (self.api.device_get_info)(device, &mut info) };
        if status != 0 {
            return None;
        }
        unsafe { borrowed_str(info.kind) }.filter(|kind| !kind.is_empty())
    }

    /// Loads a GGUF model file onto the requested backend/device pair.
    /// `device` may be NULL for automatic selection. Blocking and
    /// potentially slow; call via `spawn_blocking` from async code.
    pub fn load_model(
        &self,
        path: &Path,
        backend: c_int,
        device: *mut ffi::transcribe_device,
    ) -> Result<Model, TranscribeCppError> {
        let path_c = c_string(&path.to_string_lossy(), "transcribe_model_load_file")?;
        let mut params = ffi::transcribe_model_load_params {
            struct_size: 0,
            backend,
            device,
            no_mmap: false,
        };
        unsafe { (self.api.model_load_params_init)(&mut params) };
        params.backend = backend;
        params.device = device;
        let mut raw: *mut ffi::transcribe_model = std::ptr::null_mut();
        let status =
            unsafe { (self.api.model_load_file)(path_c.as_ptr(), &params, &mut raw) };
        self.api.check(status, "transcribe_model_load_file")?;
        let ptr = NonNull::new(raw).ok_or(TranscribeCppError::Native {
            status: super::error::Status::Backend,
            detail: Some("transcribe_model_load_file returned a null model".to_string()),
            context: "transcribe_model_load_file",
        })?;
        Ok(Model {
            ptr,
            api: Arc::clone(&self.api),
        })
    }
}

/// The transcribe.cpp library answers device-registry queries for
/// [`super::backend::select_device`].
impl DeviceQuery for TranscribeCpp {
    fn backend_available(&self, kind: c_int) -> bool {
        TranscribeCpp::backend_available(self, kind)
    }

    fn device_count(&self) -> i32 {
        TranscribeCpp::device_count(self)
    }

    fn device_kind(&self, index: i32) -> Option<String> {
        TranscribeCpp::device_kind(self, index)
    }
}

/// Opaque wrapper over `struct transcribe_model`. Model queries are
/// thread-safe, but keep ownership with the thread that runs sessions
/// derived from it (the model must outlive its sessions).
pub struct Model {
    ptr: NonNull<ffi::transcribe_model>,
    api: Arc<TranscribecppApi>,
}

// SAFETY: the handle may move to the dedicated thread that uses it. It is
// deliberately NOT Sync: see the module docs and the wrapper tests.
unsafe impl Send for Model {}

impl Model {
    /// The runtime backend actually bound to this model, e.g. "cpu",
    /// "vulkan", "metal" (`transcribe_model_backend`). The mechanism for
    /// detecting a GPU→CPU fallback after load.
    pub fn backend(&self) -> String {
        let ptr = unsafe { (self.api.model_backend)(self.ptr.as_ptr()) };
        unsafe { borrowed_str(ptr) }.unwrap_or_default()
    }

    /// Creates an offline ASR session against this model. The session must
    /// be dropped before this model. Blocking; call via `spawn_blocking`.
    pub fn create_session(&self) -> Result<Session, TranscribeCppError> {
        let mut params = ffi::transcribe_session_params {
            struct_size: 0,
            n_threads: 0,
            kv_type: 0,
            n_ctx: 0,
        };
        unsafe { (self.api.session_params_init)(&mut params) };
        let mut raw: *mut ffi::transcribe_session = std::ptr::null_mut();
        let status =
            unsafe { (self.api.session_init)(self.ptr.as_ptr(), &params, &mut raw) };
        self.api.check(status, "transcribe_session_init")?;
        let ptr = NonNull::new(raw).ok_or(TranscribeCppError::Native {
            status: super::error::Status::Backend,
            detail: Some("transcribe_session_init returned a null session".to_string()),
            context: "transcribe_session_init",
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

/// A transcript segment with millisecond timestamps relative to the input
/// audio (transcribe.h: all timestamps are int64 ms).
#[derive(Debug, Clone, PartialEq)]
pub struct Segment {
    pub start_ms: i64,
    pub end_ms: i64,
    pub text: String,
    /// 1-based speaker id; 0 when the run carried no speaker attribution.
    pub speaker_id: i32,
}

/// The outcome of one offline run, copied out of session-owned storage
/// before the next mutating call can invalidate it.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct RunOutput {
    /// Full transcript text (empty when the audio carried no speech).
    pub text: String,
    /// Segments as reported by the model (empty when the family emits no
    /// alignment).
    pub segments: Vec<Segment>,
    /// Language the model detected on this run, when any.
    pub detected_language: Option<String>,
}

/// Opaque wrapper over `struct transcribe_session`. Not thread-safe: one
/// session is serviced by exactly one thread at a time
/// (transcribe.h threading contract).
pub struct Session {
    ptr: NonNull<ffi::transcribe_session>,
    api: Arc<TranscribecppApi>,
}

// SAFETY: the session moves to the one thread that runs it; never Sync.
unsafe impl Send for Session {}

impl Session {
    /// Runs one offline transcription of 16 kHz mono f32 PCM to completion
    /// and copies the result out. `language` is a BCP-47-ish hint or
    /// `None` for autodetection. Blocking and potentially very slow: from
    /// async code this must run on the session's dedicated thread
    /// (`super::session_thread`).
    pub fn run(&self, audio: &[f32], language: Option<&str>) -> Result<RunOutput, TranscribeCppError> {
        let language_c = language
            .map(|hint| c_string(hint, "transcribe_run"))
            .transpose()?;
        let mut params = ffi::transcribe_run_params {
            struct_size: 0,
            task: 0, // TRANSCRIBE_TASK_TRANSCRIBE
            timestamps: ffi::TRANSCRIBE_TIMESTAMPS_AUTO,
            pnc: 0,
            itn: 0,
            diarize: 0,
            language: std::ptr::null(),
            target_language: std::ptr::null(),
            keep_special_tags: false,
            family: std::ptr::null(),
            spec_k_drafts: 0,
            vocabulary: std::ptr::null(),
            n_vocabulary: 0,
            prompt: std::ptr::null(),
            prefix: std::ptr::null(),
        };
        unsafe { (self.api.run_params_init)(&mut params) };
        // Re-apply after init(): _init stamps defaults (including AUTO
        // timestamps), the language hint is ours.
        params.language = language_c
            .as_ref()
            .map_or(std::ptr::null(), |hint| hint.as_ptr());

        let n_samples = audio.len().min(c_int::MAX as usize) as c_int;
        let status = unsafe { (self.api.run)(self.ptr.as_ptr(), audio.as_ptr(), n_samples, &params) };
        self.api.check(status, "transcribe_run")?;

        let text = unsafe { borrowed_str((self.api.full_text)(self.ptr.as_ptr())) }.unwrap_or_default();
        let detected_language =
            unsafe { borrowed_str((self.api.detected_language)(self.ptr.as_ptr())) }
                .filter(|lang| !lang.is_empty());

        let count = unsafe { (self.api.n_segments)(self.ptr.as_ptr()) }.max(0);
        let mut segments = Vec::with_capacity(count as usize);
        for index in 0..count {
            let mut row = ffi::transcribe_segment {
                struct_size: 0,
                t0_ms: 0,
                t1_ms: 0,
                first_word: 0,
                n_words: 0,
                first_token: 0,
                n_tokens: 0,
                text: std::ptr::null(),
                speaker_id: 0,
            };
            unsafe { (self.api.segment_init)(&mut row) };
            let status = unsafe { (self.api.get_segment)(self.ptr.as_ptr(), index, &mut row) };
            self.api.check(status, "transcribe_get_segment")?;
            segments.push(Segment {
                start_ms: row.t0_ms,
                end_ms: row.t1_ms,
                text: unsafe { borrowed_str(row.text) }.unwrap_or_default(),
                speaker_id: row.speaker_id,
            });
        }

        Ok(RunOutput {
            text,
            segments,
            detected_language,
        })
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        unsafe { (self.api.session_free)(self.ptr.as_ptr()) }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transcribe_cpp_engine::error::TranscribeCppError;

    /// Serializes tests that mutate or read the process environment.
    static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    fn write_contract(dir: &Path, json: &str) {
        std::fs::write(dir.join(CONTRACT_FILE_NAME), json).expect("write contract");
    }

    const GOOD_CONTRACT: &str = r#"{"version":"0.4.2","header_hash":"f99ad4169374b8f3","backends":["vulkan","cpu"],"lane":"cpu-vulkan"}"#;

    #[test]
    fn contract_accepts_the_pinned_values() {
        let dir = tempfile::tempdir().expect("tempdir");
        write_contract(dir.path(), GOOD_CONTRACT);
        let contract =
            AbiContract::load_from_file(&dir.path().join(CONTRACT_FILE_NAME)).expect("good contract");
        assert_eq!(contract.version, "0.4.2");
        assert_eq!(contract.header_hash, "f99ad4169374b8f3");
        assert_eq!(contract.backends, vec!["vulkan", "cpu"]);
        assert_eq!(contract.lane.as_deref(), Some("cpu-vulkan"));
    }

    #[test]
    fn contract_rejects_missing_file_before_any_call() {
        let dir = tempfile::tempdir().expect("tempdir");
        let library = dir.path().join("libtranscribe.so");
        let err = AbiContract::load_for(&library).expect_err("missing contract");
        assert!(
            matches!(err, TranscribeCppError::IncompatibleAbi { .. }),
            "got {err:?}"
        );
        assert!(err.to_string().contains("missing or unreadable"));
    }

    #[test]
    fn contract_rejects_wrong_version() {
        let dir = tempfile::tempdir().expect("tempdir");
        write_contract(
            dir.path(),
            r#"{"version":"0.4.1","header_hash":"f99ad4169374b8f3","backends":["cpu"],"lane":"cpu"}"#,
        );
        let err = AbiContract::load_for(&dir.path().join("libtranscribe.so"))
            .expect_err("wrong version");
        match err {
            TranscribeCppError::IncompatibleAbi { found, detail } => {
                assert!(found.contains("0.4.1"), "found: {found}");
                assert_eq!(detail, "library version mismatch");
            }
            other => panic!("expected IncompatibleAbi, got {other:?}"),
        }
    }

    #[test]
    fn contract_rejects_wrong_header_hash() {
        let dir = tempfile::tempdir().expect("tempdir");
        write_contract(
            dir.path(),
            r#"{"version":"0.4.2","header_hash":"0123456789abcdef","backends":["cpu"],"lane":"cpu"}"#,
        );
        let err = AbiContract::load_for(&dir.path().join("libtranscribe.so"))
            .expect_err("wrong hash");
        match err {
            TranscribeCppError::IncompatibleAbi { found, detail } => {
                assert!(found.contains("0123456789abcdef"), "found: {found}");
                assert_eq!(detail, "library header hash mismatch");
            }
            other => panic!("expected IncompatibleAbi, got {other:?}"),
        }
    }

    #[test]
    fn contract_rejects_malformed_json() {
        let dir = tempfile::tempdir().expect("tempdir");
        write_contract(dir.path(), "{ not json");
        let err = AbiContract::load_for(&dir.path().join("libtranscribe.so"))
            .expect_err("malformed json");
        assert!(
            matches!(err, TranscribeCppError::IncompatibleAbi { .. }),
            "got {err:?}"
        );
        assert!(err.to_string().contains("not valid JSON"));
    }

    #[test]
    fn contract_rejects_missing_keys() {
        let dir = tempfile::tempdir().expect("tempdir");
        write_contract(dir.path(), r#"{"version":"0.4.2"}"#);
        let err = AbiContract::load_for(&dir.path().join("libtranscribe.so"))
            .expect_err("missing keys");
        assert!(
            matches!(err, TranscribeCppError::IncompatibleAbi { .. }),
            "got {err:?}"
        );
    }

    #[test]
    fn abi_gate_runs_before_dlopen_on_a_fake_library() {
        // A file that is definitely not a shared library: the gate must
        // reject it as ABI-incompatible, never as a load failure, because
        // no loader code runs before the contract passes.
        let dir = tempfile::tempdir().expect("tempdir");
        let fake = dir.path().join("libtranscribe.so");
        std::fs::write(&fake, b"not a real library").expect("write fake lib");
        write_contract(
            dir.path(),
            r#"{"version":"9.9.9","header_hash":"deadbeefdeadbeef","backends":["cpu"],"lane":"cpu"}"#,
        );

        let err = match TranscribeCpp::load_from(&fake) {
            Ok(_) => panic!("mismatched contract must be rejected"),
            Err(err) => err,
        };
        assert!(
            matches!(err, TranscribeCppError::IncompatibleAbi { .. }),
            "got {err:?}"
        );
    }

    #[test]
    fn symbol_missing_names_the_symbol() {
        // Any real shared library that is not transcribe.cpp resolves but
        // lacks the entry points: the error must name the first one.
        let candidate = if cfg!(target_os = "linux") {
            "libc.so.6"
        } else {
            // Skip on platforms without the libc soname convention.
            return;
        };
        let err = match unsafe { TranscribecppApi::load(Path::new(candidate)) } {
            Ok(_) => panic!("libc is not transcribe.cpp"),
            Err(err) => err,
        };
        match err {
            TranscribeCppError::SymbolMissing { symbol, .. } => {
                assert_eq!(symbol, "transcribe_version", "first symbol must be named");
            }
            other => panic!("expected SymbolMissing, got {other:?}"),
        }
    }

    #[test]
    fn handles_are_send() {
        // Handles may move to the dedicated thread that uses them...
        fn assert_send<T: Send>() {}
        assert_send::<TranscribeCpp>();
        assert_send::<Model>();
        assert_send::<Session>();
        // ...but they are deliberately NOT Sync: no `unsafe impl Sync`
        // exists anywhere in this module (the native threading contract
        // forbids sharing a session across threads; the session thread in
        // super::session_thread owns it).
    }

    #[test]
    fn library_file_names_cover_current_platform() {
        assert!(!LIBRARY_FILE_NAMES.is_empty());
        #[cfg(target_os = "linux")]
        assert!(LIBRARY_FILE_NAMES.contains(&"libtranscribe.so"));
        #[cfg(target_os = "windows")]
        assert!(LIBRARY_FILE_NAMES.contains(&"transcribe.dll"));
        #[cfg(target_os = "macos")]
        assert!(LIBRARY_FILE_NAMES.contains(&"libtranscribe.dylib"));
    }

    #[test]
    fn lane_dir_name_matches_the_release_lanes() {
        // Independent source of truth: the release asset names
        // `transcribe-native-<lane>.tar.gz` (native-facts.md §1).
        let lane = lane_dir_name();
        #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
        assert_eq!(lane, "linux-x86_64-cpu-vulkan");
        #[cfg(all(target_os = "linux", target_arch = "aarch64"))]
        assert_eq!(lane, "linux-aarch64-cpu-vulkan");
        #[cfg(all(target_os = "windows", target_arch = "x86_64"))]
        assert_eq!(lane, "windows-x86_64-cpu-vulkan");
        #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
        assert_eq!(lane, "macos-arm64-metal");
    }

    #[test]
    fn env_override_wins_and_missing_override_is_an_error() {
        let _guard = ENV_LOCK.lock().expect("env lock");
        // One test fn on purpose: env mutation must not race with itself.
        let dir = tempfile::tempdir().expect("tempdir");
        let lib = dir.path().join("libtranscribe.so");
        std::fs::write(&lib, b"not a real library").expect("write fixture");

        std::env::set_var(TRANSCRIBECPP_LIBRARY_ENV, &lib);
        let resolved = resolve_library_path().expect("existing override resolves");
        assert_eq!(resolved, lib);

        let missing = dir.path().join("missing.so");
        std::env::set_var(TRANSCRIBECPP_LIBRARY_ENV, &missing);
        let err = resolve_library_path().expect_err("missing override errors");
        assert!(
            matches!(err, TranscribeCppError::LibraryLoad { .. }),
            "got {err:?}"
        );

        std::env::remove_var(TRANSCRIBECPP_LIBRARY_ENV);
    }

    #[test]
    fn lane_subdirectory_wins_over_the_flat_layout() {
        let _guard = ENV_LOCK.lock().expect("env lock");
        std::env::remove_var(TRANSCRIBECPP_LIBRARY_ENV);
        let dir = tempfile::tempdir().expect("tempdir");
        let lane_dir = dir.path().join(LIBRARY_LANES_DIR).join(lane_dir_name());
        std::fs::create_dir_all(&lane_dir).expect("lane dir");
        let flat = dir.path().join("libtranscribe.so");
        let lane = lane_dir.join("libtranscribe.so");
        std::fs::write(&flat, b"flat").expect("write flat fixture");
        std::fs::write(&lane, b"lane").expect("write lane fixture");

        let resolved =
            resolve_library_path_in(&[dir.path().to_path_buf()]).expect("lane library resolves");

        assert_eq!(resolved, lane);
    }

    #[test]
    fn a_flat_install_next_to_the_executable_is_discovered() {
        let _guard = ENV_LOCK.lock().expect("env lock");
        std::env::remove_var(TRANSCRIBECPP_LIBRARY_ENV);
        let dir = tempfile::tempdir().expect("tempdir");
        let flat = dir.path().join(LIBRARY_FILE_NAMES[0]);
        std::fs::write(&flat, b"flat").expect("write fixture");

        let resolved =
            resolve_library_path_in(&[dir.path().to_path_buf()]).expect("flat library resolves");

        assert_eq!(resolved, flat);
    }

    #[test]
    fn a_missing_library_reports_every_searched_path() {
        let _guard = ENV_LOCK.lock().expect("env lock");
        std::env::remove_var(TRANSCRIBECPP_LIBRARY_ENV);
        let dir = tempfile::tempdir().expect("tempdir");

        let error =
            resolve_library_path_in(&[dir.path().to_path_buf()]).expect_err("nothing to load");

        let searched = match error {
            TranscribeCppError::LibraryNotFound { searched_paths } => searched_paths,
            other => panic!("expected LibraryNotFound, got {other:?}"),
        };
        assert_eq!(searched.len(), LIBRARY_FILE_NAMES.len() * 2);
        assert!(searched
            .iter()
            .any(|path| path.to_string_lossy().contains(LIBRARY_LANES_DIR)));
        assert!(searched
            .iter()
            .any(|path| !path.to_string_lossy().contains(LIBRARY_LANES_DIR)));
    }

    // -------------------------------------------------------------------
    // Live smoke against the staged runtime when present (ticket 01
    // staging directory, gitignored). Skips silently elsewhere so CI
    // without the library stays green.
    // -------------------------------------------------------------------

    fn staged_runtime() -> Option<PathBuf> {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("transcribecpp-runtime")
            .join(LIBRARY_FILE_NAMES[0]);
        path.is_file().then_some(path)
    }

    #[test]
    fn staged_library_passes_the_gate_and_reports_contract_fields() {
        let Some(path) = staged_runtime() else {
            eprintln!("staged transcribe.cpp runtime not present; skipping");
            return;
        };
        let library = TranscribeCpp::load_from(&path).expect("staged library loads and passes the gate");
        assert_eq!(library.path(), path);
        assert_eq!(library.version(), "0.4.2");
        let contract = library.contract();
        assert_eq!(contract.version, "0.4.2");
        assert_eq!(contract.header_hash, "f99ad4169374b8f3");
        assert!(contract.backends.contains(&"cpu".to_string()));

        // init_backends fixes the mask process-wide for this test binary,
        // so keep it to this one probe test. CPU must always register.
        library
            .init_backends(ffi::TRANSCRIBE_BACKEND_MASK_ALL)
            .expect("backend modules load");
        assert!(library.device_count() >= 1, "a CPU device must register");
        assert!(library.backend_available(ffi::TRANSCRIBE_BACKEND_CPU));
    }
}
