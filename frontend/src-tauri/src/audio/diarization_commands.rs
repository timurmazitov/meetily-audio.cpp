// audio/diarization_commands.rs
//
// The manual entry point for diarizing a saved meeting: recordings made before
// the diarization model was downloaded, meetings imported without a model, and
// any meeting the user explicitly asks to label.
//
// The name keeps the audio.cpp prefix on purpose: it names the executor (the
// audio.cpp engine), not the user's transcription provider. See ADR 0004.
//
// Everything else lives in `transcription::diarization_run`: the run decision,
// the global pass lock, the label write with the speaker-name recount, and the
// native events. This module only adapts the operation to the Tauri command
// shape.

use log::info;
use serde::Serialize;
use tauri::{command, AppHandle, Runtime};

use crate::audio::transcription::diarization_run::{diarize_saved_meeting, SavedMeetingDiarization};
use crate::audio::transcription::diarization_service::DiarizationStatus;
use crate::state::AppState;

/// Result of diarizing a saved meeting.
#[derive(Debug, Clone, Serialize)]
pub struct DiarizationResult {
    pub meeting_id: String,
    /// Why the pass did or did not run. The UI switches on this instead of
    /// parsing `error`.
    pub status: DiarizationStatus,
    /// Number of transcript segments that received a speaker label.
    pub labeled_segments: usize,
    pub total_segments: usize,
    /// Historical availability flag, always derived from `status == ran`.
    pub speakers_available: bool,
    /// Diagnostic detail when `status` is `failed`.
    pub error: Option<String>,
}

impl From<SavedMeetingDiarization> for DiarizationResult {
    fn from(result: SavedMeetingDiarization) -> Self {
        DiarizationResult {
            speakers_available: result.speakers_available(),
            meeting_id: result.meeting_id,
            status: result.status,
            labeled_segments: result.labeled_segments,
            total_segments: result.total_segments,
            error: result.error,
        }
    }
}

/// Diarize a saved meeting's recording and persist speaker labels.
///
/// The command is idempotent: re-running it replaces the previous labels. An
/// absent model, an unavailable native runtime or an active recording are
/// reported in `status`, not as a command failure — the transcript is
/// untouched. Only a missing meeting or a database failure is `Err`.
#[command]
pub async fn audiocpp_diarize_meeting<R: Runtime>(
    app: AppHandle<R>,
    state: tauri::State<'_, AppState>,
    meeting_id: String,
) -> Result<DiarizationResult, String> {
    let pool = state.db_manager.pool();
    let result = diarize_saved_meeting(&app, pool, &meeting_id).await?;
    info!(
        "Diarization of meeting {} finished with status {:?}",
        meeting_id, result.status
    );
    Ok(result.into())
}
