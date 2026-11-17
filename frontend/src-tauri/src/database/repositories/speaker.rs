// Speaker names: reading the speakers of a meeting and resolving the name for
// a diarization label. `transcripts.speaker` stays the label (`speaker_N`);
// a real name is a separate entity (see `.scratch/speaker-names/spec.md`).

use sqlx::{FromRow, Executor, Sqlite, SqlitePool};
use std::collections::HashMap;

/// One row of the meeting's speaker list: a diarization label, the optionally
/// assigned person, and how often the label occurs in the transcripts.
#[derive(Debug, Clone, FromRow)]
pub struct MeetingSpeakerSummary {
    pub label: String,
    pub speaker_name: Option<String>,
    pub speaker_id: Option<String>,
    pub segment_count: i64,
    pub first_start_time: Option<f64>,
}

/// Only diarization labels of the form `speaker_N` are speakers. Legacy
/// `mic`/`system` values and free-form strings are audio sources, not people,
/// and never appear in the speaker list or the dropdown.
fn is_speaker_label(label: &str) -> bool {
    match label.strip_prefix("speaker_") {
        Some(number) => !number.is_empty() && number.bytes().all(|byte| byte.is_ascii_digit()),
        None => false,
    }
}

pub struct SpeakersRepository;

impl SpeakersRepository {
    /// Lists the speakers of one meeting with their assignments and segment
    /// counts. The list is derived from `DISTINCT speaker` over `transcripts`,
    /// so it can never drift from the labels actually present.
    pub async fn list_meeting_speakers(
        pool: &SqlitePool,
        meeting_id: &str,
    ) -> Result<Vec<MeetingSpeakerSummary>, sqlx::Error> {
        let rows = sqlx::query_as::<_, MeetingSpeakerSummary>(
            "SELECT t.speaker AS label,
                    s.name AS speaker_name,
                    s.id AS speaker_id,
                    COUNT(*) AS segment_count,
                    MIN(t.audio_start_time) AS first_start_time
             FROM transcripts t
             LEFT JOIN meeting_speakers ms
                    ON ms.meeting_id = t.meeting_id AND ms.label = t.speaker
             LEFT JOIN speakers s ON s.id = ms.speaker_id
             WHERE t.meeting_id = ? AND t.speaker IS NOT NULL
             GROUP BY t.speaker, s.name, s.id
             ORDER BY MIN(t.audio_start_time) IS NULL,
                      MIN(t.audio_start_time) ASC,
                      CAST(substr(t.speaker, 9) AS INTEGER) ASC",
        )
        .bind(meeting_id)
        .fetch_all(pool)
        .await?;

        Ok(rows
            .into_iter()
            .filter(|row| is_speaker_label(&row.label))
            .collect())
    }

    /// Maps a diarization label to the assigned person's name for one meeting.
    /// Accepts any SQLx executor so callers can reuse their own transaction.
    pub async fn speaker_name_map<'e, E>(
        executor: E,
        meeting_id: &str,
    ) -> Result<HashMap<String, String>, sqlx::Error>
    where
        E: Executor<'e, Database = Sqlite>,
    {
        let rows: Vec<(String, String)> = sqlx::query_as(
            "SELECT ms.label, s.name
             FROM meeting_speakers ms
             JOIN speakers s ON s.id = ms.speaker_id
             WHERE ms.meeting_id = ?",
        )
        .bind(meeting_id)
        .fetch_all(executor)
        .await?;

        Ok(rows.into_iter().collect())
    }
}

#[cfg(test)]
mod tests {
    use super::SpeakersRepository;
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

    async fn insert_meeting(pool: &SqlitePool, meeting_id: &str) {
        sqlx::query("INSERT INTO meetings (id, title, created_at, updated_at) VALUES (?, ?, ?, ?)")
            .bind(meeting_id)
            .bind("Meeting")
            .bind("2024-01-01T00:00:00Z")
            .bind("2024-01-01T00:00:00Z")
            .execute(pool)
            .await
            .unwrap();
    }

