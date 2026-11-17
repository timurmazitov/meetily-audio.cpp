// Speaker names: reading the speakers of a meeting and resolving the name for
// a diarization label. `transcripts.speaker` stays the label (`speaker_N`);
// a real name is a separate entity (see `.scratch/speaker-names/spec.md`).

use chrono::Utc;
use sqlx::{
    Connection, Error as SqlxError, Executor, FromRow, Sqlite, SqliteConnection, SqlitePool,
    Transaction,
};
use std::collections::{BTreeMap, HashMap, HashSet};
use uuid::Uuid;

use crate::api::{MeetingSpeaker, SpeakerAssignment, SpeakerNameEntry};

/// Maximum number of characters allowed in a person's name, after trimming.
const MAX_NAME_CHARS: usize = 100;

/// Errors of the speaker-name commands. `Display` messages are user-facing and
/// written in English, like the rest of the UI strings.
#[derive(Debug)]
pub enum SpeakerError {
    /// The label does not look like a diarization label (`speaker_N`).
    InvalidLabel(String),
    /// The name is empty or longer than [`MAX_NAME_CHARS`] characters.
    InvalidName(String),
    /// A label of this meeting is already bound to the same person.
    AlreadyAssigned {
        existing_label: String,
    },
    MeetingNotFound(String),
    TranscriptNotFound(String),
    Database(SqlxError),
}

impl std::fmt::Display for SpeakerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SpeakerError::InvalidLabel(label) => {
                write!(f, "invalid speaker label: {label}")
            }
            SpeakerError::InvalidName(reason) => {
                write!(f, "invalid speaker name: {reason}")
            }
            SpeakerError::AlreadyAssigned { existing_label } => {
                write!(f, "this person is already assigned to {existing_label}")
            }
            SpeakerError::MeetingNotFound(id) => write!(f, "meeting not found: {id}"),
            SpeakerError::TranscriptNotFound(id) => write!(f, "transcript not found: {id}"),
            SpeakerError::Database(error) => write!(f, "{error}"),
        }
    }
}

impl std::error::Error for SpeakerError {}

impl From<SqlxError> for SpeakerError {
    fn from(error: SqlxError) -> Self {
        SpeakerError::Database(error)
    }
}

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

/// How many directory names the speaker panel offers as suggestions.
const TOP_SPEAKER_NAMES_LIMIT: i64 = 8;
/// Page size of the name directory when the caller does not ask for one.
const DEFAULT_DIRECTORY_LIMIT: i64 = 20;
/// Hard cap for the name directory page size.
const MAX_DIRECTORY_LIMIT: i64 = 50;

/// Renders a label as the user sees it: `speaker_0` → `Speaker 1`.
fn display_label(label: &str) -> String {
    match label_number(label) {
        Some(number) => format!("Speaker {}", number + 1),
        None => label.to_string(),
    }
}

/// The `N` of a `speaker_N` label.
fn label_number(label: &str) -> Option<u64> {
    label.strip_prefix("speaker_")?.parse().ok()
}

/// `trim`, then 1..=100 characters counted as Unicode scalar values.
fn validate_name(raw: &str) -> Result<&str, SpeakerError> {
    let name = raw.trim();
    let length = name.chars().count();
    if length == 0 {
        return Err(SpeakerError::InvalidName(
            "a name cannot be empty".to_string(),
        ));
    }
    if length > MAX_NAME_CHARS {
        return Err(SpeakerError::InvalidName(format!(
            "a name cannot be longer than {MAX_NAME_CHARS} characters"
        )));
    }
    Ok(name)
}

/// True when the error is a SQLite uniqueness violation. The message is
/// matched because `sqlx-sqlite` does not expose the constraint name.
fn is_unique_violation(error: &SqlxError) -> bool {
    match error {
        SqlxError::Database(database_error) => database_error
            .message()
            .contains("UNIQUE constraint failed"),
        _ => false,
    }
}

/// Result of reconciling one meeting's name bindings against the speaker
/// labels its transcript actually has.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct ReconcileOutcome {
    /// True when at least one binding existed and none of its labels survived,
    /// so the UI should explain that the names were reset.
    pub names_reset: bool,
    /// Number of `meeting_speakers` rows removed.
    pub removed_bindings: usize,
    /// Number of orphaned `speakers` rows removed with them.
    pub removed_people: usize,
}

