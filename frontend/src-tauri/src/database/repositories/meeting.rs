use crate::api::{MeetingDetails, MeetingTranscript};
use crate::database::models::{MeetingModel, Transcript};
use crate::database::repositories::speaker::SpeakersRepository;
use chrono::Utc;
use sqlx::{Connection, Error as SqlxError, SqliteConnection, SqlitePool};
use log::{error, info};
use std::collections::HashMap;

pub struct MeetingsRepository;

/// Resolves the real name for a segment from the meeting's label→name map.
/// The segment keeps its diarization label; the name is a separate entity.
fn resolve_speaker_name(label: &Option<String>, names: &HashMap<String, String>) -> Option<String> {
    label.as_ref().and_then(|label| names.get(label)).cloned()
}

impl MeetingsRepository {
    pub async fn get_meetings(pool: &SqlitePool) -> Result<Vec<MeetingModel>, sqlx::Error> {
        // Chronology of meetings, not of rows: an import of an old recording
        // lands on its historical place even though its `created_at` is today.
        // `COALESCE` keeps a meeting without a date from sinking to the bottom —
        // it falls back to when the row appeared. Both columns hold RFC3339 UTC
        // text written by the same path, so the comparison is lexicographic.
        let meetings = sqlx::query_as::<_, MeetingModel>(
            "SELECT * FROM meetings ORDER BY COALESCE(recorded_at, created_at) DESC",
        )
        .fetch_all(pool)
        .await?;
        Ok(meetings)
    }

    pub async fn delete_meeting(pool: &SqlitePool, meeting_id: &str) -> Result<bool, SqlxError> {
        if meeting_id.trim().is_empty() {
            return Err(SqlxError::Protocol(
                "meeting_id cannot be empty".to_string(),
            ));
        }

        let mut conn = pool.acquire().await?;
        let mut transaction = conn.begin().await?;

        match delete_meeting_with_transaction(&mut transaction, meeting_id).await {
            Ok(success) => {
                if success {
                    transaction.commit().await?;
                    info!(
                        "Successfully deleted meeting {} and all associated data",
                        meeting_id
                    );
                    Ok(true)
                } else {
                    transaction.rollback().await?;
                    Ok(false)
                }
            }
            Err(e) => {
                let _ = transaction.rollback().await;
                error!("Failed to delete meeting {}: {}", meeting_id, e);
                Err(e)
            }
        }
    }

    pub async fn get_meeting(
        pool: &SqlitePool,
        meeting_id: &str,
    ) -> Result<Option<MeetingDetails>, SqlxError> {
        if meeting_id.trim().is_empty() {
            return Err(SqlxError::Protocol(
                "meeting_id cannot be empty".to_string(),
            ));
        }

        let mut conn = pool.acquire().await?;
        let mut transaction = conn.begin().await?;

        // Get meeting details
        let meeting: Option<MeetingModel> =
            sqlx::query_as("SELECT id, title, created_at, updated_at, folder_path, recorded_at, duration_seconds FROM meetings WHERE id = ?")
                .bind(meeting_id)
                .fetch_optional(&mut *transaction)
                .await?;

        if meeting.is_none() {
            transaction.rollback().await?;
            return Err(SqlxError::RowNotFound);
        }

        if let Some(meeting) = meeting {
            // Get all transcripts for this meeting
            let transcripts =
                sqlx::query_as::<_, Transcript>("SELECT * FROM transcripts WHERE meeting_id = ?")
                    .bind(meeting_id)
                    .fetch_all(&mut *transaction)
                    .await?;

            // Names come from the same transaction, so the snapshot matches.
            let speaker_names =
                SpeakersRepository::speaker_name_map(&mut *transaction, meeting_id).await?;

            transaction.commit().await?;

            // Convert Transcript to MeetingTranscript
            let meeting_transcripts = transcripts
                .into_iter()
                .map(|t| MeetingTranscript {
                    speaker_name: resolve_speaker_name(&t.speaker, &speaker_names),
                    id: t.id,
                    text: t.transcript,
                    timestamp: t.timestamp,
                    audio_start_time: t.audio_start_time,
                    audio_end_time: t.audio_end_time,
                    duration: t.duration,
                    speaker: t.speaker,
                })
                .collect::<Vec<_>>();

            Ok(Some(MeetingDetails {
                id: meeting.id,
                title: meeting.title,
                created_at: meeting.created_at.0.to_rfc3339(),
                updated_at: meeting.updated_at.0.to_rfc3339(),
                folder_path: meeting.folder_path,
                transcripts: meeting_transcripts,
            }))
        } else {
            transaction.rollback().await?;
            Ok(None)
        }
    }

