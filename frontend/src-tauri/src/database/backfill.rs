//! Filling the meeting's own date and length for rows that predate the columns.
//!
//! See ADR-0005 (the meeting date is separate from `created_at`) and ticket 04:
//! the pass writes only into empty fields (a half-filled row is topped up, never
//! overwritten), is idempotent, and never invents a value — a meeting with no
//! folder and no timed transcripts keeps `NULL` where nothing honest is known.
//! Approximate values for old meetings are accepted deliberately.
//!
//! It runs detached from application startup (`spawn_backfill_at_startup`), so a
//! slow or broken recording folder cannot hold the window back.

use chrono::{DateTime, Utc};
use log::info;
use sqlx::SqlitePool;

use super::meeting_facts::read_folder_meeting_facts;

/// What one pass filled, so the log line can stay aggregate and content-free.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(crate) struct BackfillReport {
    pub dates_from_recording_folder: u64,
    pub durations_from_recording_folder: u64,
    pub dates_from_created_at: u64,
    pub durations_from_transcripts: u64,
    pub left_without_date: i64,
    pub left_without_duration: i64,
}

/// Fills the meeting date and duration of every row that lacks them.
///
/// Idempotent: a second run finds nothing empty and changes nothing.
pub(crate) async fn backfill_meeting_facts(
    pool: &SqlitePool,
) -> Result<BackfillReport, sqlx::Error> {
    let mut report = BackfillReport::default();

    // Step 1: the recording folder knows when the meeting really started and how
    // long it lasted. A folder that is missing, unreadable or broken simply
    // contributes nothing; the later steps still fill what they can.
    //
    // Reading files is blocking I/O, so it happens off the async worker. One bad
    // folder costs its own two fields, never the pass.
    let pending: Vec<(String, Option<String>, Option<String>, Option<f64>)> = sqlx::query_as(
        "SELECT id, folder_path, recorded_at, duration_seconds FROM meetings
         WHERE recorded_at IS NULL OR duration_seconds IS NULL",
    )
    .fetch_all(pool)
    .await?;

    let folders: Vec<Option<String>> = pending
        .iter()
        .map(|(_, folder_path, _, _)| folder_path.clone())
        .collect();
    let folder_facts = tokio::task::spawn_blocking(move || {
        folders
            .into_iter()
            .map(|folder_path| read_folder_meeting_facts(folder_path.as_deref()))
            .collect::<Vec<_>>()
    })
    .await;

    match folder_facts {
        Ok(folder_facts) => {
            for ((meeting_id, _, recorded_at, duration_seconds), facts) in
                pending.into_iter().zip(folder_facts)
            {
                if recorded_at.is_none() {
                    if let Some(moment) = facts
                        .recorded_at_ms
                        .and_then(DateTime::<Utc>::from_timestamp_millis)
                    {
                        report.dates_from_recording_folder +=
                            fill_date(pool, &meeting_id, moment).await?;
                    }
                }

                if duration_seconds.is_none() {
                    if let Some(seconds) = facts.duration_seconds {
                        report.durations_from_recording_folder +=
                            fill_duration(pool, &meeting_id, seconds).await?;
                    }
                }
            }
        }
        Err(error) => log::warn!(
            "Meeting facts backfill: recording folders could not be read: {}",
            error
        ),
    }

    // Step 2: what the folder could not tell, the row itself can: the meeting is
    // at least as old as the row, so the row's moment is the honest fallback.
    // `created_at` is copied verbatim, which also guarantees the same text shape
    // ticket 05 relies on for lexicographic ordering.
    let filled =
        sqlx::query("UPDATE meetings SET recorded_at = created_at WHERE recorded_at IS NULL")
            .execute(pool)
            .await?;
    report.dates_from_created_at = filled.rows_affected();

    // Step 3: transcripts are the last place a length can come from. The end of
    // the furthest segment is the best available approximation of the meeting's
    // end. Step 4 falls out of the same query: a meeting with no timed segments
    // has no row here, so its length stays NULL. A degenerate zero is "unknown"
    // too, exactly as `meeting_facts` treats a zero folder duration.
    // The rule lives in Rust at `meeting_facts::usable_duration_seconds`; SQL
    // cannot call it, so `HAVING ... > 0` restates the same predicate here.
    let segment_ends: Vec<(String, f64)> = sqlx::query_as(
        "SELECT meeting_id, MAX(audio_end_time) FROM transcripts
         WHERE audio_end_time IS NOT NULL
         GROUP BY meeting_id
         HAVING MAX(audio_end_time) > 0",
    )
    .fetch_all(pool)
    .await?;

    for (meeting_id, seconds) in segment_ends {
        report.durations_from_transcripts += fill_duration(pool, &meeting_id, seconds).await?;
    }

    let (left_without_date, left_without_duration): (i64, i64) = sqlx::query_as(
        "SELECT
             COALESCE(SUM(recorded_at IS NULL), 0),
             COALESCE(SUM(duration_seconds IS NULL), 0)
         FROM meetings",
    )
    .fetch_one(pool)
    .await?;
    report.left_without_date = left_without_date;
    report.left_without_duration = left_without_duration;

    Ok(report)
}

