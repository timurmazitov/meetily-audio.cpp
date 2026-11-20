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
        let pool = empty_pool().await;
        sqlx::migrate!("./migrations")
            .run(&pool)
            .await
            .expect("migrations apply to a fresh database");
        pool
    }

    async fn empty_pool() -> SqlitePool {
        SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .expect("in-memory sqlite")
    }

    const SPEAKER_NAMES_MIGRATION_VERSION: i64 = 20_261_118_000_000;
    const MEETING_DATE_COLUMNS_MIGRATION_VERSION: i64 = 20_261_119_000_000;
    const TRANSCRIBE_BACKEND_MIGRATION_VERSION: i64 = 20_261_120_000_000;

    /// Applies the chain up to (but excluding) the given migration, so a test
    /// can seed rows that existed before a later migration introduced columns.
    async fn migrated_pool_before(version: i64) -> SqlitePool {
        let pool = empty_pool().await;
        let all = sqlx::migrate!("./migrations");
        let older = sqlx::migrate::Migrator {
            migrations: std::borrow::Cow::Owned(
                all.migrations
                    .iter()
                    .filter(|migration| migration.version < version)
                    .cloned()
                    .collect(),
            ),
            ..sqlx::migrate::Migrator::DEFAULT
        };
        older
            .run(&pool)
            .await
            .expect("the migrations older than the target version apply");
        pool
    }

    /// Applies the chain up to (but excluding) the speaker-names migration, so
    /// a test can seed rows that existed before names were introduced.
    async fn migrated_pool_before_speaker_names() -> SqlitePool {
        migrated_pool_before(SPEAKER_NAMES_MIGRATION_VERSION).await
    }

    async fn apply_full_migration_chain(pool: &SqlitePool) {
        sqlx::migrate!("./migrations")
            .run(pool)
            .await
            .expect("the speaker-names migration applies on top of an existing database");
    }

    /// Adds one speaker to the global directory and returns its id.
    async fn insert_speaker(pool: &SqlitePool, id: &str, name: &str) -> Result<(), sqlx::Error> {
        sqlx::query(
            "INSERT INTO speakers (id, name, created_at, updated_at) VALUES (?, ?, ?, ?)",
        )
        .bind(id)
        .bind(name)
        .bind("2024-01-01T00:00:00Z")
        .bind("2024-01-01T00:00:00Z")
        .execute(pool)
        .await?;
        Ok(())
    }

    async fn insert_meeting_speaker(
        pool: &SqlitePool,
        id: &str,
        meeting_id: &str,
        label: &str,
        speaker_id: &str,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            "INSERT INTO meeting_speakers (id, meeting_id, label, speaker_id, created_at, updated_at)
             VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind(id)
        .bind(meeting_id)
        .bind(label)
        .bind(speaker_id)
        .bind("2024-01-01T00:00:00Z")
        .bind("2024-01-01T00:00:00Z")
        .execute(pool)
        .await?;
        Ok(())
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
    async fn speaker_names_migration_applies_to_a_database_with_legacy_transcripts() {
        let pool = migrated_pool_before_speaker_names().await;

        sqlx::query("INSERT INTO meetings (id, title, created_at, updated_at) VALUES (?, ?, ?, ?)")
            .bind("meeting-legacy")
            .bind("Legacy meeting")
            .bind("2024-01-01T00:00:00Z")
            .bind("2024-01-01T00:00:00Z")
            .execute(&pool)
            .await
            .unwrap();

        sqlx::query(
            "INSERT INTO transcripts (id, meeting_id, transcript, timestamp, speaker)
             VALUES (?, ?, ?, ?, ?)",
        )
        .bind("transcript-legacy")
        .bind("meeting-legacy")
        .bind("Legacy text")
        .bind("2024-01-01T00:00:00Z")
        .bind("speaker_0")
        .execute(&pool)
        .await
        .unwrap();

        apply_full_migration_chain(&pool).await;

        // The legacy rows survive the migration untouched.
        let row = sqlx::query(
            "SELECT transcript, speaker FROM transcripts WHERE id = 'transcript-legacy'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(row.get::<String, _>("transcript"), "Legacy text");
        assert_eq!(row.get::<Option<String>, _>("speaker").as_deref(), Some("speaker_0"));

        // No name was invented for the old label.
        let names: Vec<(String,)> = sqlx::query_as("SELECT id FROM meeting_speakers")
            .fetch_all(&pool)
            .await
            .unwrap();
        assert!(names.is_empty(), "legacy meetings get no speaker names");

        let speakers: Vec<(String,)> = sqlx::query_as("SELECT id FROM speakers")
            .fetch_all(&pool)
            .await
            .unwrap();
        assert!(speakers.is_empty(), "the directory starts empty");
    }

    #[tokio::test]
    async fn meeting_speakers_reject_a_duplicate_label_within_one_meeting() {
        let pool = migrated_pool().await;

        sqlx::query("INSERT INTO meetings (id, title, created_at, updated_at) VALUES (?, ?, ?, ?)")
            .bind("meeting-1")
            .bind("Meeting")
            .bind("2024-01-01T00:00:00Z")
            .bind("2024-01-01T00:00:00Z")
            .execute(&pool)
            .await
            .unwrap();
        insert_speaker(&pool, "speaker-alex", "Alex").await.unwrap();
        insert_speaker(&pool, "speaker-maria", "Maria").await.unwrap();

        insert_meeting_speaker(&pool, "ms-1", "meeting-1", "speaker_0", "speaker-alex")
            .await
            .unwrap();

        let duplicate = insert_meeting_speaker(
            &pool,
            "ms-2",
            "meeting-1",
            "speaker_0",
            "speaker-maria",
        )
        .await;
        assert!(
            duplicate.is_err(),
            "one label cannot map to two people in the same meeting"
        );
    }

    #[tokio::test]
    async fn meeting_speakers_reject_one_person_assigned_to_two_labels() {
        let pool = migrated_pool().await;

        sqlx::query("INSERT INTO meetings (id, title, created_at, updated_at) VALUES (?, ?, ?, ?)")
            .bind("meeting-1")
            .bind("Meeting")
            .bind("2024-01-01T00:00:00Z")
            .bind("2024-01-01T00:00:00Z")
            .execute(&pool)
            .await
            .unwrap();
        insert_speaker(&pool, "speaker-alex", "Alex").await.unwrap();

        insert_meeting_speaker(&pool, "ms-1", "meeting-1", "speaker_0", "speaker-alex")
            .await
            .unwrap();

        let duplicate =
            insert_meeting_speaker(&pool, "ms-2", "meeting-1", "speaker_1", "speaker-alex").await;
        assert!(
            duplicate.is_err(),
            "one person cannot be two speakers of the same meeting"
        );

        // The same person may still speak in another meeting.
        sqlx::query("INSERT INTO meetings (id, title, created_at, updated_at) VALUES (?, ?, ?, ?)")
            .bind("meeting-2")
            .bind("Another meeting")
            .bind("2024-01-01T00:00:00Z")
            .bind("2024-01-01T00:00:00Z")
            .execute(&pool)
            .await
            .unwrap();
        insert_meeting_speaker(&pool, "ms-3", "meeting-2", "speaker_0", "speaker-alex")
            .await
            .expect("the directory is global across meetings");
    }

    #[tokio::test]
    async fn speaker_names_are_unique_case_insensitively() {
        let pool = migrated_pool().await;

        insert_speaker(&pool, "speaker-alex", "Alex").await.unwrap();

        let duplicate = insert_speaker(&pool, "speaker-alex-2", "alex").await;
        assert!(
            duplicate.is_err(),
            "Alex and alex are the same person in the directory"
        );

        insert_speaker(&pool, "speaker-maria", "Maria")
            .await
            .expect("a different name is accepted");
    }

    #[tokio::test]
    async fn meeting_date_columns_leave_legacy_rows_readable_as_unknown() {
        use crate::database::repositories::meeting::MeetingsRepository;

        let pool = migrated_pool_before(MEETING_DATE_COLUMNS_MIGRATION_VERSION).await;

        sqlx::query("INSERT INTO meetings (id, title, created_at, updated_at) VALUES (?, ?, ?, ?)")
            .bind("meeting-legacy")
            .bind("Legacy meeting")
            .bind("2024-05-06T07:08:09Z")
            .bind("2024-05-06T07:08:09Z")
            .execute(&pool)
            .await
            .unwrap();

        apply_full_migration_chain(&pool).await;

        let row = sqlx::query(
            "SELECT created_at, recorded_at, duration_seconds FROM meetings WHERE id = 'meeting-legacy'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert!(
            row.get::<Option<String>, _>("recorded_at").is_none(),
            "a legacy row has no meeting date: NULL, never a substituted moment"
        );
        assert!(
            row.get::<Option<f64>, _>("duration_seconds").is_none(),
            "a legacy row has no duration: NULL, never zero"
        );
        assert_eq!(
            row.get::<String, _>("created_at"),
            "2024-05-06T07:08:09Z",
            "created_at keeps its value and meaning"
        );

        // The list reads through the repository, so reading must not fail on
        // rows the migration only just extended.
        let meetings = MeetingsRepository::get_meetings(&pool).await.unwrap();
        assert_eq!(meetings.len(), 1);
        assert_eq!(
            meetings[0].recorded_at.as_ref().map(|d| d.0.to_rfc3339()),
            None
        );
        assert_eq!(meetings[0].duration_seconds, None);
        assert_eq!(
            meetings[0].created_at.0.to_rfc3339(),
            "2024-05-06T07:08:09+00:00",
            "the legacy created_at is unchanged"
        );
    }

    #[tokio::test]
    async fn transcribe_backend_column_defaults_to_auto_on_existing_rows() {
        use crate::database::repositories::setting::SettingsRepository;

        // A database saved before the selector existed: a provider/model row
        // with no compute mode anywhere.
        let pool = migrated_pool_before(TRANSCRIBE_BACKEND_MIGRATION_VERSION).await;
        sqlx::query("INSERT INTO transcript_settings (id, provider, model) VALUES ('1', ?, ?)")
            .bind("audioCpp")
            .bind("gigaam-v3-e2e-ctc-f16")
            .execute(&pool)
            .await
            .unwrap();

        apply_full_migration_chain(&pool).await;

        // The existing row keeps its stored values and reads back as Auto.
        // (The soft migration normalizes the removed provider to
        // transcribeCpp on read — covered in the settings repository tests;
        // here the point is that the pre-existing row still opens and its
        // new column defaults to 'auto'.)
        let setting = SettingsRepository::get_transcript_config(&pool)
            .await
            .unwrap()
            .expect("the pre-existing row still opens");
        assert_eq!(setting.provider, "transcribeCpp");
        assert_eq!(setting.model, crate::config::DEFAULT_TRANSCRIBE_CPP_MODEL);
        assert_eq!(
            setting.transcribe_backend.as_deref(),
            Some("auto"),
            "existing rows keep their values and default to the pre-selector behaviour"
        );
        let raw: (String, String) = sqlx::query_as(
            "SELECT provider, model FROM transcript_settings WHERE id = '1'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(
            raw,
            ("audioCpp".to_string(), "gigaam-v3-e2e-ctc-f16".to_string()),
            "the soft migration is read-only: no migration touches the stored row"
        );

        // A fresh database gets the same default through the same chain.
        let fresh = migrated_pool().await;
        SettingsRepository::save_transcript_config(&fresh, "transcribeCpp", "parakeet-ultra-Q8_0", "vulkan")
            .await
            .unwrap();
        let setting = SettingsRepository::get_transcript_config(&fresh)
            .await
            .unwrap()
            .expect("mode saved");
        assert_eq!(setting.transcribe_backend.as_deref(), Some("vulkan"));
    }

    #[tokio::test]
    async fn speaker_labels_round_trip_through_the_transcript_repository() {        use crate::api::TranscriptSegment;
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
