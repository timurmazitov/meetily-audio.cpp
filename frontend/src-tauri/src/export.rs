//! Export: native save dialog plus file write.
//!
//! The command was born for transcript export but is generic now: the caller
//! chooses the file extension and the save-dialog filter caption. Both stay
//! optional so the original transcript call keeps working unchanged.

use std::path::{Path, PathBuf};

use log::info;
use tauri::{AppHandle, Runtime};
use tauri_plugin_dialog::DialogExt;

/// Extension used when the caller does not ask for one.
const DEFAULT_EXTENSION: &str = "txt";

/// Save-dialog filter caption used when the caller does not ask for one.
const DEFAULT_FILTER_LABEL: &str = "Text Files";

/// Extension and filter caption actually used by the dialog.
#[derive(Debug, Clone, PartialEq, Eq)]
struct ExportParams {
    /// Extension without a leading dot, never empty.
    extension: String,
    /// Caption of the save-dialog filter.
    filter_label: String,
}

/// Normalize the optional command parameters into what the dialog needs.
///
/// A missing, blank or dot-only extension falls back to `txt`, a missing or
/// blank caption falls back to `Text Files`, so an older caller that passes
/// neither keeps the transcript behaviour.
fn resolve_export_params(extension: Option<&str>, filter_label: Option<&str>) -> ExportParams {
    let extension = extension
        .unwrap_or_default()
        .trim()
        .trim_start_matches('.')
        .trim();
    let extension = if extension.is_empty() {
        DEFAULT_EXTENSION
    } else {
        extension
    };

    let filter_label = filter_label.unwrap_or_default().trim();
    let filter_label = if filter_label.is_empty() {
        DEFAULT_FILTER_LABEL
    } else {
        filter_label
    };

    ExportParams {
        extension: extension.to_string(),
        filter_label: filter_label.to_string(),
    }
}

/// Append `extension` to a path the user saved without one.
///
/// A path that already carries an extension is returned untouched, so an
/// extension the user typed (for example `.markdown`) is never rewritten.
fn with_extension_if_missing(path: &Path, extension: &str) -> PathBuf {
    let has_extension = path
        .extension()
        .is_some_and(|existing| !existing.is_empty());
    if extension.is_empty() || has_extension {
        return path.to_path_buf();
    }

    path.with_extension(extension)
}

/// Open a native save dialog and write the given text to the selected file as
/// UTF-8 (no BOM). Returns the chosen file path, or None when the user cancels
/// the dialog.
///
/// `extension` defaults to `txt` and `filter_label` to `Text Files`; the
/// summary export passes `md` / `Markdown Files` instead.
#[tauri::command]
pub async fn export_transcript_file_command<R: Runtime>(
    app: AppHandle<R>,
    content: String,
    default_file_name: String,
    extension: Option<String>,
    filter_label: Option<String>,
) -> Result<Option<String>, String> {
    let params = resolve_export_params(extension.as_deref(), filter_label.as_deref());
    info!(
        "Opening save dialog for export ({} / {})",
        params.extension, params.filter_label
    );

    let app_clone = app.clone();
    let (extension, filter_label) = (params.extension.clone(), params.filter_label.clone());
    let file_path = tokio::task::spawn_blocking(move || {
        app_clone
            .dialog()
            .file()
            .set_file_name(&default_file_name)
            .add_filter(&filter_label, &[extension.as_str()])
            .blocking_save_file()
    })
    .await
    .map_err(|e| format!("Save dialog task failed: {}", e))?;

    match file_path {
        Some(path) => {
            let path = path
                .into_path()
                .map_err(|e| format!("Invalid export path: {}", e))?;
            let path_str = with_extension_if_missing(&path, &params.extension)
                .to_string_lossy()
                .to_string();
            std::fs::write(&path_str, content.as_bytes())
                .map_err(|e| format!("Failed to write export file: {}", e))?;
            info!("Exported to {}", path_str);
            Ok(Some(path_str))
        }
        None => {
            info!("User cancelled export");
            Ok(None)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{resolve_export_params, with_extension_if_missing, ExportParams};
    use std::path::{Path, PathBuf};

    fn params(extension: Option<&str>, filter_label: Option<&str>) -> ExportParams {
        resolve_export_params(extension, filter_label)
    }

    #[test]
    fn defaults_apply_when_the_parameters_are_absent() {
        assert_eq!(
            params(None, None),
            ExportParams {
                extension: "txt".to_string(),
                filter_label: "Text Files".to_string(),
            }
        );
    }

    #[test]
    fn leading_dots_are_trimmed_from_the_extension() {
        assert_eq!(params(Some(".md"), None).extension, "md");
        assert_eq!(params(Some("...md"), None).extension, "md");
        assert_eq!(params(Some("  .markdown  "), None).extension, "markdown");
    }

    #[test]
    fn an_empty_extension_falls_back_to_the_default() {
        for extension in [Some(""), Some("   "), Some("."), Some("...")] {
            assert_eq!(params(extension, None).extension, "txt");
        }
    }

    #[test]
    fn an_empty_filter_label_falls_back_to_the_default() {
        assert_eq!(params(Some("md"), Some("")).filter_label, "Text Files");
        assert_eq!(params(Some("md"), Some("  ")).filter_label, "Text Files");
        assert_eq!(
            params(Some("md"), Some(" Markdown Files ")).filter_label,
            "Markdown Files"
        );
    }

    #[test]
    fn the_requested_extension_and_caption_are_kept() {
        assert_eq!(
            params(Some("md"), Some("Markdown Files")),
            ExportParams {
                extension: "md".to_string(),
                filter_label: "Markdown Files".to_string(),
            }
        );
    }

    #[test]
    fn the_extension_is_appended_to_a_path_without_one() {
        assert_eq!(
            with_extension_if_missing(Path::new("/tmp/exports/20261118-plan-summary"), "md"),
            PathBuf::from("/tmp/exports/20261118-plan-summary.md")
        );
        assert_eq!(
            with_extension_if_missing(Path::new("20261118-plan-transcript"), "txt"),
            PathBuf::from("20261118-plan-transcript.txt")
        );
    }

    #[test]
    fn the_extension_is_replaced_when_the_dialog_left_the_dot_behind() {
        assert_eq!(
            with_extension_if_missing(Path::new("/tmp/exports/plan."), "md"),
            PathBuf::from("/tmp/exports/plan.md")
        );
    }

    #[test]
    fn an_extension_typed_by_the_user_is_kept() {
        assert_eq!(
            with_extension_if_missing(Path::new("/tmp/exports/plan.markdown"), "md"),
            PathBuf::from("/tmp/exports/plan.markdown")
        );
        assert_eq!(
            with_extension_if_missing(Path::new("/tmp/exports/plan.txt"), "md"),
            PathBuf::from("/tmp/exports/plan.txt")
        );
    }

    #[test]
    fn a_dot_in_a_directory_name_does_not_count_as_an_extension() {
        assert_eq!(
            with_extension_if_missing(Path::new("/tmp/v1.2/exports/plan-summary"), "md"),
            PathBuf::from("/tmp/v1.2/exports/plan-summary.md")
        );
    }
}
