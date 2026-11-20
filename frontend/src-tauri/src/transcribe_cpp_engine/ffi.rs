//! Raw `extern "C"` bindings to the transcribe.cpp C ABI.
//!
//! Handwritten against the pinned release header `include/transcribe.h`
//! (transcribe.cpp v0.4.2, header hash `f99ad4169374b8f3`) — there are no
//! official Rust bindings. This module is the only place that loads the
//! native library and calls into it; the rest of the app uses the safe
//! wrappers in `super::wrapper`.
//!
//! Contract from the header:
//! - Opaque handles only; no C++ type crosses this boundary.
//! - No exception escapes: every entry point returns `transcribe_status`,
//!   human-readable detail comes from `transcribe_status_string(int)`
//!   (there is NO thread-local last-error function in this ABI).
//! - `const char *` outputs are borrowed; copy what you intend to keep.
//! - Model functions are thread-safe; session functions are NOT: one
//!   session is used by at most one thread at a time.
//! - Every caller-owned params struct carries `struct_size` as field 0 and
//!   MUST be initialized via its `transcribe_*_init()` function
//!   (`struct_size == 0` is rejected with `TRANSCRIBE_ERR_BAD_STRUCT_SIZE`).
//!
//! The symbol set is deliberately minimal: exactly what backend selection,
//! one offline run, and diagnostics need. Adding an entry point is a
//! one-line `sym!` entry plus its function-pointer type (ticket 05 may add
//! result-detail accessors such as words/tokens/timings).

#![allow(non_camel_case_types)]

use std::ffi::{c_char, c_int, c_void, CStr};
use std::path::Path;

use super::error::TranscribeCppError;

// ---------------------------------------------------------------------------
// Opaque handle types
// ---------------------------------------------------------------------------

pub enum transcribe_model {}
pub enum transcribe_session {}
/// Process-local compute device, owned by the runtime; never freed by the
/// caller. `NULL` doubles as the automatic-selection sentinel in
/// `transcribe_model_load_params::device`.
pub enum transcribe_device {}

// ---------------------------------------------------------------------------
// Enum values (transcribe.h v0.4.2)
// ---------------------------------------------------------------------------

/// `transcribe_backend_request`.
pub const TRANSCRIBE_BACKEND_AUTO: c_int = 0;
pub const TRANSCRIBE_BACKEND_CPU: c_int = 1;
#[allow(dead_code)] // metal/cuda/rocm are not in the pinned linux/windows lane
pub const TRANSCRIBE_BACKEND_VULKAN: c_int = 3;

/// `transcribe_allowed_backends` mask bits.
pub const TRANSCRIBE_BACKEND_MASK_CPU: u32 = 0x0000_0001;
#[allow(dead_code)] // the pinned lanes have no Metal build
pub const TRANSCRIBE_BACKEND_MASK_METAL: u32 = 0x0000_0002;
pub const TRANSCRIBE_BACKEND_MASK_VULKAN: u32 = 0x0000_0004;
pub const TRANSCRIBE_BACKEND_MASK_ALL: u32 = 0xFFFF_FFFF;

/// `transcribe_timestamp_kind` — offline runs request the richest the
/// model can produce.
pub const TRANSCRIBE_TIMESTAMPS_AUTO: c_int = 1;

/// `transcribe_device_type` (CPU/GPU/IGPU/ACCEL axis).
#[allow(dead_code)] // kept for completeness; selection filters on `kind`
pub const TRANSCRIBE_DEVICE_TYPE_CPU: c_int = 0;

// ---------------------------------------------------------------------------
// Plain-old-data structs shared with the C ABI
// ---------------------------------------------------------------------------

/// Params for `transcribe_init_backends_ex`. Must be built via
/// `transcribe_backend_init_params_init`.
#[repr(C)]
pub struct transcribe_backend_init_params {
    pub struct_size: u64,
    /// NULL: package-local default (the directory of the loaded library).
    pub artifact_dir: *const c_char,
    /// `TRANSCRIBE_BACKEND_MASK_*`; fixed at the first registration.
    pub allowed_backends: u32,
}

/// One registered compute device, filled by `transcribe_device_get_info`.
/// The string fields are borrowed into runtime-owned storage.
#[repr(C)]
pub struct transcribe_device_info {
    pub struct_size: u64,
    pub name: *const c_char,
    pub description: *const c_char,
    /// "cpu", "accel", "metal", "vulkan", "cuda", "rocm", "sycl", "gpu",
    /// "unknown".
    pub kind: *const c_char,
    pub device_id: *const c_char,
    pub memory_total: u64,
    pub memory_free: u64,
    pub device_type: c_int,
}

/// Params for `transcribe_model_load_file`. Must be built via
/// `transcribe_model_load_params_init`.
#[repr(C)]
pub struct transcribe_model_load_params {
    pub struct_size: u64,
    /// `transcribe_backend_request`.
    pub backend: c_int,
    /// NULL = automatic selection for the backend.
    pub device: *mut transcribe_device,
    pub no_mmap: bool,
}

