//! Raw `extern "C"` bindings to the audio.cpp C ABI.
//!
//! Handwritten against the pinned release header `include/audiocpp.h`
//! (audio.cpp v0.9.5, ABI 0.3.0; minimum 0.2.x) — there are no official
//! Rust bindings.
//! This module is the only place that loads the native library and calls
//! into it; the rest of the app uses the safe wrappers in `super::wrapper`.
//!
//! Contract from the header:
//! - Opaque handles only; no C++ type crosses this boundary.
//! - No exception escapes: every entry point returns `audiocpp_status`,
//!   details live in the thread-local `audiocpp_last_error()`.
//! - `const char *` / `const float *` outputs are borrowed; copy what you
//!   intend to keep.
//! - Handles are not thread-safe individually.

#![allow(non_camel_case_types)]

use std::ffi::{c_char, c_float, c_int, CStr};
use std::path::Path;

use super::error::{AbiVersion, AudioCppError};

// ---------------------------------------------------------------------------
// Opaque handle types
// ---------------------------------------------------------------------------

pub enum audiocpp_registry {}
pub enum audiocpp_model {}
pub enum audiocpp_session {}
pub enum audiocpp_options {}
pub enum audiocpp_request {}
pub enum audiocpp_result {}

// ---------------------------------------------------------------------------
// Plain-old-data structs shared with the C ABI
// ---------------------------------------------------------------------------

/// Model selection. Every field may be NULL; `family_hint = NULL` means the
/// family is auto-detected from the model file.
#[repr(C)]
pub struct audiocpp_model_config {
    pub family_hint: *const c_char,
    pub config_id: *const c_char,
    pub weight_id: *const c_char,
    pub model_spec_override: *const c_char,
}

/// Backend selection. The library never calls `omp_set_num_threads()`; the
/// thread count is controlled here and nowhere else.
#[repr(C)]
pub struct audiocpp_backend_config {
    /// "cpu", "cuda", "hip"/"rocm", "vulkan", "metal", "webgpu" or "best";
    /// NULL means "cpu".
    pub backend: *const c_char,
    pub device: c_int,
    pub threads: c_int,
}

// ---------------------------------------------------------------------------
// Function pointer types
// ---------------------------------------------------------------------------

type FnAbiVersion = unsafe extern "C" fn() -> u32;
type FnBuildVersion = unsafe extern "C" fn() -> *const c_char;
type FnLastError = unsafe extern "C" fn() -> *const c_char;
type FnOptionsCreate = unsafe extern "C" fn() -> *mut audiocpp_options;
type FnOptionsSet =
    unsafe extern "C" fn(*mut audiocpp_options, *const c_char, *const c_char) -> c_int;
type FnOptionsFree = unsafe extern "C" fn(*mut audiocpp_options);
type FnRegistryCreate = unsafe extern "C" fn(*const c_char, *mut *mut audiocpp_registry) -> c_int;
type FnRegistryFree = unsafe extern "C" fn(*mut audiocpp_registry);
type FnRegistryFamilyCount = unsafe extern "C" fn(*const audiocpp_registry) -> usize;
type FnRegistryFamily =
    unsafe extern "C" fn(*const audiocpp_registry, usize, *mut *const c_char) -> c_int;
type FnTaskCount = unsafe extern "C" fn() -> usize;
type FnTaskName = unsafe extern "C" fn(usize) -> *const c_char;
type FnModelLoad = unsafe extern "C" fn(
    *mut audiocpp_registry,
    *const c_char,
    *const audiocpp_model_config,
    *const audiocpp_options,
    *mut *mut audiocpp_model,
) -> c_int;
type FnModelFree = unsafe extern "C" fn(*mut audiocpp_model);
type FnModelFamily = unsafe extern "C" fn(*const audiocpp_model) -> *const c_char;
type FnModelDescription = unsafe extern "C" fn(*const audiocpp_model) -> *const c_char;
type FnModelSupports =
    unsafe extern "C" fn(*const audiocpp_model, *const c_char, *const c_char) -> c_int;
type FnSessionCreate = unsafe extern "C" fn(
    *const audiocpp_model,
    *const c_char,
    *const c_char,
    *const audiocpp_backend_config,
    *const audiocpp_options,
    *mut *mut audiocpp_session,
) -> c_int;
type FnSessionFree = unsafe extern "C" fn(*mut audiocpp_session);
type FnSessionRun = unsafe extern "C" fn(
    *mut audiocpp_session,
    *const audiocpp_request,
    *mut *mut audiocpp_result,
) -> c_int;
type FnRequestCreate = unsafe extern "C" fn() -> *mut audiocpp_request;
type FnRequestFree = unsafe extern "C" fn(*mut audiocpp_request);
type FnRequestSetAudio =
    unsafe extern "C" fn(*mut audiocpp_request, *const c_float, usize, c_int, c_int) -> c_int;
type FnRequestSetOption =
    unsafe extern "C" fn(*mut audiocpp_request, *const c_char, *const c_char) -> c_int;
type FnResultFree = unsafe extern "C" fn(*mut audiocpp_result);
type FnResultSpeakerTurnCount = unsafe extern "C" fn(*const audiocpp_result) -> usize;
type FnResultSpeakerTurn = unsafe extern "C" fn(
    *const audiocpp_result,
    usize,
    *mut i64,
    *mut i64,
    *mut *const c_char,
    *mut f32,
    *mut *const c_char,
) -> c_int;

// ---------------------------------------------------------------------------
// Loaded library
// ---------------------------------------------------------------------------

