//! Error and status types for the transcribe.cpp FFI layer.
//!
//! The numeric status codes come from the pinned transcribe.cpp release
//! header (`include/transcribe.h`, v0.4.2, header hash `f99ad4169374b8f3`).
//! Unlike audio.cpp there is no thread-local last-error function: detail
//! text comes from `transcribe_status_string(int)` (see `super::ffi`).

use std::fmt;
use std::path::PathBuf;

/// Library version this build of Samobranka is pinned against. Must match
/// `contract.json`'s `version` next to the native library.
pub const TRANSCRIBECPP_PINNED_VERSION: &str = "0.4.2";

/// Hash oracle of `include/transcribe.h` at the pinned release
/// (`include/transcribe.abihash`). Must match `contract.json`'s
/// `header_hash` next to the native library.
pub const TRANSCRIBECPP_PINNED_HEADER_HASH: &str = "f99ad4169374b8f3";

/// `transcribe_status` codes from `include/transcribe.h` (v0.4.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(i32)]
pub enum Status {
    Ok = 0,
    InvalidArg = 1,
    NotImplemented = 2,
    FileNotFound = 3,
    Gguf = 4,
    UnsupportedArch = 5,
    UnsupportedVariant = 6,
    OutOfMemory = 7,
    Backend = 8,
    SampleRate = 9,
    UnsupportedLanguage = 10,
    UnsupportedTask = 11,
    UnsupportedTimestamps = 12,
    Aborted = 13,
    BadStructSize = 14,
    UnsupportedPnc = 15,
    UnsupportedItn = 16,
    InputTooLong = 17,
    OutputTruncated = 18,
    OutputRepetition = 19,
    UnsupportedRole = 20,
    InputTooShort = 21,
}

impl Status {
    /// Maps a raw `transcribe_status` code to its variant. Unknown codes
    /// (from a newer library or memory corruption) return `None`.
    pub fn from_code(code: i32) -> Option<Status> {
        match code {
            0 => Some(Status::Ok),
            1 => Some(Status::InvalidArg),
            2 => Some(Status::NotImplemented),
            3 => Some(Status::FileNotFound),
            4 => Some(Status::Gguf),
            5 => Some(Status::UnsupportedArch),
            6 => Some(Status::UnsupportedVariant),
            7 => Some(Status::OutOfMemory),
            8 => Some(Status::Backend),
            9 => Some(Status::SampleRate),
            10 => Some(Status::UnsupportedLanguage),
            11 => Some(Status::UnsupportedTask),
            12 => Some(Status::UnsupportedTimestamps),
            13 => Some(Status::Aborted),
            14 => Some(Status::BadStructSize),
            15 => Some(Status::UnsupportedPnc),
            16 => Some(Status::UnsupportedItn),
            17 => Some(Status::InputTooLong),
            18 => Some(Status::OutputTruncated),
            19 => Some(Status::OutputRepetition),
            20 => Some(Status::UnsupportedRole),
            21 => Some(Status::InputTooShort),
            _ => None,
        }
    }

