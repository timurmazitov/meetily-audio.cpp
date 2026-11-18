use crate::api::{TranscriptSearchResult, TranscriptSegment};
use chrono::Utc;
use sqlx::{Connection, Error as SqlxError, SqlitePool};
use log::{error, info};
use uuid::Uuid;

pub struct TranscriptsRepository;

impl TranscriptsRepository {
    /// Saves a new meeting and its associated transcript segments.
    /// This function uses a transaction to ensure that either both the meeting
    /// and all its transcripts are saved, or none of them are.
    pub async fn save_transcript(
        pool: &SqlitePool,
        meeting_title: &str,
        transcripts: &[TranscriptSegment],
        folder_path: Option<String>,
    ) -> Result<String, SqlxError> {
        let meeting_id = format!("meeting-{}", Uuid::new_v4());

        let mut conn = pool.acquire().await?;
        let mut transaction = conn.begin().await?;

        let now = Utc::now();
        let folder_facts = crate::database::meeting_facts::read_folder_meeting_facts(
            folder_path.as_deref(),
        );

        // The meeting's own moment and length: the folder is authoritative
        // because it knows when the recording started. Without a usable folder
        // the row still gets the honest approximations the spec allows — the
        // save moment and the end of the last segment — and otherwise NULL.
        let fallback_duration = transcripts
            .iter()
            .filter_map(|segment| segment.audio_end_time)
            .reduce(f64::max);
        let recorded_at = folder_facts
            .recorded_at_ms
            .map(chrono::DateTime::<Utc>::from_timestamp_millis)
            .flatten()
            .unwrap_or(now);
        let duration_seconds = folder_facts.duration_seconds.or(fallback_duration);

        // 1. Create the new meeting
        let result = sqlx::query(
            "INSERT INTO meetings (id, title, created_at, updated_at, folder_path, recorded_at, duration_seconds)
             VALUES (?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&meeting_id)
        .bind(meeting_title)
        .bind(now)
        .bind(now)
        .bind(&folder_path)
        .bind(recorded_at)
        .bind(duration_seconds)
        .execute(&mut *transaction)
        .await;

        if let Err(e) = result {
            error!("Failed to create meeting with id: {}", meeting_id);
            transaction.rollback().await?;
            return Err(e);
        }

        info!("Successfully created meeting with id: {}", meeting_id);

        // 2. Save each transcript segment with audio timing fields
        for segment in transcripts {
            let transcript_id = format!("transcript-{}", Uuid::new_v4());
            let result = sqlx::query(
                "INSERT INTO transcripts (id, meeting_id, transcript, timestamp, audio_start_time, audio_end_time, duration, speaker)
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?)"
            )
            .bind(&transcript_id)
            .bind(&meeting_id)
            .bind(&segment.text)
            .bind(&segment.timestamp)
            .bind(segment.audio_start_time)
            .bind(segment.audio_end_time)
            .bind(segment.duration)
            .bind(&segment.speaker)
            .execute(&mut *transaction)
            .await;

            if let Err(e) = result {
                error!("Failed to save transcript segment for meeting {}", meeting_id);
                transaction.rollback().await?;
                return Err(e);
            }
        }

        info!(
            "Successfully saved {} transcript segments for meeting {}",
            transcripts.len(),
            meeting_id
        );

        // Commit the transaction
        transaction.commit().await?;

        Ok(meeting_id)
    }

    /// Searches for a query string within the transcripts.
    /// It returns a list of matching transcripts with context.
    pub async fn search_transcripts(
        pool: &SqlitePool,
        query: &str,
    ) -> Result<Vec<TranscriptSearchResult>, SqlxError> {
        if query.trim().is_empty() {
            return Ok(Vec::new());
        }

        let search_query = format!("%{}%", query.to_lowercase());

        let rows = sqlx::query_as::<_, (String, String, String, String)>(
            "SELECT m.id, m.title, t.transcript, t.timestamp
             FROM meetings m
             JOIN transcripts t ON m.id = t.meeting_id
             WHERE LOWER(t.transcript) LIKE ?",
        )
        .bind(&search_query)
        .fetch_all(pool)
        .await?;

        let results = rows
            .into_iter()
            .map(|(id, title, transcript, timestamp)| {
                let match_context = Self::get_match_context(&transcript, query);
                TranscriptSearchResult {
                    id,
                    title,
                    match_context,
                    timestamp,
                }
            })
            .collect();

        Ok(results)
    }

