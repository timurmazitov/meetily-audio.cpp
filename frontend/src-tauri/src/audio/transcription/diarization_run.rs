// audio/transcription/diarization_run.rs
//
// One operation: "diarize a saved meeting". Every caller that produces a
// meeting — the manual command, import, retranscription — goes through it, so
// the run decision, the label write, the speaker-name recount and the events
// exist once.
//
// The pass itself lives in `diarization_service` (pure overlay + the audio.cpp
// run). This module is the orchestration around it and owns the rules that
// belong to a *saved* meeting: the process-wide serialization of passes, the
// recording guard, and the native `transcript-updated` event.

use std::path::PathBuf;
use std::sync::Arc;

use log::{info, warn};
use once_cell::sync::Lazy;
use sqlx::{Connection, SqlitePool};
use tauri::{AppHandle, Emitter, Runtime};
use tokio::sync::{Mutex as AsyncMutex, OwnedMutexGuard};

use super::diarization_service::{
    decide_diarization, transcript_updated_payload, DiarizationDecision, DiarizationStatus,
    DIARIZATION_FAILED_MESSAGE, DIARIZATION_RUNTIME_UNAVAILABLE_MESSAGE,
    DIARIZATION_UNAVAILABLE_MESSAGE, SPEAKER_NAMES_RESET_EVENT, SPEAKER_NAMES_RESET_MESSAGE,
    TRANSCRIPT_UPDATED_EVENT,
};
use crate::database::repositories::meeting::MeetingsRepository;
use crate::database::repositories::speaker;

/// Event emitted when a saved meeting could not be diarized.
pub const DIARIZATION_UNAVAILABLE_EVENT: &str = "diarization-unavailable";

/// Serializes diarization passes process-wide: at most one pass runs at a
/// time, because a second copy of the model in memory is exactly what the
/// spec wants to avoid. Deliberately separate from the transcription-engine
/// lifecycle lock (`audio::common`): a model unload after a batch job must not
/// block or cancel a running pass.
static DIARIZATION_PASS_LOCK: Lazy<Arc<AsyncMutex<()>>> =
    Lazy::new(|| Arc::new(AsyncMutex::new(())));

async fn acquire_pass_lock() -> OwnedMutexGuard<()> {
    DIARIZATION_PASS_LOCK.clone().lock_owned().await
}

/// Result of diarizing one saved meeting.
#[derive(Debug, Clone, PartialEq)]
pub struct SavedMeetingDiarization {
    pub meeting_id: String,
    /// Why the pass did or did not run.
    pub status: DiarizationStatus,
    /// Transcript segments that received a speaker label.
    pub labeled_segments: usize,
    pub total_segments: usize,
    /// Diagnostic detail for `Failed`; never user-facing prose.
    pub error: Option<String>,
}

impl SavedMeetingDiarization {
    /// The historical availability flag, derived so it cannot disagree with
    /// the status.
    pub fn speakers_available(&self) -> bool {
        self.status.speakers_available()
    }
}

/// One notification a finished pass owes the UI.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DiarizationEvent {
    /// Speaker labels reached the database; the open meeting should refetch.
    TranscriptUpdated {
        meeting_id: String,
        labeled_segments: usize,
        total_segments: usize,
    },
    /// The pass was attempted and failed. Configuration reasons
    /// (`no_model`, `no_runtime`, `deferred`) are deliberately absent: the
    /// user did not ask for those passes.
    Unavailable {
        meeting_id: String,
        status: DiarizationStatus,
        error: Option<String>,
    },
    /// The recount removed every name the meeting had.
    SpeakerNamesReset { meeting_id: String },
}

/// Message shown for an `diarization-unavailable` announcement.
///
/// Native code is the source of truth for these texts; the frontend mirrors
/// them. Missing model and missing runtime read differently on purpose so the
/// user knows what to fix.
pub fn unavailable_message(status: DiarizationStatus) -> &'static str {
    match status {
        DiarizationStatus::NoRuntime => DIARIZATION_RUNTIME_UNAVAILABLE_MESSAGE,
        DiarizationStatus::NoModel => DIARIZATION_UNAVAILABLE_MESSAGE,
        DiarizationStatus::Failed | DiarizationStatus::Ran | DiarizationStatus::Deferred => {
            DIARIZATION_FAILED_MESSAGE
        }
    }
}