    /// The C enum spelling, e.g. `TRANSCRIBE_ERR_BACKEND`.
    pub fn name(self) -> &'static str {
        match self {
            Status::Ok => "TRANSCRIBE_OK",
            Status::InvalidArg => "TRANSCRIBE_ERR_INVALID_ARG",
            Status::NotImplemented => "TRANSCRIBE_ERR_NOT_IMPLEMENTED",
            Status::FileNotFound => "TRANSCRIBE_ERR_FILE_NOT_FOUND",
            Status::Gguf => "TRANSCRIBE_ERR_GGUF",
            Status::UnsupportedArch => "TRANSCRIBE_ERR_UNSUPPORTED_ARCH",
            Status::UnsupportedVariant => "TRANSCRIBE_ERR_UNSUPPORTED_VARIANT",
            Status::OutOfMemory => "TRANSCRIBE_ERR_OOM",
            Status::Backend => "TRANSCRIBE_ERR_BACKEND",
            Status::SampleRate => "TRANSCRIBE_ERR_SAMPLE_RATE",
            Status::UnsupportedLanguage => "TRANSCRIBE_ERR_UNSUPPORTED_LANGUAGE",
            Status::UnsupportedTask => "TRANSCRIBE_ERR_UNSUPPORTED_TASK",
            Status::UnsupportedTimestamps => "TRANSCRIBE_ERR_UNSUPPORTED_TIMESTAMPS",
            Status::Aborted => "TRANSCRIBE_ERR_ABORTED",
            Status::BadStructSize => "TRANSCRIBE_ERR_BAD_STRUCT_SIZE",
            Status::UnsupportedPnc => "TRANSCRIBE_ERR_UNSUPPORTED_PNC",
            Status::UnsupportedItn => "TRANSCRIBE_ERR_UNSUPPORTED_ITN",
            Status::InputTooLong => "TRANSCRIBE_ERR_INPUT_TOO_LONG",
            Status::OutputTruncated => "TRANSCRIBE_ERR_OUTPUT_TRUNCATED",
            Status::OutputRepetition => "TRANSCRIBE_ERR_OUTPUT_REPETITION",
            Status::UnsupportedRole => "TRANSCRIBE_ERR_UNSUPPORTED_ROLE",
            Status::InputTooShort => "TRANSCRIBE_ERR_INPUT_TOO_SHORT",
        }
    }
}

impl fmt::Display for Status {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let text = match self {
            Status::Ok => "ok",
            Status::InvalidArg => "invalid argument",
            Status::NotImplemented => "not implemented",
            Status::FileNotFound => "file not found",
            Status::Gguf => "invalid GGUF model file",
            Status::UnsupportedArch => "unsupported model architecture",
            Status::UnsupportedVariant => "unsupported model variant",
            Status::OutOfMemory => "out of memory",
            Status::Backend => "compute backend failure",
            Status::SampleRate => "unsupported sample rate",
            Status::UnsupportedLanguage => "unsupported language",
            Status::UnsupportedTask => "unsupported task",
            Status::UnsupportedTimestamps => "unsupported timestamps",
            Status::Aborted => "run aborted",
            Status::BadStructSize => "bad params struct size",
            Status::UnsupportedPnc => "unsupported punctuation restoration",
            Status::UnsupportedItn => "unsupported inverse text normalization",
            Status::InputTooLong => "audio input too long",
            Status::OutputTruncated => "output truncated",
            Status::OutputRepetition => "output repetition loop",
            Status::UnsupportedRole => "unsupported model role",
            Status::InputTooShort => "audio input too short",
        };
        f.write_str(text)
    }
}

/// Errors of the transcribe.cpp FFI layer. Everything the rest of the app
/// sees from the native library goes through this type.
#[derive(Debug)]
pub enum TranscribeCppError {
    /// No candidate library file was found in any search directory.
    LibraryNotFound { searched_paths: Vec<PathBuf> },
    /// A candidate file exists but could not be loaded as a shared library.
    LibraryLoad { path: PathBuf, detail: String },
    /// A required entry point is missing from the library.
    SymbolMissing {
        symbol: &'static str,
        detail: String,
    },
    /// The `contract.json` next to the library is missing, unreadable, or
    /// disagrees with the ABI pinned at compile time. Raised before any
    /// native call is made.
    IncompatibleAbi {
        /// What the contract (or its absence) reported, e.g. the version
        /// and header hash found on disk.
        found: String,
        /// User-facing explanation of the mismatch.
        detail: String,
    },
    /// A native call returned a non-OK status. `detail` carries the
    /// `transcribe_status_string()` text of the failing call where the
    /// library was already loaded.
    Native {
        status: Status,
        detail: Option<String>,
        context: &'static str,
    },
}

