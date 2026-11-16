//! Error, status and ABI-version types for the audio.cpp FFI layer.
//!
//! The numeric status codes and the ABI versioning rule come from the
//! pinned audio.cpp release header (`include/audiocpp.h`, v0.9.5, ABI
//! 0.3.0). See `docs/research/audio-cpp-c-api.md`.

use std::fmt;
use std::path::PathBuf;

/// ABI major we bind against. Must match the library exactly.
pub const AUDIOCPP_ABI_MAJOR: u8 = 0;
/// Minimum ABI minor we require. Minor releases only add entry points, so
/// any newer minor is accepted.
pub const AUDIOCPP_ABI_MINOR_MIN: u8 = 2;

/// A packed `(major << 16) | (minor << 8) | patch` ABI version as returned
/// by `audiocpp_abi_version()`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AbiVersion {
    pub major: u8,
    pub minor: u8,
    pub patch: u8,
}

impl AbiVersion {
    /// The ABI of the pinned audio.cpp release we bind against (v0.9.5,
    /// header `include/audiocpp.h`: 0.3.0). Runtime loading still accepts
    /// any `0.{AUDIOCPP_ABI_MINOR_MIN}.x` or newer minor, per
    /// [`AbiVersion::is_compatible`].
    pub const PINNED: AbiVersion = AbiVersion {
        major: AUDIOCPP_ABI_MAJOR,
        minor: 3,
        patch: 0,
    };

    pub fn from_packed(packed: u32) -> AbiVersion {
        AbiVersion {
            major: ((packed >> 16) & 0xff) as u8,
            minor: ((packed >> 8) & 0xff) as u8,
            patch: (packed & 0xff) as u8,
        }
    }

    /// Compatibility rule from docs/c_api.md: the major must match, the
    /// minor may only add entry points (so a newer minor is fine), and the
    /// patch must not be gated on.
    pub fn is_compatible(self) -> bool {
        self.major == AUDIOCPP_ABI_MAJOR && self.minor >= AUDIOCPP_ABI_MINOR_MIN
    }
}

impl fmt::Display for AbiVersion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}.{}", self.major, self.minor, self.patch)
    }
}

/// `audiocpp_status` codes from `include/audiocpp.h` (v0.9.5; unchanged
/// since v0.9.4).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(i32)]
pub enum Status {
    Ok = 0,
    InvalidArgument = 1,
    UnsupportedFamily = 2,
    LoadFailed = 3,
    Runtime = 4,
    OutOfMemory = 5,
    OutOfRange = 6,
    NotAvailable = 7,
}

impl Status {
    /// Maps a raw `audiocpp_status` code to its variant. Unknown codes
    /// (from a newer library or memory corruption) return `None`.
    pub fn from_code(code: i32) -> Option<Status> {
        match code {
            0 => Some(Status::Ok),
            1 => Some(Status::InvalidArgument),
            2 => Some(Status::UnsupportedFamily),
            3 => Some(Status::LoadFailed),
            4 => Some(Status::Runtime),
            5 => Some(Status::OutOfMemory),
            6 => Some(Status::OutOfRange),
            7 => Some(Status::NotAvailable),
            _ => None,
        }
    }

    /// The C enum spelling, e.g. `AUDIOCPP_ERR_RUNTIME`.
    pub fn name(self) -> &'static str {
        match self {
            Status::Ok => "AUDIOCPP_OK",
            Status::InvalidArgument => "AUDIOCPP_ERR_INVALID_ARGUMENT",
            Status::UnsupportedFamily => "AUDIOCPP_ERR_UNSUPPORTED_FAMILY",
            Status::LoadFailed => "AUDIOCPP_ERR_LOAD_FAILED",
            Status::Runtime => "AUDIOCPP_ERR_RUNTIME",
            Status::OutOfMemory => "AUDIOCPP_ERR_OUT_OF_MEMORY",
            Status::OutOfRange => "AUDIOCPP_ERR_OUT_OF_RANGE",
            Status::NotAvailable => "AUDIOCPP_ERR_NOT_AVAILABLE",
        }
    }
}

impl fmt::Display for Status {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let text = match self {
            Status::Ok => "ok",
            Status::InvalidArgument => "invalid argument",
            Status::UnsupportedFamily => "unsupported model family",
            Status::LoadFailed => "model load failed",
            Status::Runtime => "runtime error",
            Status::OutOfMemory => "out of memory",
            Status::OutOfRange => "out of range",
            Status::NotAvailable => "not available",
        };
        f.write_str(text)
    }
}

