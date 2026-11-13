//! Tauri commands for the audio.cpp engine module (FFI-layer stage).

use std::path::PathBuf;

use serde::Serialize;
use tauri::command;

use super::error::AudioCppError;
use super::wrapper::AudioCpp;

/// Diagnostic report on the audio.cpp native library: whether it loaded,
/// its ABI/build version, and the model families and tasks it supports.
#[derive(Debug, Clone, Serialize)]
pub struct AudioCppDiagnostics {
    pub available: bool,
    pub library_path: Option<PathBuf>,
    pub abi_version: Option<String>,
    pub build_version: Option<String>,
    pub families: Vec<String>,
    pub tasks: Vec<String>,
    pub error: Option<String>,
}

impl AudioCppDiagnostics {
    /// Report for a library that could not be loaded or interrogated; the
    /// error text explains why in user-facing terms.
    pub fn unavailable(error: &AudioCppError) -> Self {
        AudioCppDiagnostics {
            available: false,
            library_path: None,
            abi_version: None,
            build_version: None,
            families: Vec::new(),
            tasks: Vec::new(),
            error: Some(error.to_string()),
        }
    }

    fn from_engine(engine: &AudioCpp) -> Self {
        match engine.create_registry() {
            Ok(registry) => AudioCppDiagnostics {
                available: true,
                library_path: Some(engine.path().to_path_buf()),
                abi_version: Some(engine.abi_version().to_string()),
                build_version: Some(engine.build_version()),
                families: registry.families(),
                tasks: engine.supported_tasks(),
                error: None,
            },
            Err(error) => {
                // The library itself loaded and passed the ABI gate; keep
                // that visible even though the registry query failed.
                let mut report = AudioCppDiagnostics::unavailable(&error);
                report.library_path = Some(engine.path().to_path_buf());
                report.abi_version = Some(engine.abi_version().to_string());
                report.build_version = Some(engine.build_version());
                report
            }
        }
    }
}

/// Diagnostic command: loads the audio.cpp library (if present) and
/// reports its ABI version and supported model families. A missing or
/// ABI-incompatible library is reported in-band (`available: false` with
/// an explanatory `error`), never as a command failure or a crash.
#[command]
pub async fn audiocpp_diagnostics() -> Result<AudioCppDiagnostics, String> {
    // Library loading and registry queries block; keep them off the
    // async runtime (docs/research/audio-cpp-c-api.md).
    tokio::task::spawn_blocking(collect_diagnostics)
        .await
        .map_err(|e| format!("audio.cpp diagnostics task failed: {e}"))
}

fn collect_diagnostics() -> AudioCppDiagnostics {
    match AudioCpp::load() {
        Ok(engine) => AudioCppDiagnostics::from_engine(&engine),
        Err(error) => AudioCppDiagnostics::unavailable(&error),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unavailable_report_carries_error_text() {
        let error = AudioCppError::LibraryNotFound {
            searched_paths: vec![PathBuf::from("/opt/meetily/libaudiocpp.so")],
        };
        let report = AudioCppDiagnostics::unavailable(&error);
        assert!(!report.available);
        assert!(report.library_path.is_none());
        assert!(report.abi_version.is_none());
        assert!(report.families.is_empty());
        assert!(report.tasks.is_empty());
        let text = report.error.expect("error text");
        assert!(text.contains("not found"), "text: {text}");
    }
}