impl TranscribeCppError {
    /// Builds an error from a raw status code plus detail text. Returns
    /// `None` for `TRANSCRIBE_OK`. Unknown codes are reported as a native
    /// error with a synthetic detail (a newer library may add codes; the
    /// raw number stays visible to the user).
    pub fn from_status_code(
        code: i32,
        detail: Option<String>,
        context: &'static str,
    ) -> Option<TranscribeCppError> {
        if code == 0 {
            return None;
        }
        let status = Status::from_code(code).unwrap_or(Status::InvalidArg);
        let detail = match detail {
            Some(detail) => Some(detail),
            None if Status::from_code(code).is_none() => {
                Some(format!("unknown status code {code}"))
            }
            None => None,
        };
        Some(TranscribeCppError::Native {
            status,
            detail,
            context,
        })
    }
}

impl fmt::Display for TranscribeCppError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TranscribeCppError::LibraryNotFound { searched_paths } => {
                write!(
                    f,
                    "transcribe.cpp library not found. Searched: {}",
                    searched_paths
                        .iter()
                        .map(|p| p.display().to_string())
                        .collect::<Vec<_>>()
                        .join(", ")
                )
            }
            TranscribeCppError::LibraryLoad { path, detail } => {
                write!(
                    f,
                    "failed to load transcribe.cpp library from {}: {}",
                    path.display(),
                    detail
                )
            }
            TranscribeCppError::SymbolMissing { symbol, detail } => {
                write!(
                    f,
                    "transcribe.cpp library is missing required entry point {symbol}: {detail}"
                )
            }
            TranscribeCppError::IncompatibleAbi { found, detail } => {
                write!(
                    f,
                    "incompatible transcribe.cpp ABI: {detail} (library reports {found}, but \
                     this build of Samobranka requires version \
                     {TRANSCRIBECPP_PINNED_VERSION} with header hash \
                     {TRANSCRIBECPP_PINNED_HEADER_HASH}). Update the transcribe.cpp runtime \
                     or the Samobranka transcribe.cpp integration."
                )
            }
            TranscribeCppError::Native {
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

impl std::error::Error for TranscribeCppError {}

#[cfg(test)]
mod tests {
    use super::*;

    // Independent source of truth: include/transcribe.h @ v0.4.2
    // (native-facts.md §5), cross-checked against the staged library.
    const HEADER_STATUS_CODES: &[(i32, Status, &str)] = &[
        (0, Status::Ok, "TRANSCRIBE_OK"),
        (1, Status::InvalidArg, "TRANSCRIBE_ERR_INVALID_ARG"),
        (2, Status::NotImplemented, "TRANSCRIBE_ERR_NOT_IMPLEMENTED"),
        (3, Status::FileNotFound, "TRANSCRIBE_ERR_FILE_NOT_FOUND"),
        (4, Status::Gguf, "TRANSCRIBE_ERR_GGUF"),
        (5, Status::UnsupportedArch, "TRANSCRIBE_ERR_UNSUPPORTED_ARCH"),
        (6, Status::UnsupportedVariant, "TRANSCRIBE_ERR_UNSUPPORTED_VARIANT"),
        (7, Status::OutOfMemory, "TRANSCRIBE_ERR_OOM"),
        (8, Status::Backend, "TRANSCRIBE_ERR_BACKEND"),
        (9, Status::SampleRate, "TRANSCRIBE_ERR_SAMPLE_RATE"),
        (10, Status::UnsupportedLanguage, "TRANSCRIBE_ERR_UNSUPPORTED_LANGUAGE"),
        (11, Status::UnsupportedTask, "TRANSCRIBE_ERR_UNSUPPORTED_TASK"),
        (
            12,
            Status::UnsupportedTimestamps,
            "TRANSCRIBE_ERR_UNSUPPORTED_TIMESTAMPS",
        ),
        (13, Status::Aborted, "TRANSCRIBE_ERR_ABORTED"),
        (14, Status::BadStructSize, "TRANSCRIBE_ERR_BAD_STRUCT_SIZE"),
        (15, Status::UnsupportedPnc, "TRANSCRIBE_ERR_UNSUPPORTED_PNC"),
        (16, Status::UnsupportedItn, "TRANSCRIBE_ERR_UNSUPPORTED_ITN"),
        (17, Status::InputTooLong, "TRANSCRIBE_ERR_INPUT_TOO_LONG"),
        (18, Status::OutputTruncated, "TRANSCRIBE_ERR_OUTPUT_TRUNCATED"),
        (19, Status::OutputRepetition, "TRANSCRIBE_ERR_OUTPUT_REPETITION"),
        (20, Status::UnsupportedRole, "TRANSCRIBE_ERR_UNSUPPORTED_ROLE"),
        (21, Status::InputTooShort, "TRANSCRIBE_ERR_INPUT_TOO_SHORT"),
    ];

    #[test]
    fn status_from_code_maps_all_documented_codes() {
        for &(code, expected, _) in HEADER_STATUS_CODES {
            assert_eq!(Status::from_code(code), Some(expected), "code {code}");
        }
    }

    #[test]
    fn status_from_code_rejects_unknown_codes() {
        for code in [-1, 22, 42, i32::MAX] {
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
    fn from_status_code_returns_none_for_ok() {
        assert!(
            TranscribeCppError::from_status_code(0, Some("ignored".into()), "ctx").is_none()
        );
    }

    #[test]
    fn from_status_code_builds_native_error_with_detail() {
        let err = TranscribeCppError::from_status_code(
            8,
            Some("Vulkan device lost".to_string()),
            "transcribe_model_load_file",
        )
        .expect("non-OK status must produce an error");

        match err {
            TranscribeCppError::Native {
                status,
                detail,
                context,
            } => {
                assert_eq!(status, Status::Backend);
                assert_eq!(detail.as_deref(), Some("Vulkan device lost"));
                assert_eq!(context, "transcribe_model_load_file");
            }
            other => panic!("expected Native error, got {other:?}"),
        }
    }

    #[test]
    fn from_status_code_keeps_unknown_codes_visible() {
        let err = TranscribeCppError::from_status_code(99, None, "transcribe_run")
            .expect("unknown non-zero codes are still errors");
        let text = err.to_string();
        assert!(text.contains("99"), "text: {text}");
    }

    #[test]
    fn native_error_display_contains_status_detail_and_context() {
        let err = TranscribeCppError::from_status_code(
            4,
            Some("weights header truncated".to_string()),
            "transcribe_model_load_file",
        )
        .expect("error");
        let text = err.to_string();
        assert!(text.contains("TRANSCRIBE_ERR_GGUF"), "text: {text}");
        assert!(text.contains("weights header truncated"), "text: {text}");
        assert!(text.contains("transcribe_model_load_file"), "text: {text}");
    }

    #[test]
    fn incompatible_abi_display_shows_found_expected_and_pin() {
        let err = TranscribeCppError::IncompatibleAbi {
            found: "version 0.4.1, header_hash 0123456789abcdef".to_string(),
            detail: "version mismatch".to_string(),
        };
        let text = err.to_string();
        assert!(text.contains("0.4.1"), "text: {text}");
        assert!(text.contains("0123456789abcdef"), "text: {text}");
        assert!(text.contains("0.4.2"), "text: {text}");
        assert!(text.contains("f99ad4169374b8f3"), "text: {text}");
        assert!(text.contains("incompatible transcribe.cpp ABI"), "text: {text}");
    }

    #[test]
    fn library_not_found_display_lists_searched_paths() {
        let err = TranscribeCppError::LibraryNotFound {
            searched_paths: vec![PathBuf::from("/opt/samobranka/libtranscribe.so")],
        };
        let text = err.to_string();
        assert!(text.contains("/opt/samobranka/libtranscribe.so"), "text: {text}");
    }
}