impl BackfillReport {
    /// True when the pass found no empty field to write into — what a second run
    /// must report, and the cheapest proof of idempotence in the log.
    pub(crate) fn filled_nothing(&self) -> bool {
        self.dates_from_recording_folder == 0
            && self.durations_from_recording_folder == 0
            && self.dates_from_created_at == 0
            && self.durations_from_transcripts == 0
    }
}

/// Writes the meeting date only if it is still empty; returns rows changed.
///
/// The value is bound as a `DateTime<Utc>` so sqlx encodes it exactly the way it
/// encodes `created_at` (RFC3339 UTC, `SecondsFormat::AutoSi`). Ticket 05 orders
/// by `COALESCE(recorded_at, created_at)`, and SQLite compares that text
/// lexicographically — one bare `YYYY-MM-DD` would sort the row wrongly.
async fn fill_date(
    pool: &SqlitePool,
    meeting_id: &str,
    moment: DateTime<Utc>,
) -> Result<u64, sqlx::Error> {
    let changed =
        sqlx::query("UPDATE meetings SET recorded_at = ? WHERE id = ? AND recorded_at IS NULL")
            .bind(moment)
            .bind(meeting_id)
            .execute(pool)
            .await?;
    Ok(changed.rows_affected())
}

/// Writes the duration only if it is still empty; returns rows changed.
async fn fill_duration(
    pool: &SqlitePool,
    meeting_id: &str,
    seconds: f64,
) -> Result<u64, sqlx::Error> {
    let changed = sqlx::query(
        "UPDATE meetings SET duration_seconds = ? WHERE id = ? AND duration_seconds IS NULL",
    )
    .bind(seconds)
    .bind(meeting_id)
    .execute(pool)
    .await?;
    Ok(changed.rows_affected())
}