/// Errors of the audio.cpp FFI layer. Everything the rest of the app sees
/// from the native library goes through this type.
#[derive(Debug)]
pub enum AudioCppError {
    /// No candidate library file was found in any search directory.
    LibraryNotFound { searched_paths: Vec<PathBuf> },
    /// A candidate file exists but could not be loaded as a shared library.
    LibraryLoad { path: PathBuf, detail: String },
    /// A required entry point is missing from the library.
    SymbolMissing {
        symbol: &'static str,
        detail: String,
    },
    /// `audiocpp_abi_version()` reported an incompatible ABI.
    IncompatibleAbi { found: AbiVersion },
    /// A native call returned a non-OK status. `detail` carries the
    /// thread-local `audiocpp_last_error()` text of the failing thread.
    Native {
        status: Status,
        detail: Option<String>,
        context: &'static str,
    },
}

impl AudioCppError {
    /// Builds an error from a raw status code plus the thread-local detail
    /// text. Returns `None` for `AUDIOCPP_OK`.
    pub fn from_status_code(
        code: i32,
        detail: Option<String>,
        context: &'static str,
    ) -> Option<AudioCppError> {
        let status = Status::from_code(code)?;
        AudioCppError::from_status(status, detail, context)
    }

    /// Builds an error from a known status. Returns `None` for `Status::Ok`.
    pub fn from_status(
        status: Status,
        detail: Option<String>,
        context: &'static str,
    ) -> Option<AudioCppError> {
        if status == Status::Ok {
            return None;
        }
        Some(AudioCppError::Native {
            status,
            detail,
            context,
        })
    }
}

impl fmt::Display for AudioCppError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AudioCppError::LibraryNotFound { searched_paths } => {
                write!(
                    f,
                    "audio.cpp library not found. Searched: {}",
                    searched_paths
                        .iter()
                        .map(|p| p.display().to_string())
                        .collect::<Vec<_>>()
                        .join(", ")
                )
            }
            AudioCppError::LibraryLoad { path, detail } => {
                write!(
                    f,
                    "failed to load audio.cpp library from {}: {}",
                    path.display(),
                    detail
                )
            }
            AudioCppError::SymbolMissing { symbol, detail } => {
                write!(
                    f,
                    "audio.cpp library is missing required entry point {symbol}: {detail}"
                )
            }
            AudioCppError::IncompatibleAbi { found } => {
                write!(
                    f,
                    "incompatible audio.cpp ABI: library reports {found}, but this build of \
                     Samobranka requires ABI {AUDIOCPP_ABI_MAJOR}.{AUDIOCPP_ABI_MINOR_MIN}.x \
                     (major must match, minor may be newer). Update the audio.cpp library \
                     or the Samobranka audio.cpp integration."
                )
            }
            AudioCppError::Native {
                status,
                detail,
                context,
            } => match detail {
                Some(detail) if !detail.is_empty() => {
                    write!(
                        f,
                        "{context} failed: {} ({status}): {detail}",
                        status.name()
                    )
                }
                _ => write!(f, "{context} failed: {} ({status})", status.name()),
            },
        }
    }
}

impl std::error::Error for AudioCppError {}

#[cfg(test)]
mod tests {
    use super::*;

    // Independent source of truth: include/audiocpp.h @ v0.9.5.
    const HEADER_STATUS_CODES: &[(i32, Status, &str)] = &[
        (0, Status::Ok, "AUDIOCPP_OK"),
        (1, Status::InvalidArgument, "AUDIOCPP_ERR_INVALID_ARGUMENT"),
        (
            2,
            Status::UnsupportedFamily,
            "AUDIOCPP_ERR_UNSUPPORTED_FAMILY",
        ),
        (3, Status::LoadFailed, "AUDIOCPP_ERR_LOAD_FAILED"),
        (4, Status::Runtime, "AUDIOCPP_ERR_RUNTIME"),
        (5, Status::OutOfMemory, "AUDIOCPP_ERR_OUT_OF_MEMORY"),
        (6, Status::OutOfRange, "AUDIOCPP_ERR_OUT_OF_RANGE"),
        (7, Status::NotAvailable, "AUDIOCPP_ERR_NOT_AVAILABLE"),
    ];