    /// Get meeting metadata without transcripts (for pagination)
    pub async fn get_meeting_metadata(
        pool: &SqlitePool,
        meeting_id: &str,
    ) -> Result<Option<MeetingModel>, SqlxError> {
        if meeting_id.trim().is_empty() {
            return Err(SqlxError::Protocol(
                "meeting_id cannot be empty".to_string(),
            ));
        }

        let meeting: Option<MeetingModel> =
            sqlx::query_as("SELECT id, title, created_at, updated_at, folder_path, recorded_at, duration_seconds FROM meetings WHERE id = ?")
                .bind(meeting_id)
                .fetch_optional(pool)
                .await?;

        Ok(meeting)
    }

    /// Get meeting transcripts with pagination support
    pub async fn get_meeting_transcripts_paginated(
        pool: &SqlitePool,
        meeting_id: &str,
        limit: i64,
        offset: i64,
    ) -> Result<(Vec<Transcript>, i64), SqlxError> {
        if meeting_id.trim().is_empty() {
            return Err(SqlxError::Protocol(
                "meeting_id cannot be empty".to_string(),
            ));
        }

        // Get total count of transcripts for this meeting
        let total: (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM transcripts WHERE meeting_id = ?"
        )
        .bind(meeting_id)
        .fetch_one(pool)
        .await?;

        // Get paginated transcripts ordered by audio_start_time
        let mut transcripts = sqlx::query_as::<_, Transcript>(
            "SELECT * FROM transcripts
             WHERE meeting_id = ?
             ORDER BY audio_start_time ASC
             LIMIT ? OFFSET ?"
        )
        .bind(meeting_id)
        .bind(limit)
        .bind(offset)
        .fetch_all(pool)
        .await?;

        // One extra query per page; names belong to the meeting, not to the page.
        let speaker_names = SpeakersRepository::speaker_name_map(pool, meeting_id).await?;
        for transcript in &mut transcripts {
            transcript.speaker_name = resolve_speaker_name(&transcript.speaker, &speaker_names);
        }

        Ok((transcripts, total.0))
    }

    pub async fn update_meeting_title(
        pool: &SqlitePool,
        meeting_id: &str,
        new_title: &str,
    ) -> Result<bool, SqlxError> {
        if meeting_id.trim().is_empty() {
            return Err(SqlxError::Protocol(
                "meeting_id cannot be empty".to_string(),
            ));
        }

        let mut conn = pool.acquire().await?;
        let mut transaction = conn.begin().await?;

        let now = Utc::now().naive_utc();

        let rows_affected =
            sqlx::query("UPDATE meetings SET title = ?, updated_at = ? WHERE id = ?")
                .bind(new_title)
                .bind(now)
                .bind(meeting_id)
                .execute(&mut *transaction)
                .await?;
        if rows_affected.rows_affected() == 0 {
            transaction.rollback().await?;
            return Ok(false);
        }
        transaction.commit().await?;
        Ok(true)
    }

