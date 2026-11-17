// database/migrations_tests.rs
//
// Verifies the migration chain applies to a fresh database and that the
// diarization `speaker` column is nullable, so transcripts saved before
// diarization existed still open unchanged.

#[cfg(test)]
mod tests {
    use sqlx::sqlite::SqlitePoolOptions;
    use sqlx::{Row, SqlitePool};

    /// Applies the repo's real migration chain to an in-memory database.
    async fn migrated_pool() -> SqlitePool {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .expect("in-memory sqlite");
        sqlx::migrate!("./migrations")
            .run(&pool)
            .await
            .expect("migrations apply to a fresh database");
        pool
    }

    #[tokio::test]
    async fn migrations_apply_to_a_fresh_database() {
        let pool = migrated_pool().await;

        let tables: Vec<(String,)> = sqlx::query_as(
            "SELECT name FROM sqlite_master WHERE type = 'table' AND name = 'transcripts'",
        )
        .fetch_all(&pool)
        .await
        .unwrap();

        assert_eq!(tables.len(), 1, "transcripts table exists");
    }

    #[tokio::test]
    async fn speaker_column_is_nullable_so_legacy_transcripts_open() {
        let pool = migrated_pool().await;

        sqlx::query("INSERT INTO meetings (id, title, created_at, updated_at) VALUES (?, ?, ?, ?)")
            .bind("meeting-legacy")
            .bind("Legacy meeting")
            .bind("2024-01-01T00:00:00Z")
            .bind("2024-01-01T00:00:00Z")
            .execute(&pool)
            .await
            .unwrap();

        // A pre-diarization row has no speaker value at all.
        sqlx::query(
            "INSERT INTO transcripts (id, meeting_id, transcript, timestamp, audio_start_time, audio_end_time, duration)
             VALUES (?, ?, ?, ?, ?, ?, ?)",
        )
        .bind("transcript-legacy")
        .bind("meeting-legacy")
        .bind("Legacy text")
        .bind("2024-01-01T00:00:00Z")
        .bind(0.0_f64)
        .bind(1.0_f64)
        .bind(1.0_f64)
        .execute(&pool)
        .await
        .expect("legacy row inserts with speaker defaulting to NULL");

        let row = sqlx::query("SELECT speaker FROM transcripts WHERE id = ?")
            .bind("transcript-legacy")
            .fetch_one(&pool)
            .await
            .unwrap();
        let speaker: Option<String> = row.get("speaker");
        assert_eq!(speaker, None, "legacy transcripts keep no speaker label");

        // A diarized row can carry a label.
        sqlx::query("UPDATE transcripts SET speaker = ? WHERE id = ?")
            .bind("speaker_0")
            .bind("transcript-legacy")
            .execute(&pool)
            .await
            .unwrap();

        let row = sqlx::query("SELECT speaker FROM transcripts WHERE id = ?")
            .bind("transcript-legacy")
            .fetch_one(&pool)
            .await
            .unwrap();
        let speaker: Option<String> = row.get("speaker");
        assert_eq!(speaker.as_deref(), Some("speaker_0"));
    }

    #[tokio::test]
    async fn summary_system_prompt_round_trips_through_the_settings_repository() {
        use crate::database::repositories::setting::SettingsRepository;

        let pool = migrated_pool().await;

        // A fresh database has no stored prompt — the repository reports None
        // and the read path (command) falls back to the default constant.
        assert_eq!(
            SettingsRepository::get_summary_system_prompt(&pool)
                .await
                .unwrap(),
            None
        );

        SettingsRepository::save_summary_system_prompt(&pool, "Custom prompt {template}")
            .await
            .unwrap();

        assert_eq!(
            SettingsRepository::get_summary_system_prompt(&pool)
                .await
                .unwrap()
                .as_deref(),
            Some("Custom prompt {template}")
        );

        // Saving again overwrites the previous value (single settings row).
        SettingsRepository::save_summary_system_prompt(&pool, "Updated prompt")
            .await
            .unwrap();

        assert_eq!(
            SettingsRepository::get_summary_system_prompt(&pool)
                .await
                .unwrap()
                .as_deref(),
            Some("Updated prompt")
        );
    }

    #[tokio::test]
    async fn speaker_labels_round_trip_through_the_transcript_repository() {
        use crate::api::TranscriptSegment;
        use crate::database::repositories::transcript::TranscriptsRepository;

        let pool = migrated_pool().await;
        let segments = vec![TranscriptSegment {
            id: "transcript-1".to_string(),
            text: "Hello from speaker one".to_string(),
            timestamp: "2024-01-01T00:00:00Z".to_string(),
            audio_start_time: Some(0.0),
            audio_end_time: Some(2.0),
            duration: Some(2.0),
            speaker: Some("speaker_0".to_string()),
        }];

        let meeting_id = TranscriptsRepository::save_transcript(
            &pool,
            "Diarized meeting",
            &segments,
            None,
        )
        .await
        .unwrap();

        let row = sqlx::query("SELECT speaker FROM transcripts WHERE meeting_id = ?")
            .bind(&meeting_id)
            .fetch_one(&pool)
            .await
            .unwrap();
        let speaker: Option<String> = row.get("speaker");
        assert_eq!(speaker.as_deref(), Some("speaker_0"));
    }
}