/// The notifications a result owes, in order, without an `AppHandle`.
///
/// Pure so the announcement rules are unit-tested: `ran` (even with zero
/// labels) refreshes the transcript, `failed` explains itself, and the three
/// configuration statuses stay silent.
pub fn diarization_events(
    result: &SavedMeetingDiarization,
    names_reset: bool,
) -> Vec<DiarizationEvent> {
    let mut events = Vec::new();
    match result.status {
        DiarizationStatus::Ran => {
            events.push(DiarizationEvent::TranscriptUpdated {
                meeting_id: result.meeting_id.clone(),
                labeled_segments: result.labeled_segments,
                total_segments: result.total_segments,
            });
            if names_reset {
                events.push(DiarizationEvent::SpeakerNamesReset {
                    meeting_id: result.meeting_id.clone(),
                });
            }
        }
        DiarizationStatus::Failed => events.push(DiarizationEvent::Unavailable {
            meeting_id: result.meeting_id.clone(),
            status: result.status,
            error: result.error.clone(),
        }),
        DiarizationStatus::NoModel
        | DiarizationStatus::NoRuntime
        | DiarizationStatus::Deferred => {}
    }
    events
}

/// Emits a planned event list. Failures only log: the meeting is already
/// saved and the labels are already in the database.
fn emit_events<R: Runtime>(app: &AppHandle<R>, events: &[DiarizationEvent]) {
    for event in events {
        match event {
            DiarizationEvent::TranscriptUpdated {
                meeting_id,
                labeled_segments,
                total_segments,
            } => {
                if let Err(error) = app.emit(
                    TRANSCRIPT_UPDATED_EVENT,
                    transcript_updated_payload(meeting_id, *labeled_segments, *total_segments),
                ) {
                    warn!("Failed to emit transcript update for {meeting_id}: {error}");
                }
            }
            DiarizationEvent::Unavailable {
                meeting_id,
                status,
                error,
            } => {
                if let Err(emit_error) = app.emit(
                    DIARIZATION_UNAVAILABLE_EVENT,
                    serde_json::json!({
                        "meeting_id": meeting_id,
                        "status": status,
                        "reason": error,
                        "message": unavailable_message(*status),
                    }),
                ) {
                    warn!("Failed to emit diarization-unavailable for {meeting_id}: {emit_error}");
                }
            }
            DiarizationEvent::SpeakerNamesReset { meeting_id } => {
                if let Err(error) = app.emit(
                    SPEAKER_NAMES_RESET_EVENT,
                    serde_json::json!({
                        "meeting_id": meeting_id,
                        "message": SPEAKER_NAMES_RESET_MESSAGE,
                    }),
                ) {
                    warn!("Failed to emit speaker-names-reset for {meeting_id}: {error}");
                }
            }
        }
    }
}

/// Replaces a meeting's speaker labels and recounts its names in one
/// transaction, so a name can never outlive the label it is bound to.
///
/// Returns whether the recount dropped at least one existing name.
async fn persist_speaker_labels(
    pool: &SqlitePool,
    meeting_id: &str,
    segments: &[crate::api::TranscriptSegment],
) -> Result<bool, String> {
    let mut conn = pool
        .acquire()
        .await
        .map_err(|error| format!("Database error: {error}"))?;
    let mut tx = conn
        .begin()
        .await
        .map_err(|error| format!("Failed to start transaction: {error}"))?;
    for segment in segments {
        sqlx::query("UPDATE transcripts SET speaker = ? WHERE id = ?")
            .bind(&segment.speaker)
            .bind(&segment.id)
            .execute(&mut *tx)
            .await
            .map_err(|error| format!("Failed to update speaker label: {error}"))?;
    }
    let reconcile = speaker::reconcile_meeting_speaker_assignments(&mut tx, meeting_id)
        .await
        .map_err(|error| format!("Failed to reconcile speaker names: {error}"))?;
    tx.commit()
        .await
        .map_err(|error| format!("Failed to commit speaker labels: {error}"))?;
    Ok(reconcile.names_reset)
}