    pub async fn update_meeting_name(
        pool: &SqlitePool,
        meeting_id: &str,
        new_title: &str,
    ) -> Result<bool, SqlxError> {
        let mut transaction = pool.begin().await?;
        let now = Utc::now();

        // Update meetings table
        let meeting_update =
            sqlx::query("UPDATE meetings SET title = ?, updated_at = ? WHERE id = ?")
                .bind(new_title)
                .bind(now)
                .bind(meeting_id)
                .execute(&mut *transaction)
                .await?;

        if meeting_update.rows_affected() == 0 {
            transaction.rollback().await?;
            return Ok(false); // Meeting not found
        }

        // Update transcript_chunks table
        sqlx::query("UPDATE transcript_chunks SET meeting_name = ? WHERE meeting_id = ?")
            .bind(new_title)
            .bind(meeting_id)
            .execute(&mut *transaction)
            .await?;

        transaction.commit().await?;
        Ok(true)
    }
}

async fn delete_meeting_with_transaction(
    transaction: &mut SqliteConnection,
    meeting_id: &str,
) -> Result<bool, SqlxError> {
    // Check if meeting exists
    let meeting_exists: Option<(i64,)> = sqlx::query_as("SELECT 1 FROM meetings WHERE id = ?")
        .bind(meeting_id)
        .fetch_optional(&mut *transaction)
        .await?;

    if meeting_exists.is_none() {
        error!("Meeting {} not found for deletion", meeting_id);
        return Ok(false);
    }

    // Delete from related tables in proper order
    // 1. Delete from transcript_chunks
    sqlx::query("DELETE FROM transcript_chunks WHERE meeting_id = ?")
        .bind(meeting_id)
        .execute(&mut *transaction)
        .await?;

    // 2. Delete from summary_processes
    sqlx::query("DELETE FROM summary_processes WHERE meeting_id = ?")
        .bind(meeting_id)
        .execute(&mut *transaction)
        .await?;

    // 3. Delete from transcripts
    sqlx::query("DELETE FROM transcripts WHERE meeting_id = ?")
        .bind(meeting_id)
        .execute(&mut *transaction)
        .await?;

    // 4. Finally, delete the meeting
    let result = sqlx::query("DELETE FROM meetings WHERE id = ?")
        .bind(meeting_id)
        .execute(&mut *transaction)
        .await?;

    // 5. Directory rows the meeting's cascade left unreferenced are garbage:
    // names are immutable, and an orphan would keep showing up in the picker.
    sqlx::query(
        "DELETE FROM speakers
         WHERE id NOT IN (SELECT speaker_id FROM meeting_speakers)",
    )
    .execute(&mut *transaction)
    .await?;

    Ok(result.rows_affected() > 0)
}

#[cfg(test)]
mod tests {
    use super::MeetingsRepository;
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

    /// A row whose audit field (`created_at`) and meeting date (`recorded_at`)
    /// deliberately disagree: an import of an old recording.
    async fn seed_meeting_with_moments(
        pool: &SqlitePool,
        meeting_id: &str,
        created_at: &str,
        recorded_at: Option<&str>,
    ) {
        sqlx::query(
            "INSERT INTO meetings (id, title, created_at, updated_at, recorded_at)
             VALUES (?, ?, ?, ?, ?)",
        )
        .bind(meeting_id)
        .bind(meeting_id)
        .bind(created_at)
        .bind(created_at)
        .bind(recorded_at)
        .execute(pool)
        .await
        .unwrap();
    }

    /// The list exactly as the list command returns it, ids only.
    async fn listed_ids(pool: &SqlitePool) -> Vec<String> {
        MeetingsRepository::get_meetings(pool)
            .await
            .unwrap()
            .into_iter()
            .map(|meeting| meeting.id)
            .collect()
    }