    async fn insert_transcript(
        pool: &SqlitePool,
        id: &str,
        meeting_id: &str,
        speaker: Option<&str>,
        start: Option<f64>,
    ) {
        sqlx::query(
            "INSERT INTO transcripts (id, meeting_id, transcript, timestamp, speaker, audio_start_time)
             VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind(id)
        .bind(meeting_id)
        .bind("Text")
        .bind("2024-01-01T00:00:00Z")
        .bind(speaker)
        .bind(start)
        .execute(pool)
        .await
        .unwrap();
    }

    async fn assign_name(pool: &SqlitePool, meeting_id: &str, label: &str, name: &str) {
        let person_id = format!("person-{}", name.to_lowercase());
        sqlx::query("INSERT INTO speakers (id, name, created_at, updated_at) VALUES (?, ?, ?, ?)")
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
    async fn a_name_resolves_by_its_label_for_the_same_meeting() {
        let pool = migrated_pool().await;
        insert_meeting(&pool, "meeting-1").await;
        insert_meeting(&pool, "meeting-2").await;
        insert_transcript(&pool, "t-0", "meeting-1", Some("speaker_0"), Some(0.0)).await;
        insert_transcript(&pool, "t-1", "meeting-1", Some("speaker_1"), Some(1.0)).await;
        insert_transcript(&pool, "t-2", "meeting-2", Some("speaker_0"), Some(2.0)).await;
        assign_name(&pool, "meeting-1", "speaker_0", "Alex").await;

        let names = SpeakersRepository::speaker_name_map(&pool, "meeting-1")
            .await
            .unwrap();

        assert_eq!(names.get("speaker_0").map(String::as_str), Some("Alex"));
        assert_eq!(names.get("speaker_1"), None, "an unnamed label has no entry");

        let other = SpeakersRepository::speaker_name_map(&pool, "meeting-2")
            .await
            .unwrap();
        assert!(other.is_empty(), "names belong to one meeting");
    }

    #[tokio::test]
    async fn the_speaker_list_counts_segments_and_carries_assigned_names() {
        let pool = migrated_pool().await;
        insert_meeting(&pool, "meeting-1").await;
        insert_transcript(&pool, "t-0", "meeting-1", Some("speaker_0"), Some(0.0)).await;
        insert_transcript(&pool, "t-1", "meeting-1", Some("speaker_0"), Some(2.0)).await;
        insert_transcript(&pool, "t-2", "meeting-1", Some("speaker_1"), Some(4.0)).await;
        assign_name(&pool, "meeting-1", "speaker_0", "Alex").await;

        let speakers = SpeakersRepository::list_meeting_speakers(&pool, "meeting-1")
            .await
            .unwrap();

        assert_eq!(speakers.len(), 2);
        assert_eq!(speakers[0].label, "speaker_0");
        assert_eq!(speakers[0].speaker_name.as_deref(), Some("Alex"));
        assert_eq!(speakers[0].segment_count, 2);
        assert_eq!(speakers[0].first_start_time, Some(0.0));
        assert_eq!(speakers[1].label, "speaker_1");
        assert_eq!(speakers[1].speaker_name, None);
        assert_eq!(speakers[1].segment_count, 1);
        assert_eq!(speakers[1].first_start_time, Some(4.0));
    }

    #[tokio::test]
    async fn only_speaker_labels_appear_in_the_list() {
        let pool = migrated_pool().await;
        insert_meeting(&pool, "meeting-1").await;
        insert_transcript(&pool, "t-0", "meeting-1", Some("speaker_0"), Some(0.0)).await;
        insert_transcript(&pool, "t-1", "meeting-1", Some("mic"), Some(1.0)).await;
        insert_transcript(&pool, "t-2", "meeting-1", Some("system"), Some(2.0)).await;
        insert_transcript(&pool, "t-3", "meeting-1", Some("Alex"), Some(3.0)).await;
        insert_transcript(&pool, "t-4", "meeting-1", Some("speaker_"), Some(4.0)).await;
        insert_transcript(&pool, "t-5", "meeting-1", Some("speaker_x"), Some(5.0)).await;

        let speakers = SpeakersRepository::list_meeting_speakers(&pool, "meeting-1")
            .await
            .unwrap();

        let labels: Vec<&str> = speakers.iter().map(|s| s.label.as_str()).collect();
        assert_eq!(labels, ["speaker_0"], "only speaker_N labels are speakers");
    }

    #[tokio::test]
    async fn a_segment_without_a_label_never_appears_in_the_list() {
        let pool = migrated_pool().await;
        insert_meeting(&pool, "meeting-1").await;
        insert_transcript(&pool, "t-0", "meeting-1", None, Some(0.0)).await;
        insert_transcript(&pool, "t-1", "meeting-1", Some("speaker_1"), None).await;

        let speakers = SpeakersRepository::list_meeting_speakers(&pool, "meeting-1")
            .await
            .unwrap();

        assert_eq!(speakers.len(), 1, "the unlabelled segment is not a speaker");
        assert_eq!(speakers[0].label, "speaker_1");
        assert_eq!(speakers[0].first_start_time, None);
    }

    #[tokio::test]
    async fn speakers_are_ordered_by_first_start_then_label_with_nulls_last() {
        let pool = migrated_pool().await;
        insert_meeting(&pool, "meeting-1").await;
        insert_transcript(&pool, "t-0", "meeting-1", Some("speaker_10"), Some(1.0)).await;
        insert_transcript(&pool, "t-1", "meeting-1", Some("speaker_2"), Some(1.0)).await;
        insert_transcript(&pool, "t-2", "meeting-1", Some("speaker_1"), Some(5.0)).await;
        insert_transcript(&pool, "t-3", "meeting-1", Some("speaker_0"), None).await;

        let speakers = SpeakersRepository::list_meeting_speakers(&pool, "meeting-1")
            .await
            .unwrap();

        let labels: Vec<&str> = speakers.iter().map(|s| s.label.as_str()).collect();
        assert_eq!(
            labels,
            ["speaker_2", "speaker_10", "speaker_1", "speaker_0"],
            "time first, then numeric label, with missing times last"
        );
    }
}