/// Diarizes a saved meeting: decide, run one pass over the meeting's audio
/// file, persist the labels with the speaker-name recount, and emit the
/// native events.
///
/// The pass decodes the meeting file itself and releases the native library
/// and model before returning. A configuration reason (`no_model`,
/// `no_runtime`, `deferred`) is a normal result, not an error; only database
/// failures and a missing meeting are `Err`.
pub async fn diarize_saved_meeting<R: Runtime>(
    app: &AppHandle<R>,
    pool: &SqlitePool,
    meeting_id: &str,
) -> Result<SavedMeetingDiarization, String> {
    // One pass at a time. The guard is held across the recording check and
    // the model load so the decision and the pass cannot interleave with a
    // second caller.
    let _pass = acquire_pass_lock().await;

    let models = match crate::audio_cpp_engine::commands::model_manager() {
        Some(manager) => manager.discover_models().await.unwrap_or_default(),
        // No model manager means no catalogued model at all, exactly like a
        // Whisper/Parakeet session in unit tests.
        None => Vec::new(),
    };
    // Only pay for the runtime probe when a model exists: "no model" outranks
    // "no runtime", and probing loads the library.
    let has_model = crate::audio::transcription::diarization_service::select_diarization_model(
        &models,
    )
    .is_some();
    let runtime_available = if has_model {
        crate::audio_cpp_engine::commands::detect_runtime_available().await
    } else {
        false
    };
    // Checked before the model is loaded: a pass must not compete for memory
    // with an active recording.
    let recording_active = crate::audio::recording_commands::is_recording().await;

    let decision = decide_diarization(&models, runtime_available, recording_active);
    let model = match decision {
        DiarizationDecision::Run { model } => model,
        DiarizationDecision::DoNotRun(status) => {
            info!(
                "Diarization of meeting {} skipped: {:?}",
                meeting_id, status
            );
            let result = SavedMeetingDiarization {
                meeting_id: meeting_id.to_string(),
                status,
                labeled_segments: 0,
                total_segments: 0,
                error: None,
            };
            emit_events(app, &diarization_events(&result, false));
            return Ok(result);
        }
    };

    let meeting = MeetingsRepository::get_meeting(pool, meeting_id)
        .await
        .map_err(|error| format!("Failed to load meeting: {error}"))?
        .ok_or_else(|| format!("Meeting not found: {meeting_id}"))?;

    let folder = match meeting.folder_path.as_deref().map(PathBuf::from) {
        Some(folder) => folder,
        None => {
            let result = SavedMeetingDiarization {
                meeting_id: meeting_id.to_string(),
                status: DiarizationStatus::Failed,
                labeled_segments: 0,
                total_segments: meeting.transcripts.len(),
                error: Some("meeting has no recording folder".to_string()),
            };
            emit_events(app, &diarization_events(&result, false));
            return Ok(result);
        }
    };

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

    let outcome = super::diarization_service::diarize_recording_folder_with_model(
        &folder,
        &mut segments,
        &model,
    )
    .await;

    let names_reset;
    if outcome.status == DiarizationStatus::Ran {
        let labeled = segments
            .iter()
            .filter(|segment| segment.speaker.is_some())
            .count();
        match persist_speaker_labels(pool, meeting_id, &segments).await {
            Ok(reset) => names_reset = reset,
            Err(error) => {
                // The pass succeeded but the write did not; report it as a
                // failure without losing the transcript.
                warn!("Failed to persist diarization labels: {error}");
                let result = SavedMeetingDiarization {
                    meeting_id: meeting_id.to_string(),
                    status: DiarizationStatus::Failed,
                    labeled_segments: 0,
                    total_segments,
                    error: Some(error),
                };
                emit_events(app, &diarization_events(&result, false));
                return Ok(result);
            }
        }
        info!(
            "Diarized meeting {}: {} of {} transcript segments labeled",
            meeting_id, labeled, total_segments
        );
        let result = SavedMeetingDiarization {
            meeting_id: meeting_id.to_string(),
            status: DiarizationStatus::Ran,
            labeled_segments: labeled,
            total_segments,
            error: None,
        };
        emit_events(app, &diarization_events(&result, names_reset));
        Ok(result)
    } else {
        let result = SavedMeetingDiarization {
            meeting_id: meeting_id.to_string(),
            status: outcome.status,
            labeled_segments: 0,
            total_segments,
            error: outcome.error,
        };
        emit_events(app, &diarization_events(&result, false));
        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio::transcription::diarization_service::DiarizationStatus;

    fn result(status: DiarizationStatus, labeled: usize, total: usize) -> SavedMeetingDiarization {
        SavedMeetingDiarization {
            meeting_id: "meeting-1".to_string(),
            status,
            labeled_segments: labeled,
            total_segments: total,
            error: None,
        }
    }

    #[test]
    fn a_ran_pass_with_no_labeled_segments_announces_the_transcript_not_a_failure() {
        let events = diarization_events(&result(DiarizationStatus::Ran, 0, 4), false);

        assert_eq!(
            events,
            vec![DiarizationEvent::TranscriptUpdated {
                meeting_id: "meeting-1".to_string(),
                labeled_segments: 0,
                total_segments: 4,
            }]
        );
    }

    #[test]
    fn a_ran_pass_with_labels_announces_the_transcript() {
        let events = diarization_events(&result(DiarizationStatus::Ran, 3, 4), false);

        assert_eq!(
            events,
            vec![DiarizationEvent::TranscriptUpdated {
                meeting_id: "meeting-1".to_string(),
                labeled_segments: 3,
                total_segments: 4,
            }]
        );
    }

    #[test]
    fn a_failed_pass_announces_unavailability_with_its_reason() {
        let mut failed = result(DiarizationStatus::Failed, 0, 4);
        failed.error = Some("audio is too short".to_string());

        let events = diarization_events(&failed, false);

        assert_eq!(
            events,
            vec![DiarizationEvent::Unavailable {
                meeting_id: "meeting-1".to_string(),
                status: DiarizationStatus::Failed,
                error: Some("audio is too short".to_string()),
            }]
        );
    }

    #[test]
    fn configuration_reasons_stay_silent_because_the_user_did_not_ask() {
        for status in [
            DiarizationStatus::NoModel,
            DiarizationStatus::NoRuntime,
            DiarizationStatus::Deferred,
        ] {
            assert_eq!(
                diarization_events(&result(status, 0, 4), false),
                Vec::new(),
                "status: {status:?}"
            );
        }
    }

    #[test]
    fn a_recount_that_dropped_every_name_is_announced_after_the_refresh() {
        let events = diarization_events(&result(DiarizationStatus::Ran, 2, 4), true);

        assert_eq!(
            events,
            vec![
                DiarizationEvent::TranscriptUpdated {
                    meeting_id: "meeting-1".to_string(),
                    labeled_segments: 2,
                    total_segments: 4,
                },
                DiarizationEvent::SpeakerNamesReset {
                    meeting_id: "meeting-1".to_string(),
                },
            ]
        );
    }

    #[test]
    fn missing_model_and_missing_runtime_explain_themselves_differently() {
        assert_eq!(
            unavailable_message(DiarizationStatus::NoModel),
            DIARIZATION_UNAVAILABLE_MESSAGE
        );
        assert_eq!(
            unavailable_message(DiarizationStatus::NoRuntime),
            DIARIZATION_RUNTIME_UNAVAILABLE_MESSAGE
        );
        assert_eq!(
            unavailable_message(DiarizationStatus::Failed),
            DIARIZATION_FAILED_MESSAGE
        );
        assert_ne!(
            unavailable_message(DiarizationStatus::NoModel),
            unavailable_message(DiarizationStatus::NoRuntime)
        );
    }

    async fn migrated_pool() -> SqlitePool {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .expect("in-memory sqlite");
        sqlx::migrate!("./migrations")
            .run(&pool)
            .await
            .expect("migrations apply");
        pool
    }

    async fn seed_meeting(pool: &SqlitePool, meeting_id: &str) {
        sqlx::query("INSERT INTO meetings (id, title, created_at, updated_at) VALUES (?, ?, ?, ?)")
            .bind(meeting_id)
            .bind("Meeting")
            .bind("2024-01-01T00:00:00Z")
            .bind("2024-01-01T00:00:00Z")
            .execute(pool)
            .await
            .unwrap();
        for (id, speaker, start) in [
            ("t-0", Some("speaker_0"), 0.0_f64),
            ("t-1", Some("speaker_1"), 1.0),
        ] {
            sqlx::query(
                "INSERT INTO transcripts (id, meeting_id, transcript, timestamp, speaker, audio_start_time)
                 VALUES (?, ?, ?, ?, ?, ?)",
            )
            .bind(id)
            .bind(meeting_id)
            .bind(format!("Text from {id}"))
            .bind("2024-01-01T00:00:00Z")
            .bind(speaker)
            .bind(start)
            .execute(pool)
            .await
            .unwrap();
        }
        sqlx::query(
            "INSERT INTO speakers (id, name, created_at, updated_at) VALUES (?, ?, ?, ?)",
        )
        .bind("person-anna")
        .bind("Anna")
        .bind("2024-01-01T00:00:00Z")
        .bind("2024-01-01T00:00:00Z")
        .execute(pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO meeting_speakers (id, meeting_id, label, speaker_id, created_at, updated_at)
             VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind("binding-1")
        .bind(meeting_id)
        .bind("speaker_0")
        .bind("person-anna")
        .bind("2024-01-01T00:00:00Z")
        .bind("2024-01-01T00:00:00Z")
        .execute(pool)
        .await
        .unwrap();
    }

    fn segment(id: &str, speaker: Option<&str>) -> crate::api::TranscriptSegment {
        crate::api::TranscriptSegment {
            id: id.to_string(),
            text: "hello".to_string(),
            timestamp: "2024-01-01T00:00:00Z".to_string(),
            audio_start_time: Some(0.0),
            audio_end_time: Some(1.0),
            duration: Some(1.0),
            speaker: speaker.map(str::to_string),
        }
    }

    #[tokio::test]
    async fn labels_are_written_wholesale_and_the_name_recount_joins_the_same_transaction() {
        let pool = migrated_pool().await;
        seed_meeting(&pool, "meeting-1").await;

        // The pass renumbered the voices: `speaker_0` is gone, `speaker_2` is
        // new. The name bound to the vanished label must not survive.
        let names_reset = persist_speaker_labels(
            &pool,
            "meeting-1",
            &[segment("t-0", Some("speaker_2")), segment("t-1", None)],
        )
        .await
        .expect("labels persist");

        assert!(names_reset, "the old binding should be reported as reset");
        let labels: Vec<(Option<String>,)> =
            sqlx::query_as("SELECT speaker FROM transcripts WHERE meeting_id = ? ORDER BY id")
                .bind("meeting-1")
                .fetch_all(&pool)
                .await
                .unwrap();
        assert_eq!(
            labels,
            vec![(Some("speaker_2".to_string()),), (None,)],
            "labels are replaced, not merged"
        );
        let bindings: Vec<(String,)> =
            sqlx::query_as("SELECT label FROM meeting_speakers WHERE meeting_id = ?")
                .bind("meeting-1")
                .fetch_all(&pool)
                .await
                .unwrap();
        assert!(bindings.is_empty(), "binding: {bindings:?}");
    }

    #[tokio::test]
    async fn a_kept_label_keeps_its_name_and_is_not_reported_as_a_reset() {
        let pool = migrated_pool().await;
        seed_meeting(&pool, "meeting-1").await;

        let names_reset = persist_speaker_labels(
            &pool,
            "meeting-1",
            &[segment("t-0", Some("speaker_0")), segment("t-1", Some("speaker_1"))],
        )
        .await
        .expect("labels persist");

        assert!(!names_reset);
        let bindings: Vec<(String,)> =
            sqlx::query_as("SELECT label FROM meeting_speakers WHERE meeting_id = ?")
                .bind("meeting-1")
                .fetch_all(&pool)
                .await
                .unwrap();
        assert_eq!(bindings, vec![("speaker_0".to_string(),)]);
    }
}