    #[tokio::test]
    async fn an_archived_import_sits_on_its_historical_place_not_on_top() {
        let pool = migrated_pool().await;
        // Both rows appeared in the database today; only one meeting happened today.
        seed_meeting_with_moments(
            &pool,
            "today",
            "2026-11-18T10:00:00Z",
            Some("2026-11-18T09:00:00Z"),
        )
        .await;
        seed_meeting_with_moments(
            &pool,
            "archived",
            "2026-11-18T11:00:00Z",
            Some("2020-03-04T09:00:00Z"),
        )
        .await;

        assert_eq!(
            listed_ids(&pool).await,
            ["today", "archived"],
            "the meeting that happened today stays on top, the freshly imported old one goes under it"
        );
    }

    #[tokio::test]
    async fn a_meeting_without_a_date_does_not_sink_to_the_bottom() {
        let pool = migrated_pool().await;
        seed_meeting_with_moments(&pool, "undated", "2026-11-17T10:00:00Z", None).await;
        seed_meeting_with_moments(
            &pool,
            "archived",
            "2026-11-18T10:00:00Z",
            Some("2020-03-04T09:00:00Z"),
        )
        .await;

        assert_eq!(
            listed_ids(&pool).await,
            ["undated", "archived"],
            "unknown date falls back to when the row appeared, so it is not last"
        );
    }

    async fn seed_meeting_with_two_speakers(pool: &SqlitePool, meeting_id: &str) {
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
            ("t-2", None, 2.0),
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
    }

