//! Tauri command reporting the state of the transcribe.cpp native runtime.
//!
//! Mirrors audio.cpp's `audiocpp_diagnostics`: the command never fails —
//! a missing or ABI-incompatible library is reported in-band as
//! `available: false` with a user-facing `error` string, so the UI can
//! show a runtime warning instead of an exception.

use serde::Serialize;
use tauri::command;

use super::error::TranscribeCppError;
use super::wrapper::TranscribeCpp;

/// Diagnostic report on the transcribe.cpp native library: whether it
/// loaded, where it came from, its pinned version and header hash, and
/// the compute backends the lane ships.
#[derive(Debug, Clone, Serialize)]
pub struct TranscribeCppDiagnostics {
    pub available: bool,
    pub library_path: Option<String>,
    pub version: Option<String>,
    pub header_hash: Option<String>,
    pub backends: Vec<String>,
    pub error: Option<String>,
}

impl TranscribeCppDiagnostics {
    /// Report for a library that could not be loaded or interrogated; the
    /// error text explains why in user-facing terms.
    pub fn unavailable(error: &TranscribeCppError) -> Self {
        TranscribeCppDiagnostics {
            available: false,
            library_path: None,
            version: None,
            header_hash: None,
            backends: Vec::new(),
            error: Some(error.to_string()),
        }
    }

    fn from_library(library: &TranscribeCpp) -> Self {
        let contract = library.contract();
        TranscribeCppDiagnostics {
            available: true,
            library_path: Some(library.path().to_string_lossy().into_owned()),
            // The gate already proved contract and transcribe_version()
            // agree; report the library's own answer.
            version: Some(library.version()),
            header_hash: Some(contract.header_hash.clone()),
            backends: contract.backends.clone(),
            error: None,
        }
    }
}

/// Diagnostic command: loads the transcribe.cpp library (if present) and
/// reports its path, version, header hash, and available backends. A
/// missing or ABI-incompatible library is reported in-band
/// (`available: false` with an explanatory `error`), never as a command
/// failure or a crash.
#[command]
pub async fn transcribecpp_diagnostics() -> Result<TranscribeCppDiagnostics, String> {
    // Library loading and contract reads block; keep them off the async
    // runtime (transcribe.h: every entry point blocks).
    tokio::task::spawn_blocking(collect_diagnostics)
        .await
        .map_err(|e| format!("transcribe.cpp diagnostics task failed: {e}"))
}

fn collect_diagnostics() -> TranscribeCppDiagnostics {
    match TranscribeCpp::load() {
        Ok(library) => TranscribeCppDiagnostics::from_library(&library),
        Err(error) => TranscribeCppDiagnostics::unavailable(&error),
    }
}

/// Detects native-runtime availability through the same probe the
/// diagnostics command uses, off the async runtime.
///
/// The probe loads the library and drops it again; transcription still
/// loads it for the pass itself. Sharing [`collect_diagnostics`] keeps one
/// definition of "available" instead of a second ad-hoc load path.
pub async fn detect_runtime_available() -> bool {
    tokio::task::spawn_blocking(|| collect_diagnostics().available)
        .await
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn unavailable_report_carries_the_error_and_no_fields() {
        let error = TranscribeCppError::LibraryNotFound {
            searched_paths: vec![PathBuf::from("/usr/bin/libtranscribe.so")],
        };
        let report = TranscribeCppDiagnostics::unavailable(&error);
        assert!(!report.available);
        assert_eq!(report.library_path, None);
        assert_eq!(report.version, None);
        assert_eq!(report.header_hash, None);
        assert!(report.backends.is_empty());
        assert!(report
            .error
            .as_deref()
            .is_some_and(|text| text.contains("libtranscribe.so")));
    }

    #[test]
    fn unavailable_report_serializes_with_the_shared_json_shape() {
        let error = TranscribeCppError::IncompatibleAbi {
            found: "version 0.4.1".into(),
            detail: "library version mismatch".into(),
        };
        let report = TranscribeCppDiagnostics::unavailable(&error);
        let json = serde_json::to_value(&report).expect("serializes");
        assert_eq!(json["available"], serde_json::json!(false));
        assert!(json["backends"].is_array());
        assert!(json["error"].is_string());
        assert!(json["library_path"].is_null());
    }
}