/// A loaded audio.cpp library with all entry points this integration uses,
/// resolved. Function pointers stay valid for the lifetime of `_library`.
pub struct AudiocppApi {
    _library: libloading::Library,
    pub abi_version: FnAbiVersion,
    pub build_version: FnBuildVersion,
    pub last_error: FnLastError,
    pub options_create: FnOptionsCreate,
    pub options_set: FnOptionsSet,
    pub options_free: FnOptionsFree,
    pub registry_create: FnRegistryCreate,
    pub registry_free: FnRegistryFree,
    pub registry_family_count: FnRegistryFamilyCount,
    pub registry_family: FnRegistryFamily,
    pub task_count: FnTaskCount,
    pub task_name: FnTaskName,
    pub model_load: FnModelLoad,
    pub model_free: FnModelFree,
    pub model_family: FnModelFamily,
    pub model_description: FnModelDescription,
    pub model_supports: FnModelSupports,
    pub session_create: FnSessionCreate,
    pub session_free: FnSessionFree,
    pub session_run: FnSessionRun,
    pub request_create: FnRequestCreate,
    pub request_free: FnRequestFree,
    pub request_set_audio: FnRequestSetAudio,
    pub request_set_option: FnRequestSetOption,
    pub result_free: FnResultFree,
    pub result_speaker_turn_count: FnResultSpeakerTurnCount,
    pub result_speaker_turn: FnResultSpeakerTurn,
}

impl AudiocppApi {
    /// Loads the library at `path` and resolves every entry point the
    /// integration uses.
    ///
    /// # Safety
    ///
    /// Loading a native library and calling its entry points is inherently
    /// unsafe: the file must be a genuine audio.cpp build with a matching
    /// C ABI. Callers must gate on [`AudiocppApi::abi_version`] before use.
    pub unsafe fn load(path: &Path) -> Result<AudiocppApi, AudioCppError> {
        let library = libloading::Library::new(path).map_err(|e| AudioCppError::LibraryLoad {
            path: path.to_path_buf(),
            detail: e.to_string(),
        })?;

        macro_rules! sym {
            ($name:literal, $ty:ty) => {
                *library
                    .get::<$ty>(concat!($name, "\0").as_bytes())
                    .map_err(|e| AudioCppError::SymbolMissing {
                        symbol: $name,
                        detail: e.to_string(),
                    })?
            };
        }

        Ok(AudiocppApi {
            abi_version: sym!("audiocpp_abi_version", FnAbiVersion),
            build_version: sym!("audiocpp_build_version", FnBuildVersion),
            last_error: sym!("audiocpp_last_error", FnLastError),
            options_create: sym!("audiocpp_options_create", FnOptionsCreate),
            options_set: sym!("audiocpp_options_set", FnOptionsSet),
            options_free: sym!("audiocpp_options_free", FnOptionsFree),
            registry_create: sym!("audiocpp_registry_create", FnRegistryCreate),
            registry_free: sym!("audiocpp_registry_free", FnRegistryFree),
            registry_family_count: sym!("audiocpp_registry_family_count", FnRegistryFamilyCount),
            registry_family: sym!("audiocpp_registry_family", FnRegistryFamily),
            task_count: sym!("audiocpp_task_count", FnTaskCount),
            task_name: sym!("audiocpp_task_name", FnTaskName),
            model_load: sym!("audiocpp_model_load", FnModelLoad),
            model_free: sym!("audiocpp_model_free", FnModelFree),
            model_family: sym!("audiocpp_model_family", FnModelFamily),
            model_description: sym!("audiocpp_model_description", FnModelDescription),
            model_supports: sym!("audiocpp_model_supports", FnModelSupports),
            session_create: sym!("audiocpp_session_create", FnSessionCreate),
            session_free: sym!("audiocpp_session_free", FnSessionFree),
            session_run: sym!("audiocpp_session_run", FnSessionRun),
            request_create: sym!("audiocpp_request_create", FnRequestCreate),
            request_free: sym!("audiocpp_request_free", FnRequestFree),
            request_set_audio: sym!("audiocpp_request_set_audio", FnRequestSetAudio),
            request_set_option: sym!("audiocpp_request_set_option", FnRequestSetOption),
            result_free: sym!("audiocpp_result_free", FnResultFree),
            result_speaker_turn_count: sym!(
                "audiocpp_result_speaker_turn_count",
                FnResultSpeakerTurnCount
            ),
            result_speaker_turn: sym!("audiocpp_result_speaker_turn", FnResultSpeakerTurn),
            _library: library,
        })
    }

    /// Packed ABI version reported by the library.
    pub fn abi_version(&self) -> AbiVersion {
        AbiVersion::from_packed(unsafe { (self.abi_version)() })
    }

    /// audio.cpp's own build version, e.g. "0.9.5".
    pub fn build_version(&self) -> String {
        unsafe { borrowed_str((self.build_version)()) }.unwrap_or_default()
    }

    /// Detail for the most recent failure on THIS thread, read from the
    /// thread-local `audiocpp_last_error()`. `None` when empty.
    pub fn last_error(&self) -> Option<String> {
        unsafe { borrowed_str((self.last_error)()) }.filter(|s| !s.is_empty())
    }

    /// Turns a non-OK `audiocpp_status` into an `AudioCppError` carrying
    /// this thread's last-error detail. `Ok(())` for `AUDIOCPP_OK`.
    pub fn check(&self, code: c_int, context: &'static str) -> Result<(), AudioCppError> {
        match AudioCppError::from_status_code(code, self.last_error(), context) {
            None => Ok(()),
            Some(err) => Err(err),
        }
    }
}

/// Copies a borrowed C string; None for NULL.
///
/// # Safety
/// `ptr` must be NULL or a valid NUL-terminated string.
unsafe fn borrowed_str(ptr: *const c_char) -> Option<String> {
    if ptr.is_null() {
        return None;
    }
    Some(
        unsafe { CStr::from_ptr(ptr) }
            .to_string_lossy()
            .into_owned(),
    )
}