/// Params for `transcribe_session_init`. Must be built via
/// `transcribe_session_params_init`.
#[repr(C)]
pub struct transcribe_session_params {
    pub struct_size: u64,
    pub n_threads: c_int,
    /// `transcribe_kv_type` (AUTO = 0).
    pub kv_type: c_int,
    pub n_ctx: i32,
}

/// Params for `transcribe_run`. Must be built via
/// `transcribe_run_params_init`. Only the ASR-relevant fields are typed
/// here; the rest keep their init() defaults.
#[repr(C)]
pub struct transcribe_run_params {
    pub struct_size: u64,
    /// `transcribe_task` (TRANSCRIBE = 0).
    pub task: c_int,
    /// `transcribe_timestamp_kind`.
    pub timestamps: c_int,
    /// `transcribe_pnc_mode` (DEFAULT = 0).
    pub pnc: c_int,
    /// `transcribe_itn_mode` (DEFAULT = 0).
    pub itn: c_int,
    /// `transcribe_diarize_mode` (DEFAULT = 0).
    pub diarize: c_int,
    /// BCP-47-ish hint, or NULL to autodetect. The library copies it
    /// before the call returns.
    pub language: *const c_char,
    pub target_language: *const c_char,
    pub keep_special_tags: bool,
    /// Family extension pointer; NULL selects family defaults.
    pub family: *const c_void,
    pub spec_k_drafts: i32,
    pub vocabulary: *const *const c_char,
    pub n_vocabulary: i32,
    pub prompt: *const c_char,
    pub prefix: *const c_char,
}

/// One transcript segment, filled by `transcribe_get_segment`. `text` is
/// borrowed session-owned storage; the timestamps are milliseconds
/// relative to the original input audio.
#[repr(C)]
pub struct transcribe_segment {
    pub struct_size: u64,
    pub t0_ms: i64,
    pub t1_ms: i64,
    pub first_word: c_int,
    pub n_words: c_int,
    pub first_token: c_int,
    pub n_tokens: c_int,
    pub text: *const c_char,
    /// 1-based; 0 = no speaker attribution.
    pub speaker_id: i32,
}

// ---------------------------------------------------------------------------
// Function pointer types
// ---------------------------------------------------------------------------

type FnVersion = unsafe extern "C" fn() -> *const c_char;
type FnStatusString = unsafe extern "C" fn(status: c_int) -> *const c_char;
type FnBackendInitParamsInit =
    unsafe extern "C" fn(*mut transcribe_backend_init_params);
type FnInitBackendsEx =
    unsafe extern "C" fn(*const transcribe_backend_init_params) -> c_int;
type FnBackendAvailable = unsafe extern "C" fn(kind: c_int) -> bool;
type FnDeviceCount = unsafe extern "C" fn() -> c_int;
type FnDeviceGet = unsafe extern "C" fn(index: c_int) -> *mut transcribe_device;
type FnDeviceInfoInit = unsafe extern "C" fn(*mut transcribe_device_info);
type FnDeviceGetInfo =
    unsafe extern "C" fn(*mut transcribe_device, *mut transcribe_device_info) -> c_int;
type FnModelLoadParamsInit = unsafe extern "C" fn(*mut transcribe_model_load_params);
type FnModelLoadFile = unsafe extern "C" fn(
    *const c_char,
    *const transcribe_model_load_params,
    *mut *mut transcribe_model,
) -> c_int;
type FnModelFree = unsafe extern "C" fn(*mut transcribe_model);
type FnModelBackend = unsafe extern "C" fn(*const transcribe_model) -> *const c_char;
type FnSessionParamsInit = unsafe extern "C" fn(*mut transcribe_session_params);
type FnSessionInit = unsafe extern "C" fn(
    *mut transcribe_model,
    *const transcribe_session_params,
    *mut *mut transcribe_session,
) -> c_int;
type FnSessionFree = unsafe extern "C" fn(*mut transcribe_session);
type FnRunParamsInit = unsafe extern "C" fn(*mut transcribe_run_params);
type FnRun = unsafe extern "C" fn(
    *mut transcribe_session,
    *const f32,
    c_int,
    *const transcribe_run_params,
) -> c_int;
type FnFullText = unsafe extern "C" fn(*const transcribe_session) -> *const c_char;
type FnNSegments = unsafe extern "C" fn(*const transcribe_session) -> c_int;
type FnSegmentInit = unsafe extern "C" fn(*mut transcribe_segment);
type FnGetSegment = unsafe extern "C" fn(
    *const transcribe_session,
    c_int,
    *mut transcribe_segment,
) -> c_int;
type FnDetectedLanguage = unsafe extern "C" fn(*const transcribe_session) -> *const c_char;

// ---------------------------------------------------------------------------
// Loaded library
// ---------------------------------------------------------------------------