    /// Helper function to extract a snippet of text around the first match of a query.
    fn get_match_context(transcript: &str, query: &str) -> String {
        let transcript_lower = transcript.to_lowercase();
        let query_lower = query.to_lowercase();

        match transcript_lower.find(&query_lower) {
            Some(match_index) => {
                let start_index = match_index.saturating_sub(100);
                let end_index = (match_index + query.len() + 100).min(transcript.len());

                let mut context = String::new();
                if start_index > 0 {
                    context.push_str("...");
                }
                context.push_str(&transcript[start_index..end_index]);
                if end_index < transcript.len() {
                    context.push_str("...");
                }
                context
            }
            None => transcript.chars().take(200).collect(), // Fallback to the start of the transcript
        }
    }
}

#[cfg(test)]
mod tests {
    use super::TranscriptsRepository;
    use crate::api::TranscriptSegment;
    use crate::database::repositories::meeting::MeetingsRepository;
    use sqlx::sqlite::SqlitePoolOptions;
    use sqlx::SqlitePool;

    async fn migrated_pool() -> SqlitePool {
        let pool = SqlitePoolOptions::new()
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

    fn segment(text: &str, end_seconds: f64) -> TranscriptSegment {
        TranscriptSegment {
            id: format!("transcript-{text}"),
            text: text.to_string(),
            timestamp: "2024-01-01T00:00:00Z".to_string(),
            audio_start_time: Some(end_seconds - 1.0),
            audio_end_time: Some(end_seconds),
            duration: Some(1.0),
            speaker: None,
        }
    }

    /// A recording folder as `recording_saver` leaves it after a stop: the
    /// start moment and the finished length live in `metadata.json`.
    fn folder_with_metadata(metadata: serde_json::Value) -> tempfile::TempDir {
        let dir = tempfile::tempdir().expect("tempdir");
        std::fs::write(
            dir.path().join("metadata.json"),
            serde_json::to_string(&metadata).unwrap(),
        )
        .unwrap();
        dir
    }

    /// The meeting as the list command reads it, so the assertions see exactly
    /// what the sidebar will receive.
    async fn listed_meeting(pool: &SqlitePool, meeting_id: &str) -> crate::database::models::MeetingModel {
        MeetingsRepository::get_meetings(pool)
            .await
            .unwrap()
            .into_iter()
            .find(|meeting| meeting.id == meeting_id)
            .expect("the saved meeting is listed")
    }

    async fn meeting_row(pool: &SqlitePool, meeting_id: &str) -> (String, Option<String>, Option<f64>) {
        sqlx::query_as("SELECT created_at, recorded_at, duration_seconds FROM meetings WHERE id = ?")
            .bind(meeting_id)
            .fetch_one(pool)
            .await
            .unwrap()
    }

    #[tokio::test]
    async fn a_recording_without_usable_metadata_still_gets_an_honest_moment_and_length() {
        let pool = migrated_pool().await;
        // The folder exists but the file is absent: the saver had no chance to
        // write it, so the save point is the best moment there is.
        let folder = tempfile::tempdir().unwrap();
        let before = chrono::Utc::now();

        let meeting_id = TranscriptsRepository::save_transcript(
            &pool,
            "Recovered call",
            &[segment("first", 10.0), segment("last", 90.0)],
            Some(folder.path().to_string_lossy().to_string()),
        )
        .await
        .unwrap();

        let meeting = listed_meeting(&pool, &meeting_id).await;
        let recorded_at = meeting
            .recorded_at
            .as_ref()
            .map(|moment| moment.0)
            .expect("the save moment stands in for a missing file");
        assert!(
            recorded_at >= before && recorded_at <= chrono::Utc::now(),
            "the stand-in moment is the save moment, not an arbitrary date"
        );
        assert_eq!(
            meeting.duration_seconds,
            Some(90.0),
            "without a file the length falls back to the end of the last segment"
        );
    }

    #[tokio::test]
    async fn an_unparsable_length_in_the_file_falls_back_to_the_last_segment() {
        let pool = migrated_pool().await;
        let folder = folder_with_metadata(serde_json::json!({
            "created_at": "2026-11-18T14:32:05+00:00",
            "duration_seconds": "about an hour",
        }));

        let meeting_id = TranscriptsRepository::save_transcript(
            &pool,
            "Half-broken call",
            &[segment("only", 600.0)],
            Some(folder.path().to_string_lossy().to_string()),
        )
        .await
        .unwrap();

        let meeting = listed_meeting(&pool, &meeting_id).await;
        assert_eq!(
            meeting
                .recorded_at
                .as_ref()
                .map(|moment| moment.0.to_rfc3339()),
            Some("2026-11-18T14:32:05+00:00".to_string()),
            "the start moment in the file is still good"
        );
        assert_eq!(
            meeting.duration_seconds,
            Some(600.0),
            "an unreadable length does not discard the file's moment"
        );
    }
    #[tokio::test]
    async fn a_meeting_with_nothing_to_measure_keeps_an_unknown_length() {
        let pool = migrated_pool().await;

        let meeting_id =
            TranscriptsRepository::save_transcript(&pool, "Empty call", &[], None)
                .await
                .unwrap();

        let meeting = listed_meeting(&pool, &meeting_id).await;
        assert_eq!(
            meeting.duration_seconds, None,
            "no file and no segments means unknown, never zero"
        );
    }

    #[tokio::test]
    async fn a_saved_recording_keeps_its_audit_created_at() {
        let pool = migrated_pool().await;
        let folder = folder_with_metadata(serde_json::json!({
            "created_at": "2026-11-18T14:32:05+00:00",
            "duration_seconds": 60,
        }));
        let before = chrono::Utc::now();

        let meeting_id = TranscriptsRepository::save_transcript(
            &pool,
            "Audited call",
            &[segment("hello", 5.0)],
            Some(folder.path().to_string_lossy().to_string()),
        )
        .await
        .unwrap();

        let (created_at, _, _) = meeting_row(&pool, &meeting_id).await;
        let created_at = chrono::DateTime::parse_from_rfc3339(&created_at)
            .expect("created_at stays an RFC3339 moment")
            .with_timezone(&chrono::Utc);
        assert!(
            created_at >= before,
            "created_at remains the moment the row appeared"
        );
    }

    #[tokio::test]
    async fn saving_a_recording_takes_the_meeting_moment_and_length_from_its_folder() {
        let pool = migrated_pool().await;
        let folder = folder_with_metadata(serde_json::json!({
            "version": "1.0",
            "created_at": "2026-11-18T14:32:05+00:00",
            "completed_at": "2026-11-18T15:55:00+00:00",
            "duration_seconds": 4_975.5,
            "status": "completed",
        }));

        let meeting_id = TranscriptsRepository::save_transcript(
            &pool,
            "Planning call",
            &[segment("first", 10.0), segment("last", 20.0)],
            Some(folder.path().to_string_lossy().to_string()),
        )
        .await
        .unwrap();

        let meeting = listed_meeting(&pool, &meeting_id).await;

        assert_eq!(
            meeting
                .recorded_at
                .as_ref()
                .map(|moment| moment.0.to_rfc3339()),
            Some("2026-11-18T14:32:05+00:00".to_string()),
            "the meeting is dated by the start of the recording, not by the save"
        );
        assert_eq!(
            meeting.duration_seconds,
            Some(4_975.5),
            "the length is the recording's own, not the end of the last segment"
        );
        assert_ne!(
            meeting.created_at.0.to_rfc3339(),
            "2026-11-18T14:32:05+00:00",
            "created_at stays the moment the row appeared"
        );
    }
}