    #[test]
    fn status_from_code_maps_all_documented_codes() {
        for &(code, expected, _) in HEADER_STATUS_CODES {
            assert_eq!(Status::from_code(code), Some(expected), "code {code}");
        }
    }

    #[test]
    fn status_from_code_rejects_unknown_codes() {
        for code in [-1, 8, 42, i32::MAX] {
            assert_eq!(Status::from_code(code), None, "code {code}");
        }
    }

    #[test]
    fn status_names_match_header_spelling() {
        for &(code, status, name) in HEADER_STATUS_CODES {
            assert_eq!(status.name(), name, "code {code}");
        }
    }

    #[test]
    fn abi_version_from_packed_unpacks_components() {
        assert_eq!(
            AbiVersion::from_packed(0x00_02_00),
            AbiVersion {
                major: 0,
                minor: 2,
                patch: 0
            }
        );
        assert_eq!(
            AbiVersion::from_packed(0x01_03_07),
            AbiVersion {
                major: 1,
                minor: 3,
                patch: 7
            }
        );
    }

    #[test]
    fn abi_gate_accepts_pinned_newer_minor_and_any_patch() {
        assert!(AbiVersion::PINNED.is_compatible());
        assert!(AbiVersion {
            major: 0,
            minor: 3,
            patch: 0
        }
        .is_compatible());
        assert!(AbiVersion {
            major: 0,
            minor: 2,
            patch: 9
        }
        .is_compatible());
    }

    #[test]
    fn abi_gate_rejects_major_mismatch_and_older_minor() {
        assert!(!AbiVersion {
            major: 1,
            minor: 2,
            patch: 0
        }
        .is_compatible());
        assert!(!AbiVersion {
            major: 0,
            minor: 1,
            patch: 9
        }
        .is_compatible());
    }

    #[test]
    fn from_status_code_returns_none_for_ok() {
        assert!(AudioCppError::from_status_code(0, Some("ignored".into()), "ctx").is_none());
    }

    #[test]
    fn from_status_code_builds_native_error_with_detail() {
        let err = AudioCppError::from_status_code(
            4,
            Some("CUDA out of memory".to_string()),
            "audiocpp_session_run",
        )
        .expect("non-OK status must produce an error");

        match err {
            AudioCppError::Native {
                status,
                detail,
                context,
            } => {
                assert_eq!(status, Status::Runtime);
                assert_eq!(detail.as_deref(), Some("CUDA out of memory"));
                assert_eq!(context, "audiocpp_session_run");
            }
            other => panic!("expected Native error, got {other:?}"),
        }
    }

    #[test]
    fn from_status_code_without_detail_still_builds_error() {
        let err = AudioCppError::from_status_code(7, None, "audiocpp_result_text").expect("error");
        match err {
            AudioCppError::Native { status, detail, .. } => {
                assert_eq!(status, Status::NotAvailable);
                assert_eq!(detail, None);
            }
            other => panic!("expected Native error, got {other:?}"),
        }
    }

    #[test]
    fn native_error_display_contains_status_detail_and_context() {
        let err = AudioCppError::from_status(
            Status::LoadFailed,
            Some("weights header truncated".to_string()),
            "audiocpp_model_load",
        )
        .expect("error");
        let text = err.to_string();
        assert!(text.contains("AUDIOCPP_ERR_LOAD_FAILED"), "text: {text}");
        assert!(text.contains("weights header truncated"), "text: {text}");
        assert!(text.contains("audiocpp_model_load"), "text: {text}");
    }

    #[test]
    fn incompatible_abi_display_shows_found_and_expected() {
        let err = AudioCppError::IncompatibleAbi {
            found: AbiVersion {
                major: 1,
                minor: 0,
                patch: 0,
            },
        };
        let text = err.to_string();
        assert!(text.contains("1.0.0"), "text: {text}");
        assert!(text.contains("0.2"), "text: {text}");
    }

    #[test]
    fn library_not_found_display_lists_searched_paths() {
        let err = AudioCppError::LibraryNotFound {
            searched_paths: vec![PathBuf::from("/opt/samobranka/libaudiocpp.so")],
        };
        let text = err.to_string();
        assert!(text.contains("/opt/samobranka/libaudiocpp.so"), "text: {text}");
    }
}