/// Aligns the meeting's name bindings with the speaker labels written by the
/// last recount (import, retranscription, post-recording diarization).
///
/// A label that still exists keeps its name even when its segment count
/// changed; a label that disappeared loses its binding, and a person nobody
/// references any more is removed from the directory. When none of the old
/// labels survived, every binding is gone and [`ReconcileOutcome::names_reset`]
/// is true so the caller can tell the user why the names vanished.
///
/// Takes a raw connection so the caller can run it inside the transaction that
/// writes the labels: the recount and the binding cleanup then commit together.
pub async fn reconcile_meeting_speaker_assignments(
    conn: &mut SqliteConnection,
    meeting_id: &str,
) -> Result<ReconcileOutcome, SqlxError> {
    let label_rows: Vec<(String,)> =
        sqlx::query_as("SELECT DISTINCT speaker FROM transcripts WHERE meeting_id = ?")
            .bind(meeting_id)
            .fetch_all(&mut *conn)
            .await?;
    let current: HashSet<String> = label_rows
        .into_iter()
        .map(|(label,)| label)
        .filter(|label| is_speaker_label(label))
        .collect();

    let bound_rows: Vec<(String,)> =
        sqlx::query_as("SELECT label FROM meeting_speakers WHERE meeting_id = ?")
            .bind(meeting_id)
            .fetch_all(&mut *conn)
            .await?;
    let bound_before = bound_rows.len();

    let mut outcome = ReconcileOutcome::default();
    for (label,) in &bound_rows {
        if current.contains(label) {
            continue;
        }
        let result = sqlx::query("DELETE FROM meeting_speakers WHERE meeting_id = ? AND label = ?")
            .bind(meeting_id)
            .bind(label)
            .execute(&mut *conn)
            .await?;
        outcome.removed_bindings += result.rows_affected() as usize;
    }

    if outcome.removed_bindings > 0 {
        let result = sqlx::query(
            "DELETE FROM speakers
             WHERE id NOT IN (SELECT speaker_id FROM meeting_speakers)",
        )
        .execute(&mut *conn)
        .await?;
        outcome.removed_people = result.rows_affected() as usize;

        sqlx::query("UPDATE meetings SET updated_at = ? WHERE id = ?")
            .bind(Utc::now().naive_utc())
            .bind(meeting_id)
            .execute(&mut *conn)
            .await?;

        outcome.names_reset = bound_before > 0 && outcome.removed_bindings == bound_before;
    }

    Ok(outcome)
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

    /// The whole speaker panel of one meeting: one row per label plus the
    /// directory names offered as suggestions. Suggestions are global (not
    /// limited to this meeting) and most-used first.
    pub async fn meeting_speakers(
        pool: &SqlitePool,
        meeting_id: &str,
    ) -> Result<Vec<MeetingSpeaker>, SpeakerError> {
        let rows = Self::list_meeting_speakers(pool, meeting_id).await?;
        let top_names = Self::top_speaker_names(pool).await?;

        Ok(rows
            .into_iter()
            .map(|row| MeetingSpeaker {
                label: row.label,
                speaker_name: row.speaker_name,
                segment_count: row.segment_count,
                first_start_time: row.first_start_time,
                top_speaker_names: top_names.clone(),
            })
            .collect())
    }

    /// Directory names for the panel's autocomplete, most-used first.
    async fn top_speaker_names(pool: &SqlitePool) -> Result<Vec<String>, SpeakerError> {
        let rows: Vec<(String,)> = sqlx::query_as(
            "SELECT s.name
             FROM speakers s
             LEFT JOIN meeting_speakers ms ON ms.speaker_id = s.id
             GROUP BY s.id, s.name
             ORDER BY COUNT(DISTINCT ms.meeting_id) DESC,
                      s.name COLLATE NOCASE ASC
             LIMIT ?",
        )
        .bind(TOP_SPEAKER_NAMES_LIMIT)
        .fetch_all(pool)
        .await?;

        Ok(rows.into_iter().map(|(name,)| name).collect())
    }

    /// Searches the global directory for the panel's autocomplete. The query
    /// is a case-insensitive substring match after trimming (an empty query
    /// lists everything); results are ordered by usage, then by name.
    ///
    /// Filtering happens in Rust so that non-ASCII names (Cyrillic, accented
    /// letters) match case-insensitively, which SQLite's ASCII-only `LIKE`
    /// and `NOCASE` cannot do.
    pub async fn search_names(
        pool: &SqlitePool,
        query: &str,
        limit: i64,
    ) -> Result<Vec<SpeakerNameEntry>, SpeakerError> {
        let limit = if limit <= 0 {
            DEFAULT_DIRECTORY_LIMIT
        } else {
            limit.min(MAX_DIRECTORY_LIMIT)
        };
        let needle = query.trim().to_lowercase();

        let rows: Vec<(String, String, i64)> = sqlx::query_as(
            "SELECT s.id, s.name, COUNT(DISTINCT ms.meeting_id) AS meeting_count
             FROM speakers s
             LEFT JOIN meeting_speakers ms ON ms.speaker_id = s.id
             GROUP BY s.id, s.name
             ORDER BY meeting_count DESC, s.name COLLATE NOCASE ASC",
        )
        .fetch_all(pool)
        .await?;

        Ok(rows
            .into_iter()
            .map(|(id, name, meeting_count)| SpeakerNameEntry {
                id,
                name,
                meeting_count,
            })
            .filter(|row| needle.is_empty() || row.name.to_lowercase().contains(&needle))
            .take(limit as usize)
            .collect())
    }

    /// Applies a batch of panel assignments atomically: either every assignment
    /// applies or none does. `name: None` removes the assignment; a non-empty
    /// name binds the label to an existing person (case-insensitive) or creates
    /// a new directory row. Unreferenced directory rows are removed afterwards.
    ///
    /// Returns the refreshed speaker panel.
    pub async fn assign(
        pool: &SqlitePool,
        meeting_id: &str,
        assignments: &[SpeakerAssignment],
    ) -> Result<Vec<MeetingSpeaker>, SpeakerError> {
        if meeting_id.trim().is_empty() {
            return Err(SpeakerError::MeetingNotFound(meeting_id.to_string()));
        }

        // Validate the whole batch before touching the database so a rejected
        // item can never leave earlier items behind.
        let mut prepared: Vec<(&str, Option<&str>)> = Vec::with_capacity(assignments.len());
        for assignment in assignments {
            if !is_speaker_label(&assignment.label) {
                return Err(SpeakerError::InvalidLabel(assignment.label.clone()));
            }
            let name = match assignment.name.as_deref() {
                Some(raw) => Some(validate_name(raw)?),
                None => None,
            };
            prepared.push((assignment.label.as_str(), name));
        }

        let mut conn = pool.acquire().await?;
        let mut transaction = conn.begin().await?;

        let meeting_exists: Option<(i64,)> = sqlx::query_as("SELECT 1 FROM meetings WHERE id = ?")
            .bind(meeting_id)
            .fetch_optional(&mut *transaction)
            .await?;
        if meeting_exists.is_none() {
            transaction.rollback().await?;
            return Err(SpeakerError::MeetingNotFound(meeting_id.to_string()));
        }

        let now = Utc::now().naive_utc();

        // Bindings before the batch, so the conflict check can tell a swap
        // (allowed) from assigning two labels to one person (rejected).
        let current: Vec<(String, String)> =
            sqlx::query_as("SELECT label, speaker_id FROM meeting_speakers WHERE meeting_id = ?")
                .bind(meeting_id)
                .fetch_all(&mut *transaction)
                .await?;
        let current: BTreeMap<String, String> = current.into_iter().collect();

        // Resolve every label of the batch to the person it should end up with.
        let mut desired = current.clone();
        for (label, name) in &prepared {
            match name {
                None => {
                    desired.remove(*label);
                }
                Some(name) => {
                    let speaker_id =
                        Self::find_or_create_person(&mut transaction, name, now).await?;
                    desired.insert(label.to_string(), speaker_id);
                }
            }
        }

        // A person can hold only one label per meeting. Checking the resulting
        // state (instead of each step) lets one batch swap two people.
        let mut bound: HashMap<&str, &str> = HashMap::new();
        let mut labels: Vec<&String> = desired.keys().collect();
        labels.sort_by_key(|label| display_label(label));
        for label in labels {
            let speaker_id = desired.get(label.as_str()).expect("iterating over keys");
            if let Some(other) = bound.insert(speaker_id.as_str(), label.as_str()) {
                // Report the label that was already bound before the batch when
                // there is one, so the message names the person's old speaker.
                let existing_label = match (
                    current.contains_key(other),
                    current.contains_key(label.as_str()),
                ) {
                    (true, false) => other,
                    (false, true) => label.as_str(),
                    _ => {
                        if label_number(other) <= label_number(label.as_str()) {
                            other
                        } else {
                            label.as_str()
                        }
                    }
                };
                transaction.rollback().await?;
                return Err(SpeakerError::AlreadyAssigned {
                    existing_label: display_label(existing_label),
                });
            }
        }

        // Apply only the differences: re-saving an unchanged panel is a no-op,
        // so "Done" does not bump the meeting timestamp needlessly.
        let mut changed = false;
        for label in current.keys() {
            let keep = desired
                .get(label)
                .is_some_and(|speaker_id| current.get(label) == Some(speaker_id));
            if !keep {
                let result =
                    sqlx::query("DELETE FROM meeting_speakers WHERE meeting_id = ? AND label = ?")
                        .bind(meeting_id)
                        .bind(label)
                        .execute(&mut *transaction)
                        .await?;
                changed |= result.rows_affected() > 0;
            }
        }

        // Insert after the deletions so a batch can swap two people without
        // tripping the per-meeting unique indexes halfway through.
        for (label, speaker_id) in &desired {
            if current.get(label) == Some(speaker_id) {
                continue;
            }
            let result = sqlx::query(
                "INSERT INTO meeting_speakers
                     (id, meeting_id, label, speaker_id, created_at, updated_at)
                 VALUES (?, ?, ?, ?, ?, ?)",
            )
            .bind(format!("meeting-speaker-{}", Uuid::new_v4()))
            .bind(meeting_id)
            .bind(label)
            .bind(speaker_id)
            .bind(now)
            .bind(now)
            .execute(&mut *transaction)
            .await;
            Self::finish_binding_write(result, &mut transaction, meeting_id, label, speaker_id)
                .await?;
            changed = true;
        }

        // Directory rows nobody references any more are garbage: names are
        // immutable and re-created rather than renamed.
        sqlx::query(
            "DELETE FROM speakers
             WHERE id NOT IN (SELECT speaker_id FROM meeting_speakers)",
        )
        .execute(&mut *transaction)
        .await?;

        if changed {
            sqlx::query("UPDATE meetings SET updated_at = ? WHERE id = ?")
                .bind(now)
                .bind(meeting_id)
                .execute(&mut *transaction)
                .await?;
        }

        transaction.commit().await?;
        drop(conn);

        Self::meeting_speakers(pool, meeting_id).await
    }

    /// Returns the directory id for `name`, creating the person if the
    /// directory does not have it. Matching is case-insensitive and the first
    /// spelling wins (`Alex` then `alex` → one person shown as `Alex`).
    ///
    /// Comparison happens in Rust because SQLite's `NOCASE` only folds ASCII,
    /// which would let `Мария` and `мария` become two people.
    async fn find_or_create_person(
        transaction: &mut SqliteConnection,
        name: &str,
        now: chrono::NaiveDateTime,
    ) -> Result<String, SpeakerError> {
        let existing: Vec<(String, String)> = sqlx::query_as("SELECT id, name FROM speakers")
            .fetch_all(&mut *transaction)
            .await?;
        let folded = name.to_lowercase();
        if let Some((id, _)) = existing
            .into_iter()
            .find(|(_, stored)| stored.to_lowercase() == folded)
        {
            return Ok(id);
        }

        let id = format!("person-{}", Uuid::new_v4());
        sqlx::query("INSERT INTO speakers (id, name, created_at, updated_at) VALUES (?, ?, ?, ?)")
            .bind(&id)
            .bind(name)
            .bind(now)
            .bind(now)
            .execute(&mut *transaction)
            .await?;
        Ok(id)
    }

    /// Maps a failed binding write to the user-facing error. Uniqueness
    /// violations are reported as "already assigned" because they can only come
    /// from a concurrent writer racing the batch's own conflict check.
    async fn finish_binding_write(
        result: Result<sqlx::sqlite::SqliteQueryResult, SqlxError>,
        transaction: &mut Transaction<'_, Sqlite>,
        meeting_id: &str,
        label: &str,
        speaker_id: &str,
    ) -> Result<(), SpeakerError> {
        match result {
            Ok(_) => Ok(()),
            Err(error) if is_unique_violation(&error) => {
                let existing: Option<(String,)> = sqlx::query_as(
                    "SELECT label FROM meeting_speakers
                     WHERE meeting_id = ? AND speaker_id = ? AND label <> ?",
                )
                .bind(meeting_id)
                .bind(speaker_id)
                .bind(label)
                .fetch_optional(&mut **transaction)
                .await?;
                Err(SpeakerError::AlreadyAssigned {
                    existing_label: existing
                        .map(|(label,)| display_label(&label))
                        .unwrap_or_else(|| display_label(label)),
                })
            }
            Err(error) => Err(SpeakerError::Database(error)),
        }
    }

    /// Sets or clears the speaker label of a single transcript segment. The
    /// meeting's bindings are untouched; only this row's attribution changes.
    pub async fn set_transcript_speaker(
        pool: &SqlitePool,
        transcript_id: &str,
        label: Option<&str>,
    ) -> Result<(), SpeakerError> {
        if let Some(label) = label {
            if !is_speaker_label(label) {
                return Err(SpeakerError::InvalidLabel(label.to_string()));
            }
        }

        let mut conn = pool.acquire().await?;
        let mut transaction = conn.begin().await?;

        let row: Option<(String, Option<String>)> =
            sqlx::query_as("SELECT meeting_id, speaker FROM transcripts WHERE id = ?")
                .bind(transcript_id)
                .fetch_optional(&mut *transaction)
                .await?;
        let Some((meeting_id, current_label)) = row else {
            transaction.rollback().await?;
            return Err(SpeakerError::TranscriptNotFound(transcript_id.to_string()));
        };

        if current_label.as_deref() == label {
            transaction.rollback().await?;
            return Ok(());
        }

        sqlx::query("UPDATE transcripts SET speaker = ? WHERE id = ?")
            .bind(label)
            .bind(transcript_id)
            .execute(&mut *transaction)
            .await?;

        // A per-segment attribution is a name-level change of the meeting, so
        // it bumps the meeting timestamp like a rename does.
        sqlx::query("UPDATE meetings SET updated_at = ? WHERE id = ?")
            .bind(Utc::now().naive_utc())
            .bind(&meeting_id)
            .execute(&mut *transaction)
            .await?;

        transaction.commit().await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::{
        reconcile_meeting_speaker_assignments, ReconcileOutcome, SpeakerError, SpeakersRepository,
    };
    use crate::api::SpeakerAssignment;
    use sqlx::sqlite::SqlitePoolOptions;
    use sqlx::{Connection, Row, SqlitePool};

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
        assert_eq!(
            names.get("speaker_1"),
            None,
            "an unnamed label has no entry"
        );

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

    async fn speaker_rows(pool: &SqlitePool) -> Vec<(String, String)> {
        sqlx::query_as("SELECT id, name FROM speakers ORDER BY name")
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

    async fn updated_at(pool: &SqlitePool, meeting_id: &str) -> String {
        sqlx::query("SELECT updated_at FROM meetings WHERE id = ?")
            .bind(meeting_id)
            .fetch_one(pool)
            .await
            .unwrap()
            .get("updated_at")
    }

    fn assignment(label: &str, name: Option<&str>) -> SpeakerAssignment {
        SpeakerAssignment {
            label: label.to_string(),
            name: name.map(str::to_string),
        }
    }

    #[tokio::test]
    async fn a_new_name_creates_a_directory_row_and_a_binding() {
        let pool = migrated_pool().await;
        insert_meeting(&pool, "meeting-1").await;
        insert_transcript(&pool, "t-0", "meeting-1", Some("speaker_0"), Some(0.0)).await;

        let speakers = SpeakersRepository::assign(
            &pool,
            "meeting-1",
            &[assignment("speaker_0", Some("Alex"))],
        )
        .await
        .unwrap();

        assert_eq!(speaker_rows(&pool).await.len(), 1, "one directory row");
        assert_eq!(
            speaker_rows(&pool).await[0].1,
            "Alex",
            "the typed name is stored"
        );
        assert_eq!(
            bindings(&pool, "meeting-1").await,
            [("speaker_0".to_string(), "Alex".to_string())]
        );
        assert_eq!(speakers.len(), 1);
        assert_eq!(speakers[0].speaker_name.as_deref(), Some("Alex"));
        assert_eq!(speakers[0].segment_count, 1);
    }

    #[tokio::test]
    async fn assigning_the_same_name_in_another_case_reuses_the_existing_row() {
        let pool = migrated_pool().await;
        insert_meeting(&pool, "meeting-1").await;
        insert_transcript(&pool, "t-0", "meeting-1", Some("speaker_0"), Some(0.0)).await;

        SpeakersRepository::assign(&pool, "meeting-1", &[assignment("speaker_0", Some("Alex"))])
            .await
            .unwrap();
        SpeakersRepository::assign(&pool, "meeting-1", &[assignment("speaker_0", Some("alex"))])
            .await
            .unwrap();

        let rows = speaker_rows(&pool).await;
        assert_eq!(rows.len(), 1, "case-insensitive match reuses the row");
        assert_eq!(rows[0].1, "Alex", "the first spelling is kept");
        assert_eq!(
            bindings(&pool, "meeting-1").await,
            [("speaker_0".to_string(), "Alex".to_string())],
            "the second spelling rebinds the same person to the same label"
        );
    }

    #[tokio::test]
    async fn non_ascii_names_fold_case_too() {
        let pool = migrated_pool().await;
        insert_meeting(&pool, "meeting-1").await;
        insert_meeting(&pool, "meeting-2").await;
        insert_transcript(&pool, "t-0", "meeting-1", Some("speaker_0"), Some(0.0)).await;
        insert_transcript(&pool, "t-1", "meeting-2", Some("speaker_0"), Some(0.0)).await;

        SpeakersRepository::assign(
            &pool,
            "meeting-1",
            &[assignment("speaker_0", Some("Мария"))],
        )
        .await
        .unwrap();
        SpeakersRepository::assign(
            &pool,
            "meeting-2",
            &[assignment("speaker_0", Some("мария"))],
        )
        .await
        .unwrap();

        let rows = speaker_rows(&pool).await;
        assert_eq!(rows.len(), 1, "one person, not two spellings of one name");
        assert_eq!(rows[0].1, "Мария");
    }

    #[tokio::test]
    async fn names_are_trimmed_before_matching_and_storing() {
        let pool = migrated_pool().await;
        insert_meeting(&pool, "meeting-1").await;
        insert_meeting(&pool, "meeting-2").await;
        insert_transcript(&pool, "t-0", "meeting-1", Some("speaker_0"), Some(0.0)).await;
        insert_transcript(&pool, "t-1", "meeting-2", Some("speaker_0"), Some(0.0)).await;

        SpeakersRepository::assign(
            &pool,
            "meeting-1",
            &[assignment("speaker_0", Some(" Alex "))],
        )
        .await
        .unwrap();
        SpeakersRepository::assign(&pool, "meeting-2", &[assignment("speaker_0", Some("Alex"))])
            .await
            .unwrap();

        assert_eq!(
            speaker_rows(&pool).await,
            [(speaker_rows(&pool).await[0].0.clone(), "Alex".to_string())],
            "the outer spaces are trimmed and the row is reused"
        );
    }

    #[tokio::test]
    async fn inner_spaces_of_a_name_are_kept() {
        let pool = migrated_pool().await;
        insert_meeting(&pool, "meeting-1").await;
        insert_transcript(&pool, "t-0", "meeting-1", Some("speaker_0"), Some(0.0)).await;

        SpeakersRepository::assign(
            &pool,
            "meeting-1",
            &[assignment("speaker_0", Some("  Mary  Jane "))],
        )
        .await
        .unwrap();

        assert_eq!(speaker_rows(&pool).await[0].1, "Mary  Jane");
    }

    #[tokio::test]
    async fn the_same_name_in_another_case_is_one_person_across_meetings() {
        let pool = migrated_pool().await;
        insert_meeting(&pool, "meeting-1").await;
        insert_meeting(&pool, "meeting-2").await;
        insert_transcript(&pool, "t-0", "meeting-1", Some("speaker_0"), Some(0.0)).await;
        insert_transcript(&pool, "t-1", "meeting-2", Some("speaker_0"), Some(0.0)).await;

        SpeakersRepository::assign(&pool, "meeting-1", &[assignment("speaker_0", Some("Alex"))])
            .await
            .unwrap();
        SpeakersRepository::assign(&pool, "meeting-2", &[assignment("speaker_0", Some("alex"))])
            .await
            .unwrap();

        let rows = speaker_rows(&pool).await;
        assert_eq!(rows.len(), 1, "one person across meetings");
        assert_eq!(rows[0].1, "Alex", "the first spelling is kept");
        assert_eq!(
            bindings(&pool, "meeting-2").await,
            [("speaker_0".to_string(), "Alex".to_string())]
        );
    }

    #[tokio::test]
    async fn removing_a_name_deletes_the_binding_and_then_the_orphan() {
        let pool = migrated_pool().await;
        insert_meeting(&pool, "meeting-1").await;
        insert_transcript(&pool, "t-0", "meeting-1", Some("speaker_0"), Some(0.0)).await;
        SpeakersRepository::assign(&pool, "meeting-1", &[assignment("speaker_0", Some("Alex"))])
            .await
            .unwrap();

        let speakers =
            SpeakersRepository::assign(&pool, "meeting-1", &[assignment("speaker_0", None)])
                .await
                .unwrap();

        assert!(bindings(&pool, "meeting-1").await.is_empty());
        assert!(
            speaker_rows(&pool).await.is_empty(),
            "the unreferenced person is deleted"
        );
        assert_eq!(speakers[0].speaker_name, None);
    }

    #[tokio::test]
    async fn removing_a_name_keeps_a_person_referenced_by_another_meeting() {
        let pool = migrated_pool().await;
        insert_meeting(&pool, "meeting-1").await;
        insert_meeting(&pool, "meeting-2").await;
        insert_transcript(&pool, "t-0", "meeting-1", Some("speaker_0"), Some(0.0)).await;
        insert_transcript(&pool, "t-1", "meeting-2", Some("speaker_0"), Some(0.0)).await;
        SpeakersRepository::assign(&pool, "meeting-1", &[assignment("speaker_0", Some("Alex"))])
            .await
            .unwrap();
        SpeakersRepository::assign(&pool, "meeting-2", &[assignment("speaker_0", Some("Alex"))])
            .await
            .unwrap();

        SpeakersRepository::assign(&pool, "meeting-1", &[assignment("speaker_0", None)])
            .await
            .unwrap();

        assert!(bindings(&pool, "meeting-1").await.is_empty());
        assert_eq!(
            bindings(&pool, "meeting-2").await,
            [("speaker_0".to_string(), "Alex".to_string())],
            "the other meeting keeps its binding"
        );
        assert_eq!(
            speaker_rows(&pool).await.len(),
            1,
            "the person is still used"
        );
    }

    #[tokio::test]
    async fn rejecting_one_person_for_two_labels_leaves_no_partial_changes() {
        let pool = migrated_pool().await;
        insert_meeting(&pool, "meeting-1").await;
        insert_transcript(&pool, "t-0", "meeting-1", Some("speaker_0"), Some(0.0)).await;
        insert_transcript(&pool, "t-1", "meeting-1", Some("speaker_1"), Some(1.0)).await;

        let error = SpeakersRepository::assign(
            &pool,
            "meeting-1",
            &[
                assignment("speaker_0", Some("Alex")),
                assignment("speaker_1", Some("alex")),
            ],
        )
        .await
        .unwrap_err();

        assert!(
            error
                .to_string()
                .contains("this person is already assigned to Speaker 1"),
            "unexpected message: {error}"
        );
        assert!(
            bindings(&pool, "meeting-1").await.is_empty(),
            "the whole batch rolled back"
        );
        assert!(
            speaker_rows(&pool).await.is_empty(),
            "no directory row survived the rollback"
        );
    }

    #[tokio::test]
    async fn rejecting_a_second_label_for_a_person_takes_the_existing_binding_into_account() {
        let pool = migrated_pool().await;
        insert_meeting(&pool, "meeting-1").await;
        insert_transcript(&pool, "t-0", "meeting-1", Some("speaker_0"), Some(0.0)).await;
        insert_transcript(&pool, "t-1", "meeting-1", Some("speaker_1"), Some(1.0)).await;
        SpeakersRepository::assign(&pool, "meeting-1", &[assignment("speaker_0", Some("Alex"))])
            .await
            .unwrap();

        let error = SpeakersRepository::assign(
            &pool,
            "meeting-1",
            &[assignment("speaker_1", Some("Alex"))],
        )
        .await
        .unwrap_err();

        assert!(
            error
                .to_string()
                .contains("this person is already assigned to Speaker 1"),
            "unexpected message: {error}"
        );
        assert_eq!(
            bindings(&pool, "meeting-1").await,
            [("speaker_0".to_string(), "Alex".to_string())],
            "the earlier assignment is untouched"
        );
    }

    #[tokio::test]
    async fn name_lengths_are_validated_after_trimming() {
        let pool = migrated_pool().await;
        insert_meeting(&pool, "meeting-1").await;
        insert_transcript(&pool, "t-0", "meeting-1", Some("speaker_0"), Some(0.0)).await;

        for (name, label) in [("", "empty"), ("   ", "whitespace only")] {
            let error = SpeakersRepository::assign(
                &pool,
                "meeting-1",
                &[assignment("speaker_0", Some(name))],
            )
            .await
            .unwrap_err();
            assert!(
                matches!(error, SpeakerError::InvalidName(_)),
                "{label} must be a validation error, got {error:?}"
            );
        }

        let hundred = "A".repeat(100);
        for name in ["A".to_string(), hundred.clone()] {
            SpeakersRepository::assign(&pool, "meeting-1", &[assignment("speaker_0", Some(&name))])
                .await
                .unwrap_or_else(|e| panic!("{} chars must be accepted: {e}", name.chars().count()));
        }

        let too_long = "A".repeat(101);
        let error = SpeakersRepository::assign(
            &pool,
            "meeting-1",
            &[assignment("speaker_0", Some(&too_long))],
        )
        .await
        .unwrap_err();
        assert!(matches!(error, SpeakerError::InvalidName(_)));
        assert_eq!(
            bindings(&pool, "meeting-1").await,
            [("speaker_0".to_string(), hundred)],
            "the rejected batch changed nothing"
        );
    }

    #[tokio::test]
    async fn name_length_counts_unicode_characters_not_bytes() {
        let pool = migrated_pool().await;
        insert_meeting(&pool, "meeting-1").await;
        insert_transcript(&pool, "t-0", "meeting-1", Some("speaker_0"), Some(0.0)).await;

        let name = "Ю".repeat(100);
        SpeakersRepository::assign(&pool, "meeting-1", &[assignment("speaker_0", Some(&name))])
            .await
            .expect("100 two-byte characters are 100 characters");
    }

    #[tokio::test]
    async fn a_label_outside_the_speaker_pattern_is_rejected() {
        let pool = migrated_pool().await;
        insert_meeting(&pool, "meeting-1").await;

        for label in [
            "mic",
            "system",
            "Alex",
            "speaker_",
            "speaker_x",
            "speaker_-1",
        ] {
            let error =
                SpeakersRepository::assign(&pool, "meeting-1", &[assignment(label, Some("Alex"))])
                    .await
                    .unwrap_err();
            assert!(
                matches!(error, SpeakerError::InvalidLabel(_)),
                "{label} must be rejected, got {error:?}"
            );
        }
        assert!(speaker_rows(&pool).await.is_empty());
    }

    #[tokio::test]
    async fn a_successful_assignment_bumps_the_meeting_timestamp() {
        let pool = migrated_pool().await;
        insert_meeting(&pool, "meeting-1").await;
        insert_transcript(&pool, "t-0", "meeting-1", Some("speaker_0"), Some(0.0)).await;
        let before = updated_at(&pool, "meeting-1").await;

        SpeakersRepository::assign(&pool, "meeting-1", &[assignment("speaker_0", Some("Alex"))])
            .await
            .unwrap();

        assert_ne!(updated_at(&pool, "meeting-1").await, before);
    }

    #[tokio::test]
    async fn resaving_the_same_panel_does_not_bump_the_meeting_timestamp() {
        let pool = migrated_pool().await;
        insert_meeting(&pool, "meeting-1").await;
        insert_transcript(&pool, "t-0", "meeting-1", Some("speaker_0"), Some(0.0)).await;
        SpeakersRepository::assign(&pool, "meeting-1", &[assignment("speaker_0", Some("Alex"))])
            .await
            .unwrap();
        let after_first = updated_at(&pool, "meeting-1").await;

        let panel = SpeakersRepository::assign(
            &pool,
            "meeting-1",
            &[assignment("speaker_0", Some("Alex"))],
        )
        .await
        .unwrap();

        assert_eq!(panel[0].speaker_name.as_deref(), Some("Alex"));
        assert_eq!(updated_at(&pool, "meeting-1").await, after_first);
    }

    #[tokio::test]
    async fn a_label_can_be_rebound_to_another_person() {
        let pool = migrated_pool().await;
        insert_meeting(&pool, "meeting-1").await;
        insert_transcript(&pool, "t-0", "meeting-1", Some("speaker_0"), Some(0.0)).await;
        SpeakersRepository::assign(&pool, "meeting-1", &[assignment("speaker_0", Some("Alex"))])
            .await
            .unwrap();

        let panel =
            SpeakersRepository::assign(&pool, "meeting-1", &[assignment("speaker_0", Some("Zoe"))])
                .await
                .unwrap();

        assert_eq!(
            bindings(&pool, "meeting-1").await,
            [("speaker_0".to_string(), "Zoe".to_string())]
        );
        assert_eq!(panel[0].speaker_name.as_deref(), Some("Zoe"));
        assert_eq!(
            speaker_rows(&pool).await,
            [(speaker_rows(&pool).await[0].0.clone(), "Zoe".to_string())],
            "the previous person was orphaned and deleted"
        );
    }

    #[tokio::test]
    async fn switching_two_labels_at_once_swaps_their_people() {
        let pool = migrated_pool().await;
        insert_meeting(&pool, "meeting-1").await;
        insert_transcript(&pool, "t-0", "meeting-1", Some("speaker_0"), Some(0.0)).await;
        insert_transcript(&pool, "t-1", "meeting-1", Some("speaker_1"), Some(1.0)).await;
        SpeakersRepository::assign(
            &pool,
            "meeting-1",
            &[
                assignment("speaker_0", Some("Alex")),
                assignment("speaker_1", Some("Zoe")),
            ],
        )
        .await
        .unwrap();

        SpeakersRepository::assign(
            &pool,
            "meeting-1",
            &[
                assignment("speaker_0", Some("Zoe")),
                assignment("speaker_1", Some("Alex")),
            ],
        )
        .await
        .expect("a batch that swaps two people must not conflict with itself");

        assert_eq!(
            bindings(&pool, "meeting-1").await,
            [
                ("speaker_0".to_string(), "Zoe".to_string()),
                ("speaker_1".to_string(), "Alex".to_string()),
            ]
        );
        assert_eq!(speaker_rows(&pool).await.len(), 2);
    }

    #[tokio::test]
    async fn assigning_to_a_missing_meeting_is_an_error() {
        let pool = migrated_pool().await;

        let error = SpeakersRepository::assign(
            &pool,
            "meeting-missing",
            &[assignment("speaker_0", Some("Alex"))],
        )
        .await
        .unwrap_err();

        assert!(
            matches!(error, SpeakerError::MeetingNotFound(_)),
            "{error:?}"
        );
        assert!(speaker_rows(&pool).await.is_empty());
    }

    #[tokio::test]
    async fn setting_and_clearing_a_segment_speaker_never_touches_bindings() {
        let pool = migrated_pool().await;
        insert_meeting(&pool, "meeting-1").await;
        insert_transcript(&pool, "t-0", "meeting-1", None, Some(0.0)).await;
        insert_transcript(&pool, "t-1", "meeting-1", Some("speaker_0"), Some(1.0)).await;
        SpeakersRepository::assign(&pool, "meeting-1", &[assignment("speaker_0", Some("Alex"))])
            .await
            .unwrap();

        SpeakersRepository::set_transcript_speaker(&pool, "t-0", Some("speaker_1"))
            .await
            .unwrap();

        let label: Option<String> =
            sqlx::query_scalar("SELECT speaker FROM transcripts WHERE id = ?")
                .bind("t-0")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(label.as_deref(), Some("speaker_1"));
        assert_eq!(
            bindings(&pool, "meeting-1").await,
            [("speaker_0".to_string(), "Alex".to_string())],
            "the per-meeting assignment is untouched"
        );
        assert_eq!(
            speaker_rows(&pool).await.len(),
            1,
            "no directory row appeared"
        );

        SpeakersRepository::set_transcript_speaker(&pool, "t-0", None)
            .await
            .unwrap();

        let label: Option<String> =
            sqlx::query_scalar("SELECT speaker FROM transcripts WHERE id = ?")
                .bind("t-0")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(label, None, "None clears the label (\"No speaker\")");
        assert_eq!(
            bindings(&pool, "meeting-1").await,
            [("speaker_0".to_string(), "Alex".to_string())],
            "clearing a segment label keeps the meeting assignment"
        );
    }

    #[tokio::test]
    async fn setting_a_segment_speaker_rejects_free_form_labels() {
        let pool = migrated_pool().await;
        insert_meeting(&pool, "meeting-1").await;
        insert_transcript(&pool, "t-0", "meeting-1", Some("speaker_0"), Some(0.0)).await;

        for label in ["mic", "system", "Guest A", "Alex", "speaker_x"] {
            let error = SpeakersRepository::set_transcript_speaker(&pool, "t-0", Some(label))
                .await
                .unwrap_err();
            assert!(
                matches!(error, SpeakerError::InvalidLabel(_)),
                "{label} must be rejected, got {error:?}"
            );
        }

        let label: Option<String> =
            sqlx::query_scalar("SELECT speaker FROM transcripts WHERE id = ?")
                .bind("t-0")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(label.as_deref(), Some("speaker_0"), "nothing was written");
    }

    #[tokio::test]
    async fn setting_the_speaker_of_an_unknown_transcript_is_an_error() {
        let pool = migrated_pool().await;
        insert_meeting(&pool, "meeting-1").await;

        let error = SpeakersRepository::set_transcript_speaker(&pool, "nope", Some("speaker_0"))
            .await
            .unwrap_err();

        assert!(
            matches!(error, SpeakerError::TranscriptNotFound(_)),
            "{error:?}"
        );
    }

    #[tokio::test]
    async fn a_per_segment_speaker_change_bumps_the_meeting_timestamp() {
        let pool = migrated_pool().await;
        insert_meeting(&pool, "meeting-1").await;
        insert_transcript(&pool, "t-0", "meeting-1", None, Some(0.0)).await;
        let before = updated_at(&pool, "meeting-1").await;

        SpeakersRepository::set_transcript_speaker(&pool, "t-0", Some("speaker_0"))
            .await
            .unwrap();

        assert_ne!(updated_at(&pool, "meeting-1").await, before);
    }

    #[tokio::test]
    async fn resaving_the_same_segment_speaker_does_not_bump_the_timestamp() {
        let pool = migrated_pool().await;
        insert_meeting(&pool, "meeting-1").await;
        insert_transcript(&pool, "t-0", "meeting-1", Some("speaker_0"), Some(0.0)).await;
        let before = updated_at(&pool, "meeting-1").await;

        SpeakersRepository::set_transcript_speaker(&pool, "t-0", Some("speaker_0"))
            .await
            .unwrap();

        assert_eq!(updated_at(&pool, "meeting-1").await, before);
    }

    #[tokio::test]
    async fn the_directory_matches_substrings_case_insensitively() {
        let pool = migrated_pool().await;
        insert_meeting(&pool, "meeting-1").await;
        insert_transcript(&pool, "t-0", "meeting-1", Some("speaker_0"), Some(0.0)).await;
        insert_transcript(&pool, "t-1", "meeting-1", Some("speaker_1"), Some(1.0)).await;
        SpeakersRepository::assign(
            &pool,
            "meeting-1",
            &[
                assignment("speaker_0", Some("Alex")),
                assignment("speaker_1", Some("Мария")),
            ],
        )
        .await
        .unwrap();

        let names = |rows: Vec<crate::api::SpeakerNameEntry>| -> Vec<String> {
            rows.into_iter().map(|row| row.name).collect()
        };

        assert_eq!(
            names(
                SpeakersRepository::search_names(&pool, "lex", 20)
                    .await
                    .unwrap()
            ),
            ["Alex"],
            "substring in any case"
        );
        assert_eq!(
            names(
                SpeakersRepository::search_names(&pool, "  AL  ", 20)
                    .await
                    .unwrap()
            ),
            ["Alex"],
            "the query is trimmed and matched case-insensitively"
        );
        assert_eq!(
            names(
                SpeakersRepository::search_names(&pool, "мария", 20)
                    .await
                    .unwrap()
            ),
            ["Мария"],
            "non-ASCII names match too"
        );
        assert!(SpeakersRepository::search_names(&pool, "bob", 20)
            .await
            .unwrap()
            .is_empty());
        assert_eq!(
            names(
                SpeakersRepository::search_names(&pool, "  ", 20)
                    .await
                    .unwrap()
            )
            .len(),
            2,
            "an empty query lists the whole directory"
        );
    }

    #[tokio::test]
    async fn the_directory_counts_meetings_and_orders_by_usage_then_name() {
        let pool = migrated_pool().await;
        insert_meeting(&pool, "meeting-1").await;
        insert_meeting(&pool, "meeting-2").await;
        insert_transcript(&pool, "t-0", "meeting-1", Some("speaker_0"), Some(0.0)).await;
        insert_transcript(&pool, "t-1", "meeting-1", Some("speaker_1"), Some(1.0)).await;
        insert_transcript(&pool, "t-2", "meeting-2", Some("speaker_0"), Some(0.0)).await;
        insert_transcript(&pool, "t-3", "meeting-2", Some("speaker_1"), Some(1.0)).await;
        SpeakersRepository::assign(
            &pool,
            "meeting-1",
            &[
                assignment("speaker_0", Some("Alex")),
                assignment("speaker_1", Some("Zoe")),
            ],
        )
        .await
        .unwrap();
        SpeakersRepository::assign(&pool, "meeting-2", &[assignment("speaker_0", Some("Alex"))])
            .await
            .unwrap();

        let rows = SpeakersRepository::search_names(&pool, "", 20)
            .await
            .unwrap();

        let summary: Vec<(&str, i64)> = rows
            .iter()
            .map(|row| (row.name.as_str(), row.meeting_count))
            .collect();
        assert_eq!(
            summary,
            [("Alex", 2), ("Zoe", 1)],
            "most used first, then by name"
        );
        let directory = speaker_rows(&pool).await;
        let alex_id = directory
            .iter()
            .find(|(_, name)| name == "Alex")
            .map(|(id, _)| id.clone())
            .expect("Alex is in the directory");
        assert_eq!(rows[0].id, alex_id, "the row carries the directory id");
    }

    #[tokio::test]
    async fn the_directory_limit_is_clamped_and_defaults() {
        let pool = migrated_pool().await;
        insert_meeting(&pool, "meeting-1").await;
        let names: Vec<String> = (0..60).map(|n| format!("Person {n:02}")).collect();
        for (index, name) in names.iter().enumerate() {
            sqlx::query(
                "INSERT INTO speakers (id, name, created_at, updated_at) VALUES (?, ?, ?, ?)",
            )
            .bind(format!("person-{index}"))
            .bind(name)
            .bind("2024-01-01T00:00:00Z")
            .bind("2024-01-01T00:00:00Z")
            .execute(&pool)
            .await
            .unwrap();
        }

        assert_eq!(
            SpeakersRepository::search_names(&pool, "", 5)
                .await
                .unwrap()
                .len(),
            5
        );
        assert_eq!(
            SpeakersRepository::search_names(&pool, "", 500)
                .await
                .unwrap()
                .len(),
            50,
            "the page size is capped"
        );
        assert_eq!(
            SpeakersRepository::search_names(&pool, "", 0)
                .await
                .unwrap()
                .len(),
            20,
            "a non-positive limit falls back to the default"
        );
    }

    #[tokio::test]
    async fn the_panel_offers_bounded_most_used_names_regardless_of_the_meeting() {
        let pool = migrated_pool().await;
        insert_meeting(&pool, "meeting-1").await;
        insert_meeting(&pool, "meeting-2").await;
        insert_meeting(&pool, "meeting-3").await;
        insert_transcript(&pool, "t-0", "meeting-1", Some("speaker_0"), Some(0.0)).await;
        insert_transcript(&pool, "t-1", "meeting-2", Some("speaker_0"), Some(0.0)).await;
        insert_transcript(&pool, "t-2", "meeting-3", Some("speaker_0"), Some(0.0)).await;
        SpeakersRepository::assign(&pool, "meeting-1", &[assignment("speaker_0", Some("Alex"))])
            .await
            .unwrap();
        SpeakersRepository::assign(&pool, "meeting-2", &[assignment("speaker_0", Some("Alex"))])
            .await
            .unwrap();
        SpeakersRepository::assign(&pool, "meeting-3", &[assignment("speaker_0", Some("Zoe"))])
            .await
            .unwrap();
        for index in 0..12 {
            sqlx::query(
                "INSERT INTO speakers (id, name, created_at, updated_at) VALUES (?, ?, ?, ?)",
            )
            .bind(format!("person-extra-{index}"))
            .bind(format!("Extra {index:02}"))
            .bind("2024-01-01T00:00:00Z")
            .bind("2024-01-01T00:00:00Z")
            .execute(&pool)
            .await
            .unwrap();
        }

        let panel = SpeakersRepository::meeting_speakers(&pool, "meeting-1")
            .await
            .unwrap();

        assert_eq!(panel.len(), 1);
        let suggestions = &panel[0].top_speaker_names;
        assert_eq!(
            suggestions,
            &[
                "Alex".to_string(),
                "Zoe".to_string(),
                "Extra 00".to_string(),
                "Extra 01".to_string(),
                "Extra 02".to_string(),
                "Extra 03".to_string(),
                "Extra 04".to_string(),
                "Extra 05".to_string(),
            ],
            "used names first, then the alphabet, capped at 8"
        );
    }

    /// Runs the reconcile inside a transaction owned by the caller, the way the
    /// import, retranscription and diarization paths call it.
    async fn reconcile(pool: &SqlitePool, meeting_id: &str) -> ReconcileOutcome {
        let mut conn = pool.acquire().await.unwrap();
        let mut transaction = conn.begin().await.unwrap();
        let outcome = reconcile_meeting_speaker_assignments(&mut transaction, meeting_id)
            .await
            .expect("reconcile");
        transaction.commit().await.unwrap();
        outcome
    }

    /// Rewrites the meeting's transcripts with a new set of labels, the way a
    /// recount does (`DELETE FROM transcripts` + re-INSERT).
    async fn recount_labels(pool: &SqlitePool, meeting_id: &str, labels: &[Option<&str>]) {
        sqlx::query("DELETE FROM transcripts WHERE meeting_id = ?")
            .bind(meeting_id)
            .execute(pool)
            .await
            .unwrap();
        for (index, label) in labels.iter().enumerate() {
            insert_transcript(
                pool,
                &format!("{meeting_id}-recount-{index}"),
                meeting_id,
                *label,
                Some(index as f64),
            )
            .await;
        }
    }

    #[tokio::test]
    async fn an_unchanged_label_set_keeps_every_name() {
        let pool = migrated_pool().await;
        insert_meeting(&pool, "meeting-1").await;
        insert_transcript(&pool, "t-0", "meeting-1", Some("speaker_0"), Some(0.0)).await;
        insert_transcript(&pool, "t-1", "meeting-1", Some("speaker_1"), Some(1.0)).await;
        assign_name(&pool, "meeting-1", "speaker_0", "Alex").await;
        assign_name(&pool, "meeting-1", "speaker_1", "Zoe").await;

        let outcome = reconcile(&pool, "meeting-1").await;

        assert_eq!(
            bindings(&pool, "meeting-1").await,
            [
                ("speaker_0".to_string(), "Alex".to_string()),
                ("speaker_1".to_string(), "Zoe".to_string()),
            ]
        );
        assert!(!outcome.names_reset);
        assert_eq!(outcome.removed_bindings, 0);
    }

    #[tokio::test]
    async fn a_grown_label_set_keeps_the_old_names_and_leaves_the_new_label_unnamed() {
        let pool = migrated_pool().await;
        insert_meeting(&pool, "meeting-1").await;
        insert_transcript(&pool, "t-0", "meeting-1", Some("speaker_0"), Some(0.0)).await;
        assign_name(&pool, "meeting-1", "speaker_0", "Alex").await;

        recount_labels(&pool, "meeting-1", &[Some("speaker_0"), Some("speaker_1")]).await;
        let outcome = reconcile(&pool, "meeting-1").await;

        assert_eq!(
            bindings(&pool, "meeting-1").await,
            [("speaker_0".to_string(), "Alex".to_string())],
            "the surviving label keeps its name"
        );
        assert!(!outcome.names_reset);
        assert_eq!(outcome.removed_bindings, 0);
        assert_eq!(
            SpeakersRepository::meeting_speakers(&pool, "meeting-1")
                .await
                .unwrap()
                .iter()
                .map(|speaker| (speaker.label.as_str(), speaker.speaker_name.as_deref()))
                .collect::<Vec<_>>(),
            [("speaker_0", Some("Alex")), ("speaker_1", None)]
        );
    }

    #[tokio::test]
    async fn a_label_that_survives_keeps_its_name_even_when_its_segment_count_changed() {
        let pool = migrated_pool().await;
        insert_meeting(&pool, "meeting-1").await;
        insert_transcript(&pool, "t-0", "meeting-1", Some("speaker_0"), Some(0.0)).await;
        insert_transcript(&pool, "t-1", "meeting-1", Some("speaker_1"), Some(1.0)).await;
        assign_name(&pool, "meeting-1", "speaker_0", "Alex").await;

        recount_labels(
            &pool,
            "meeting-1",
            &[
                Some("speaker_0"),
                Some("speaker_0"),
                Some("speaker_0"),
                Some("speaker_1"),
                Some("speaker_1"),
            ],
        )
        .await;
        reconcile(&pool, "meeting-1").await;

        assert_eq!(
            bindings(&pool, "meeting-1").await,
            [("speaker_0".to_string(), "Alex".to_string())],
            "segments are counted by label, not by segment"
        );
    }

    #[tokio::test]
    async fn a_shrunk_label_set_drops_the_removed_binding_and_its_orphaned_person() {
        let pool = migrated_pool().await;
        insert_meeting(&pool, "meeting-1").await;
        insert_transcript(&pool, "t-0", "meeting-1", Some("speaker_0"), Some(0.0)).await;
        insert_transcript(&pool, "t-1", "meeting-1", Some("speaker_1"), Some(1.0)).await;
        assign_name(&pool, "meeting-1", "speaker_0", "Alex").await;
        assign_name(&pool, "meeting-1", "speaker_1", "Zoe").await;

        recount_labels(&pool, "meeting-1", &[Some("speaker_0")]).await;
        let outcome = reconcile(&pool, "meeting-1").await;

        assert_eq!(
            bindings(&pool, "meeting-1").await,
            [("speaker_0".to_string(), "Alex".to_string())]
        );
        assert_eq!(
            speaker_rows(&pool).await,
            [(speaker_rows(&pool).await[0].0.clone(), "Alex".to_string())],
            "the person only the removed label used is gone"
        );
        assert!(!outcome.names_reset, "one label survived");
        assert_eq!(outcome.removed_bindings, 1);
    }

    #[tokio::test]
    async fn a_completely_different_label_set_resets_the_names() {
        let pool = migrated_pool().await;
        insert_meeting(&pool, "meeting-1").await;
        insert_transcript(&pool, "t-0", "meeting-1", Some("speaker_0"), Some(0.0)).await;
        insert_transcript(&pool, "t-1", "meeting-1", Some("speaker_1"), Some(1.0)).await;
        assign_name(&pool, "meeting-1", "speaker_0", "Alex").await;
        assign_name(&pool, "meeting-1", "speaker_1", "Zoe").await;

        recount_labels(&pool, "meeting-1", &[Some("speaker_4"), Some("speaker_5")]).await;
        let outcome = reconcile(&pool, "meeting-1").await;

        assert!(
            bindings(&pool, "meeting-1").await.is_empty(),
            "no old label survived, so every binding is gone"
        );
        assert!(speaker_rows(&pool).await.is_empty(), "orphans deleted");
        assert!(outcome.names_reset, "the UI must show the reset hint");
        assert_eq!(outcome.removed_bindings, 2);
    }

    #[tokio::test]
    async fn a_transcript_without_labels_reconciles_to_nothing_without_error() {
        let pool = migrated_pool().await;
        insert_meeting(&pool, "meeting-1").await;
        insert_transcript(&pool, "t-0", "meeting-1", None, Some(0.0)).await;

        let outcome = reconcile(&pool, "meeting-1").await;

        assert!(bindings(&pool, "meeting-1").await.is_empty());
        assert!(!outcome.names_reset);
        assert_eq!(outcome.removed_bindings, 0);
        assert_eq!(outcome.removed_people, 0);
    }

    #[tokio::test]
    async fn audio_source_values_are_not_labels_and_do_not_keep_a_binding_alive() {
        let pool = migrated_pool().await;
        insert_meeting(&pool, "meeting-1").await;
        insert_transcript(&pool, "t-0", "meeting-1", Some("speaker_0"), Some(0.0)).await;
        assign_name(&pool, "meeting-1", "speaker_0", "Alex").await;

        recount_labels(
            &pool,
            "meeting-1",
            &[Some("mic"), Some("system"), Some("Guest A")],
        )
        .await;
        let outcome = reconcile(&pool, "meeting-1").await;

        assert!(
            bindings(&pool, "meeting-1").await.is_empty(),
            "mic/system/free-form are audio sources, not speakers"
        );
        assert!(outcome.names_reset);
        assert!(speaker_rows(&pool).await.is_empty());
    }

    #[tokio::test]
    async fn reconcile_bumps_the_meeting_timestamp_only_when_it_removes_something() {
        let pool = migrated_pool().await;
        insert_meeting(&pool, "meeting-1").await;
        insert_transcript(&pool, "t-0", "meeting-1", Some("speaker_0"), Some(0.0)).await;
        assign_name(&pool, "meeting-1", "speaker_0", "Alex").await;
        let before = updated_at(&pool, "meeting-1").await;

        reconcile(&pool, "meeting-1").await;
        assert_eq!(
            updated_at(&pool, "meeting-1").await,
            before,
            "an unchanged label set is a no-op"
        );

        recount_labels(&pool, "meeting-1", &[Some("speaker_1")]).await;
        reconcile(&pool, "meeting-1").await;
        assert_ne!(
            updated_at(&pool, "meeting-1").await,
            before,
            "removing a binding is a change to the meeting"
        );
    }

    #[tokio::test]
    async fn reconcile_touches_only_its_own_meeting() {
        let pool = migrated_pool().await;
        insert_meeting(&pool, "meeting-1").await;
        insert_meeting(&pool, "meeting-2").await;
        insert_transcript(&pool, "t-0", "meeting-1", Some("speaker_0"), Some(0.0)).await;
        insert_transcript(&pool, "t-1", "meeting-2", Some("speaker_0"), Some(0.0)).await;
        assign_name(&pool, "meeting-1", "speaker_0", "Alex").await;
        assign_name(&pool, "meeting-2", "speaker_0", "Zoe").await;

        recount_labels(&pool, "meeting-1", &[Some("speaker_3")]).await;
        let outcome = reconcile(&pool, "meeting-1").await;

        assert!(outcome.names_reset);
        assert_eq!(
            bindings(&pool, "meeting-2").await,
            [("speaker_0".to_string(), "Zoe".to_string())],
            "another meeting keeps its names"
        );
        assert_eq!(
            speaker_rows(&pool).await,
            [(speaker_rows(&pool).await[0].0.clone(), "Zoe".to_string())],
            "a person still used elsewhere is not orphaned"
        );
    }
}