/// Runs the pass in the background, so startup never waits for it.
pub(crate) fn spawn_backfill_at_startup(pool: SqlitePool) {
    tauri::async_runtime::spawn(async move {
        match backfill_meeting_facts(&pool).await {
            Ok(report) if report.filled_nothing() => {
                info!("Meeting facts backfill: no empty meeting date or duration to fill");
            }
            Ok(report) => info!(
                "Meeting facts backfill: {} dates and {} durations from recording folders, \
                 {} dates from created_at, {} durations from transcripts, \
                 {} meetings still without a date, {} still without a duration",
                report.dates_from_recording_folder,
                report.durations_from_recording_folder,
                report.dates_from_created_at,
                report.durations_from_transcripts,
                report.left_without_date,
                report.left_without_duration,
            ),
            Err(error) => log::warn!("Meeting facts backfill failed: {}", error),
        }
    });
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use sqlx::sqlite::SqlitePoolOptions;
    use sqlx::{Row, SqlitePool};

    use super::backfill_meeting_facts;

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

    /// A meeting row as it looked before this feature existed: only the audit
    /// columns, no meeting date, no duration.
    async fn seed_meeting(
        pool: &SqlitePool,
        id: &str,
        created_at: &str,
        folder_path: Option<&Path>,
    ) {
        sqlx::query("INSERT INTO meetings (id, title, created_at, updated_at, folder_path) VALUES (?, ?, ?, ?, ?)")
            .bind(id)
            .bind(format!("Meeting {id}"))
            .bind(created_at)
            .bind(created_at)
            .bind(folder_path.map(|path| path.to_string_lossy().to_string()))
            .execute(pool)
            .await
            .unwrap();
    }

    async fn seed_transcript(
        pool: &SqlitePool,
        id: &str,
        meeting_id: &str,
        audio_end_time: Option<f64>,
    ) {
        sqlx::query(
            "INSERT INTO transcripts (id, meeting_id, transcript, timestamp, audio_end_time)
             VALUES (?, ?, ?, ?, ?)",
        )
        .bind(id)
        .bind(meeting_id)
        .bind("Text")
        .bind("2024-01-01T00:00:00Z")
        .bind(audio_end_time)
        .execute(pool)
        .await
        .unwrap();
    }

    /// The three columns the pass is allowed to touch, in a comparable form.
    async fn meeting_row(pool: &SqlitePool, id: &str) -> (String, Option<String>, Option<f64>) {
        let row = sqlx::query(
            "SELECT created_at, recorded_at, duration_seconds FROM meetings WHERE id = ?",
        )
        .bind(id)
        .fetch_one(pool)
        .await
        .unwrap();
        (
            row.get::<String, _>("created_at"),
            row.get::<Option<String>, _>("recorded_at"),
            row.get::<Option<f64>, _>("duration_seconds"),
        )
    }

    /// A recording folder whose `metadata.json` says when the meeting started
    /// and how long it lasted.
    fn recording_folder(started_at: &str, duration_seconds: Option<f64>) -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        let duration = match duration_seconds {
            Some(seconds) => format!("{seconds}"),
            None => "null".to_string(),
        };
        std::fs::write(
            dir.path().join("metadata.json"),
            format!(r#"{{"created_at": "{started_at}", "duration_seconds": {duration}}}"#),
        )
        .unwrap();
        dir
    }

    #[tokio::test]
    async fn fills_the_date_and_length_from_the_recording_folder() {
        let pool = migrated_pool().await;
        let folder = recording_folder("2026-11-18T14:32:05+00:00", Some(4_975.5));
        seed_meeting(
            &pool,
            "recorded",
            "2024-05-06T07:08:09Z",
            Some(folder.path()),
        )
        .await;

        backfill_meeting_facts(&pool).await.unwrap();

        let (created_at, recorded_at, duration) = meeting_row(&pool, "recorded").await;
        assert_eq!(
            recorded_at.as_deref(),
            Some("2026-11-18T14:32:05+00:00"),
            "the date is when the recording started, in the same RFC3339 UTC shape created_at uses"
        );
        assert_eq!(duration, Some(4_975.5), "the length comes from the file");
        assert_eq!(
            created_at, "2024-05-06T07:08:09Z",
            "created_at stays the audit field"
        );
    }

    #[tokio::test]
    async fn a_meeting_with_no_folder_and_no_transcripts_gets_the_row_moment_but_no_length() {
        let pool = migrated_pool().await;
        seed_meeting(&pool, "bare", "2024-05-06T07:08:09Z", None).await;

        backfill_meeting_facts(&pool).await.unwrap();

        let (created_at, recorded_at, duration) = meeting_row(&pool, "bare").await;
        assert_eq!(
            recorded_at.as_deref(),
            Some(created_at.as_str()),
            "the only honest date left is when the row appeared, in the same text shape"
        );
        assert_eq!(
            duration, None,
            "nothing knows the length, so it stays NULL — never zero"
        );
    }

    #[tokio::test]
    async fn the_length_falls_back_to_the_last_transcript_segment() {
        let pool = migrated_pool().await;
        seed_meeting(&pool, "transcribed", "2024-05-06T07:08:09Z", None).await;
        seed_transcript(&pool, "t-1", "transcribed", Some(10.0)).await;
        seed_transcript(&pool, "t-2", "transcribed", Some(95.5)).await;
        seed_transcript(&pool, "t-3", "transcribed", None).await;

        backfill_meeting_facts(&pool).await.unwrap();

        let (_, recorded_at, duration) = meeting_row(&pool, "transcribed").await;
        assert_eq!(
            duration,
            Some(95.5),
            "the furthest segment end is the length of the meeting"
        );
        assert_eq!(
            recorded_at.as_deref(),
            Some("2024-05-06T07:08:09Z"),
            "a meeting without a folder still gets a date"
        );
    }

    #[tokio::test]
    async fn transcripts_without_timing_leave_the_length_unknown() {
        let pool = migrated_pool().await;
        seed_meeting(&pool, "untimed", "2024-05-06T07:08:09Z", None).await;
        seed_transcript(&pool, "u-1", "untimed", None).await;

        backfill_meeting_facts(&pool).await.unwrap();

        let (_, _, duration) = meeting_row(&pool, "untimed").await;
        assert_eq!(duration, None, "no segment knows its end, so NULL it is");
    }

    #[tokio::test]
    async fn a_zero_length_transcript_end_is_unknown_not_a_zero_length_meeting() {
        let pool = migrated_pool().await;
        seed_meeting(&pool, "zero", "2024-05-06T07:08:09Z", None).await;
        seed_transcript(&pool, "z-1", "zero", Some(0.0)).await;

        backfill_meeting_facts(&pool).await.unwrap();

        let (_, _, duration) = meeting_row(&pool, "zero").await;
        assert_eq!(
            duration, None,
            "a true zero reads as an untimed row: NULL means unknown, zero would be a lie"
        );
    }

    #[tokio::test]
    async fn a_second_pass_changes_nothing() {
        let pool = migrated_pool().await;
        let folder = recording_folder("2026-11-18T14:32:05+00:00", Some(4_975.5));
        seed_meeting(
            &pool,
            "recorded",
            "2024-05-06T07:08:09Z",
            Some(folder.path()),
        )
        .await;
        seed_meeting(&pool, "bare", "2024-05-07T07:08:09Z", None).await;
        seed_transcript(&pool, "t-1", "bare", Some(95.5)).await;

        backfill_meeting_facts(&pool).await.unwrap();
        let after_first = (
            meeting_row(&pool, "recorded").await,
            meeting_row(&pool, "bare").await,
        );

        let second = backfill_meeting_facts(&pool).await.unwrap();

        assert_eq!(
            (
                meeting_row(&pool, "recorded").await,
                meeting_row(&pool, "bare").await,
            ),
            after_first,
            "the pass is idempotent"
        );
        assert!(
            second.filled_nothing(),
            "a second pass has no empty field left to fill, so it writes nothing"
        );
    }

    #[tokio::test]
    async fn a_broken_metadata_file_does_not_break_the_pass_or_the_other_meetings() {
        let pool = migrated_pool().await;
        let broken = tempfile::tempdir().unwrap();
        std::fs::write(broken.path().join("metadata.json"), "{ this is not json").unwrap();
        let good = recording_folder("2026-11-18T14:32:05+00:00", Some(4_975.5));

        seed_meeting(&pool, "broken", "2024-05-06T07:08:09Z", Some(broken.path())).await;
        seed_meeting(&pool, "good", "2024-05-07T07:08:09Z", Some(good.path())).await;

        backfill_meeting_facts(&pool)
            .await
            .expect("a corrupt folder is skipped, not fatal");

        let (_, broken_date, broken_duration) = meeting_row(&pool, "broken").await;
        assert_eq!(
            broken_date.as_deref(),
            Some("2024-05-06T07:08:09Z"),
            "the unusable folder falls through to the row's own moment"
        );
        assert_eq!(broken_duration, None, "nothing honest to write, so NULL");

        let (_, good_date, good_duration) = meeting_row(&pool, "good").await;
        assert_eq!(good_date.as_deref(), Some("2026-11-18T14:32:05+00:00"));
        assert_eq!(good_duration, Some(4_975.5));
    }

    #[tokio::test]
    async fn the_written_date_sorts_against_created_at_as_one_text_column() {
        let pool = migrated_pool().await;
        let folder = recording_folder("2026-11-18T14:32:05+00:00", Some(4_975.5));
        seed_meeting(
            &pool,
            "from_folder",
            "2024-05-06T07:08:09Z",
            Some(folder.path()),
        )
        .await;
        seed_meeting(&pool, "from_row", "2025-01-02T03:04:05Z", None).await;

        backfill_meeting_facts(&pool).await.unwrap();

        // Ticket 05 orders the list by this expression; the backfill's job is to
        // keep both halves in one comparable text shape.
        let ordered: Vec<String> = sqlx::query_scalar(
            "SELECT id FROM meetings ORDER BY COALESCE(recorded_at, created_at) DESC",
        )
        .fetch_all(&pool)
        .await
        .unwrap();

        assert_eq!(
            ordered,
            ["from_folder", "from_row"],
            "the recorded meeting is the newer one and sorts first"
        );
    }

    #[tokio::test]
    async fn a_row_missing_only_the_length_keeps_its_date_and_gains_only_the_length() {
        let pool = migrated_pool().await;
        let folder = recording_folder("2026-11-18T14:32:05+00:00", Some(4_975.5));
        seed_meeting(&pool, "dated", "2024-05-06T07:08:09Z", Some(folder.path())).await;
        sqlx::query(
            "UPDATE meetings SET recorded_at = '2023-01-02T03:04:05+00:00' WHERE id = 'dated'",
        )
        .execute(&pool)
        .await
        .unwrap();

        backfill_meeting_facts(&pool).await.unwrap();

        let (_, recorded_at, duration) = meeting_row(&pool, "dated").await;
        assert_eq!(
            recorded_at.as_deref(),
            Some("2023-01-02T03:04:05+00:00"),
            "a date the row already has is never overwritten, even by the folder"
        );
        assert_eq!(duration, Some(4_975.5), "only the empty half is filled");
    }

    #[tokio::test]
    async fn a_row_missing_only_the_date_keeps_its_length_and_gains_only_the_date() {
        let pool = migrated_pool().await;
        let folder = recording_folder("2026-11-18T14:32:05+00:00", Some(4_975.5));
        seed_meeting(&pool, "timed", "2024-05-06T07:08:09Z", Some(folder.path())).await;
        sqlx::query("UPDATE meetings SET duration_seconds = 1200.0 WHERE id = 'timed'")
            .execute(&pool)
            .await
            .unwrap();
        seed_transcript(&pool, "t-1", "timed", Some(95.5)).await;

        backfill_meeting_facts(&pool).await.unwrap();

        let (_, recorded_at, duration) = meeting_row(&pool, "timed").await;
        assert_eq!(
            recorded_at.as_deref(),
            Some("2026-11-18T14:32:05+00:00"),
            "the date comes from the folder"
        );
        assert_eq!(
            duration,
            Some(1200.0),
            "a length the row already has is never overwritten by the folder or the transcripts"
        );
    }
}