    async fn assign_name(pool: &SqlitePool, meeting_id: &str, label: &str, name: &str) {
        let person_id = format!("person-{}", name.to_lowercase());
        sqlx::query(
            "INSERT OR IGNORE INTO speakers (id, name, created_at, updated_at) VALUES (?, ?, ?, ?)",
        )
        .bind(&person_id)
        .bind(name)
        .bind("2024-01-01T00:00:00Z")
        .bind("2024-01-01T00:00:00Z")
        .execute(pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO meeting_speakers (id, meeting_id, label, speaker_id, created_at, updated_at)
             VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind(format!("assignment-{}-{}", meeting_id, label))
        .bind(meeting_id)
        .bind(label)
        .bind(&person_id)
        .bind("2024-01-01T00:00:00Z")
        .bind("2024-01-01T00:00:00Z")
        .execute(pool)
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn reading_a_meeting_resolves_speaker_names_per_segment() {
        let pool = migrated_pool().await;
        seed_meeting_with_two_speakers(&pool, "meeting-1").await;
        assign_name(&pool, "meeting-1", "speaker_0", "Alex").await;

        let meeting = MeetingsRepository::get_meeting(&pool, "meeting-1")
            .await
            .unwrap()
            .expect("meeting exists");

        let by_id = |id: &str| {
            meeting
                .transcripts
                .iter()
                .find(|t| t.id == id)
                .unwrap_or_else(|| panic!("segment {id} present"))
        };

        assert_eq!(by_id("t-0").speaker_name.as_deref(), Some("Alex"));
        assert_eq!(by_id("t-0").speaker.as_deref(), Some("speaker_0"));
        assert_eq!(
            by_id("t-1").speaker_name,
            None,
            "an unnamed label keeps none"
        );
        assert_eq!(
            by_id("t-2").speaker_name,
            None,
            "an unlabelled segment keeps none"
        );
    }

    /// Second meeting with its own transcript ids (the shared seeder above uses
    /// fixed ids, so it can only run once per database).
    async fn seed_second_meeting(pool: &SqlitePool, meeting_id: &str) {
        sqlx::query("INSERT INTO meetings (id, title, created_at, updated_at) VALUES (?, ?, ?, ?)")
            .bind(meeting_id)
            .bind("Second meeting")
            .bind("2024-01-02T00:00:00Z")
            .bind("2024-01-02T00:00:00Z")
            .execute(pool)
            .await
            .unwrap();

        for (index, speaker) in [Some("speaker_0"), Some("speaker_1")].into_iter().enumerate() {
            sqlx::query(
                "INSERT INTO transcripts (id, meeting_id, transcript, timestamp, speaker, audio_start_time)
                 VALUES (?, ?, ?, ?, ?, ?)",
            )
            .bind(format!("{meeting_id}-t-{index}"))
            .bind(meeting_id)
            .bind("Text")
            .bind("2024-01-02T00:00:00Z")
            .bind(speaker)
            .bind(index as f64)
            .execute(pool)
            .await
            .unwrap();
        }
    }

    async fn speaker_names(pool: &SqlitePool) -> Vec<String> {
        sqlx::query_scalar("SELECT name FROM speakers ORDER BY name")
            .fetch_all(pool)
            .await
            .unwrap()
    }

    async fn bindings(pool: &SqlitePool, meeting_id: &str) -> Vec<(String, String)> {
        sqlx::query_as(
            "SELECT ms.label, s.name FROM meeting_speakers ms
             JOIN speakers s ON s.id = ms.speaker_id
             WHERE ms.meeting_id = ? ORDER BY ms.label",
        )
        .bind(meeting_id)
        .fetch_all(pool)
        .await
        .unwrap()
    }

    #[tokio::test]
    async fn deleting_a_meeting_removes_the_names_no_meeting_uses_any_more() {
        let pool = migrated_pool().await;
        seed_meeting_with_two_speakers(&pool, "meeting-1").await;
        assign_name(&pool, "meeting-1", "speaker_0", "Alex").await;

        let deleted = MeetingsRepository::delete_meeting(&pool, "meeting-1")
            .await
            .unwrap();

        assert!(deleted);
        assert!(
            bindings(&pool, "meeting-1").await.is_empty(),
            "the meeting's bindings go with it"
        );
        assert!(
            speaker_names(&pool).await.is_empty(),
            "the person nobody references any more is gone from the directory"
        );

        // The directory is read back through the command the picker uses, so an
        // orphan would show up there.
        let directory = crate::database::repositories::speaker::SpeakersRepository::search_names(
            &pool, "", 20,
        )
        .await
        .unwrap();
        assert!(directory.is_empty(), "the orphan would be suggested again");
    }

    #[tokio::test]
    async fn deleting_a_meeting_keeps_a_person_another_meeting_still_uses() {
        let pool = migrated_pool().await;
        seed_meeting_with_two_speakers(&pool, "meeting-1").await;
        seed_second_meeting(&pool, "meeting-2").await;
        assign_name(&pool, "meeting-1", "speaker_0", "Alex").await;
        assign_name(&pool, "meeting-2", "speaker_0", "Alex").await;
        assign_name(&pool, "meeting-2", "speaker_1", "Zoe").await;

        MeetingsRepository::delete_meeting(&pool, "meeting-1")
            .await
            .unwrap();

        assert_eq!(
            speaker_names(&pool).await,
            ["Alex", "Zoe"],
            "a person another meeting still uses survives the deletion"
        );
        assert_eq!(
            bindings(&pool, "meeting-2").await,
            [
                ("speaker_0".to_string(), "Alex".to_string()),
                ("speaker_1".to_string(), "Zoe".to_string()),
            ],
            "the other meeting keeps its names"
        );
    }

    #[tokio::test]
    async fn paginated_transcripts_carry_the_speaker_names_of_the_meeting() {
        let pool = migrated_pool().await;
        seed_meeting_with_two_speakers(&pool, "meeting-1").await;
        assign_name(&pool, "meeting-1", "speaker_0", "Alex").await;

        let (transcripts, total) =
            MeetingsRepository::get_meeting_transcripts_paginated(&pool, "meeting-1", 10, 0)
                .await
                .unwrap();

        assert_eq!(total, 3);
        let named: Vec<Option<&str>> = transcripts
            .iter()
            .map(|t| t.speaker_name.as_deref())
            .collect();
        assert_eq!(named, [Some("Alex"), None, None]);
        assert_eq!(
            transcripts[0].speaker.as_deref(),
            Some("speaker_0"),
            "the label itself is unchanged"
        );
    }
}
