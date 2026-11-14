// audio/diarization_commands.rs
//
// Post-transcription diarization for recordings that were saved before the
// diarization model was downloaded, and the explicit entry point the stop
// flow does not need to call in the common case.
//
// The overlay logic lives in `transcription::diarization_service`; this module
// only exposes the Tauri command and writes the labels back to the database.

use std::path::PathBuf;

use log::{info, warn};
use serde::Serialize;
use tauri::{command, AppHandle, Emitter, Runtime};

use crate::audio::transcription::diarization_service::{
    diarize_recording_folder, DIARIZATION_UNAVAILABLE_MESSAGE,
};
use crate::state::AppState;

/// Result of diarizing a saved meeting.
#[derive(Debug, Clone, Serialize)]
pub struct DiarizationResult {
    pub meeting_id: String,
    /// Number of transcript segments that received a speaker label.
    pub labeled_segments: usize,
    pub total_segments: usize,
    /// False when no diarization model is installed or the pass failed.
    pub speakers_available: bool,
    pub error: Option<String>,
}

/// Diarize a saved meeting's recording and persist speaker labels.
///
/// The command is idempotent: re-running it replaces the previous labels.
/// A missing Nemotron model or a failed pass returns `speakers_available:
/// false` with the transcript untouched.
#[command]
pub async fn audiocpp_diarize_meeting<R: Runtime>(
    app: AppHandle<R>,
    state: tauri::State<'_, AppState>,
    meeting_id: String,
) -> Result<DiarizationResult, String> {
    let pool = state.db_manager.pool();

    let meeting =
        crate::database::repositories::meeting::MeetingsRepository::get_meeting(pool, &meeting_id)
            .await
            .map_err(|error| format!("Failed to load meeting: {error}"))?
            .ok_or_else(|| format!("Meeting not found: {meeting_id}"))?;

    let folder = meeting
        .folder_path
        .as_deref()
        .map(PathBuf::from)
        .ok_or_else(|| "Meeting has no recording folder; nothing to diarize".to_string())?;

    // Build the segment list in the same shape the repository stores.
    let mut segments: Vec<crate::api::TranscriptSegment> = meeting
        .transcripts
        .iter()
        .map(|segment| crate::api::TranscriptSegment {
            id: segment.id.clone(),
            text: segment.text.clone(),
            timestamp: segment.timestamp.clone(),
            audio_start_time: segment.audio_start_time,
            audio_end_time: segment.audio_end_time,
            duration: segment.duration,
            speaker: segment.speaker.clone(),
        })
        .collect();

    let total_segments = segments.len();
    let outcome = diarize_recording_folder(&folder, &mut segments).await;

    if outcome.speakers_available {
        let mut conn = pool
            .acquire()
            .await
            .map_err(|error| format!("Database error: {error}"))?;
        let mut tx = sqlx::Connection::begin(&mut *conn)
            .await
            .map_err(|error| format!("Failed to start transaction: {error}"))?;
        for segment in &segments {
            sqlx::query("UPDATE transcripts SET speaker = ? WHERE id = ?")
                .bind(&segment.speaker)
                .bind(&segment.id)
                .execute(&mut *tx)
                .await
                .map_err(|error| format!("Failed to update speaker label: {error}"))?;
        }
        tx.commit()
            .await
            .map_err(|error| format!("Failed to commit speaker labels: {error}"))?;
        info!(
            "Diarized meeting {}: {} transcript segments labeled",
            meeting_id, total_segments
        );
    } else {
        warn!(
            "Diarization unavailable for meeting {}: {:?}",
            meeting_id, outcome.error
        );
        let _ = app.emit(
            "diarization-unavailable",
            serde_json::json!({
                "meeting_id": meeting_id,
                "reason": outcome.error,
                "message": DIARIZATION_UNAVAILABLE_MESSAGE,
            }),
        );
    }

    Ok(DiarizationResult {
        meeting_id,
        labeled_segments: segments
            .iter()
            .filter(|segment| segment.speaker.is_some())
            .count(),
        total_segments,
        speakers_available: outcome.speakers_available,
        error: outcome.error,
    })
}