/// A loaded transcribe.cpp library with all entry points this integration
/// uses, resolved. Function pointers stay valid for the lifetime of
/// `_library`.
pub struct TranscribecppApi {
    _library: libloading::Library,
    pub version: FnVersion,
    pub status_string: FnStatusString,
    pub backend_init_params_init: FnBackendInitParamsInit,
    pub init_backends_ex: FnInitBackendsEx,
    pub backend_available: FnBackendAvailable,
    pub device_count: FnDeviceCount,
    pub device_get: FnDeviceGet,
    pub device_info_init: FnDeviceInfoInit,
    pub device_get_info: FnDeviceGetInfo,
    pub model_load_params_init: FnModelLoadParamsInit,
    pub model_load_file: FnModelLoadFile,
    pub model_free: FnModelFree,
    pub model_backend: FnModelBackend,
    pub session_params_init: FnSessionParamsInit,
    pub session_init: FnSessionInit,
    pub session_free: FnSessionFree,
    pub run_params_init: FnRunParamsInit,
    pub run: FnRun,
    pub full_text: FnFullText,
    pub n_segments: FnNSegments,
    pub segment_init: FnSegmentInit,
    pub get_segment: FnGetSegment,
    pub detected_language: FnDetectedLanguage,
}

impl TranscribecppApi {
    /// Loads the library at `path` and resolves every entry point the
    /// integration uses. A missing symbol yields
    /// [`TranscribeCppError::SymbolMissing`] naming it.
    ///
    /// # Safety
    ///
    /// Loading a native library and calling its entry points is inherently
    /// unsafe: the file must be a genuine transcribe.cpp build with a
    /// matching C ABI. Callers must gate on the ABI contract (see
    /// [`super::wrapper`]) before use.
    pub unsafe fn load(path: &Path) -> Result<TranscribecppApi, TranscribeCppError> {
        let library = libloading::Library::new(path).map_err(|e| TranscribeCppError::LibraryLoad {
            path: path.to_path_buf(),
            detail: e.to_string(),
        })?;

        macro_rules! sym {
            ($name:literal, $ty:ty) => {
                *library
                    .get::<$ty>(concat!($name, "\0").as_bytes())
                    .map_err(|e| TranscribeCppError::SymbolMissing {
                        symbol: $name,
                        detail: e.to_string(),
                    })?
            };
        }

        Ok(TranscribecppApi {
            version: sym!("transcribe_version", FnVersion),
            status_string: sym!("transcribe_status_string", FnStatusString),
            backend_init_params_init: sym!(
                "transcribe_backend_init_params_init",
                FnBackendInitParamsInit
            ),
            init_backends_ex: sym!("transcribe_init_backends_ex", FnInitBackendsEx),
            backend_available: sym!("transcribe_backend_available", FnBackendAvailable),
            device_count: sym!("transcribe_device_count", FnDeviceCount),
            device_get: sym!("transcribe_device_get", FnDeviceGet),
            device_info_init: sym!("transcribe_device_info_init", FnDeviceInfoInit),
            device_get_info: sym!("transcribe_device_get_info", FnDeviceGetInfo),
            model_load_params_init: sym!("transcribe_model_load_params_init", FnModelLoadParamsInit),
            model_load_file: sym!("transcribe_model_load_file", FnModelLoadFile),
            model_free: sym!("transcribe_model_free", FnModelFree),
            model_backend: sym!("transcribe_model_backend", FnModelBackend),
            session_params_init: sym!("transcribe_session_params_init", FnSessionParamsInit),
            session_init: sym!("transcribe_session_init", FnSessionInit),
            session_free: sym!("transcribe_session_free", FnSessionFree),
            run_params_init: sym!("transcribe_run_params_init", FnRunParamsInit),
            run: sym!("transcribe_run", FnRun),
            full_text: sym!("transcribe_full_text", FnFullText),
            n_segments: sym!("transcribe_n_segments", FnNSegments),
            segment_init: sym!("transcribe_segment_init", FnSegmentInit),
            get_segment: sym!("transcribe_get_segment", FnGetSegment),
            detected_language: sym!("transcribe_detected_language", FnDetectedLanguage),
            _library: library,
        })
    }

    /// transcribe.cpp's own version string, e.g. "0.4.2".
    pub fn version(&self) -> String {
        unsafe { borrowed_str((self.version)()) }.unwrap_or_default()
    }

    /// Static description of a status code from the library itself
    /// (`transcribe_status_string`); `None` when the pointer is NULL or
    /// empty.
    pub fn status_string(&self, code: c_int) -> Option<String> {
        unsafe { borrowed_str((self.status_string)(code)) }.filter(|s| !s.is_empty())
    }

    /// Turns a non-OK `transcribe_status` into a
    /// [`TranscribeCppError::Native`] carrying the library's own
    /// status-string detail. `Ok(())` for `TRANSCRIBE_OK`.
    pub fn check(&self, code: c_int, context: &'static str) -> Result<(), TranscribeCppError> {
        match TranscribeCppError::from_status_code(code, self.status_string(code), context) {
            None => Ok(()),
            Some(err) => Err(err),
        }
    }
}

/// Copies a borrowed C string; None for NULL.
///
/// # Safety
/// `ptr` must be NULL or a valid NUL-terminated string.
pub(crate) unsafe fn borrowed_str(ptr: *const c_char) -> Option<String> {
    if ptr.is_null() {
        return None;
    }
    Some(
        unsafe { CStr::from_ptr(ptr) }
            .to_string_lossy()
            .into_owned(),
    )
}
