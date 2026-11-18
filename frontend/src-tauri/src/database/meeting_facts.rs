//! Reading the meeting's own moment and length out of a recording folder.
//!
//! `metadata.json` is written by `recording_saver`: `created_at` at the moment
//! the recording starts (so it is the real start, unlike the database row's
//! `created_at`, which appears after the stop), and `duration_seconds` when the
//! recording is saved. Both are re-read here at the save point instead of being
//! kept in memory, because the frontend, not the saver, calls the database save.
//!
//! ORDERING: the saver writes the final `metadata.json` (status `completed`,
//! with the duration) before it emits `recording-stopped`, and the frontend
//! saves the meeting only after that event. The file is therefore normally
//! complete here. When it is not (missing, unreadable, still `recording`), the
//! fallbacks below keep the row correct anyway: a last-segment length and "the
//! moment of the save" are honest approximations, and `None` is used when even
//! those are unavailable.

use std::path::Path;

const METADATA_FILE: &str = "metadata.json";

/// What the meeting folder says about the meeting itself.
#[derive(Debug, Default, Clone, Copy, PartialEq)]
pub(crate) struct FolderMeetingFacts {
    /// Start of the recording in milliseconds since the Unix epoch.
    pub recorded_at_ms: Option<i64>,
    pub duration_seconds: Option<f64>,
}

/// A length worth storing: zero, negative or non-finite means "unknown" (the
/// recording was too short or could not be measured), and the spec forbids
/// writing that as a real length.
///
/// This is the one home of the rule for Rust. The other two are the SQL
/// `HAVING MAX(audio_end_time) > 0` in `database/backfill.rs` and the display
/// formatter `formatMeetingDuration` in the frontend; they read the same values
/// from places Rust cannot reach.
pub(crate) fn usable_duration_seconds(duration_seconds: Option<f64>) -> Option<f64> {
    duration_seconds.filter(|seconds| seconds.is_finite() && *seconds > 0.0)
}

/// Reads the meeting's own facts from `folder_path`, if it has any.
///
/// A missing folder, a missing file, unparsable JSON, or an unusable value each
/// simply drop that field: suggesting a value is the caller's decision, and the
/// spec forbids inventing one here.
pub(crate) fn read_folder_meeting_facts(folder_path: Option<&str>) -> FolderMeetingFacts {
    let Some(folder_path) = folder_path.map(str::trim).filter(|path| !path.is_empty()) else {
        return FolderMeetingFacts::default();
    };

    let metadata_path = Path::new(folder_path).join(METADATA_FILE);
    let Ok(raw) = std::fs::read_to_string(&metadata_path) else {
        return FolderMeetingFacts::default();
    };
    let Ok(value) = serde_json::from_str::<serde_json::Value>(&raw) else {
        return FolderMeetingFacts::default();
    };

    FolderMeetingFacts {
        recorded_at_ms: value
            .get("created_at")
            .and_then(|value| value.as_str())
            .and_then(parse_moment_ms),
        duration_seconds: usable_duration_seconds(
            value.get("duration_seconds").and_then(|value| value.as_f64()),
        ),
    }
}

/// RFC3339 is what the saver writes; `YYYY-MM-DD HH:MM:SS` and a bare date are
/// accepted too so an older or hand-edited file is still usable.
fn parse_moment_ms(value: &str) -> Option<i64> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return None;
    }

    if let Ok(moment) = chrono::DateTime::parse_from_rfc3339(trimmed) {
        return Some(moment.timestamp_millis());
    }

    for format in ["%Y-%m-%d %H:%M:%S", "%Y-%m-%dT%H:%M:%S"] {
        if let Ok(moment) = chrono::NaiveDateTime::parse_from_str(trimmed, format) {
            return Some(moment.and_utc().timestamp_millis());
        }
    }

    chrono::NaiveDate::parse_from_str(trimmed, "%Y-%m-%d")
        .ok()
        .and_then(|day| day.and_hms_opt(0, 0, 0))
        .map(|moment| moment.and_utc().timestamp_millis())
}

#[cfg(test)]
mod tests {
    use super::{read_folder_meeting_facts, usable_duration_seconds, FolderMeetingFacts};

    #[test]
    fn a_length_is_stored_only_when_it_is_a_real_positive_measurement() {
        assert_eq!(usable_duration_seconds(Some(4_975.5)), Some(4_975.5));
        assert_eq!(usable_duration_seconds(Some(0.5)), Some(0.5));
        // Zero, negative, NaN and the infinities are all "not measured".
        assert_eq!(usable_duration_seconds(Some(0.0)), None);
        assert_eq!(usable_duration_seconds(Some(-3.0)), None);
        assert_eq!(usable_duration_seconds(Some(f64::NAN)), None);
        assert_eq!(usable_duration_seconds(Some(f64::INFINITY)), None);
        assert_eq!(usable_duration_seconds(None), None);
    }

    fn folder_with(metadata: &str) -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("metadata.json"), metadata).unwrap();
        dir
    }

    #[test]
    fn reads_the_start_moment_and_the_finished_length() {
        let dir = folder_with(
            r#"{"created_at": "2026-11-18T14:32:05+00:00", "duration_seconds": 4975.5}"#,
        );

        let facts = read_folder_meeting_facts(Some(&dir.path().to_string_lossy()));
        assert_eq!(
            facts.recorded_at_ms,
            Some(1_795_012_325_000),
            "14:32:05 UTC on 2026-11-18 in epoch milliseconds"
        );
        assert_eq!(facts.duration_seconds, Some(4_975.5));
    }

    #[test]
    fn a_missing_or_unreadable_file_yields_nothing_instead_of_a_guess() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(
            read_folder_meeting_facts(Some(&dir.path().to_string_lossy())),
            FolderMeetingFacts::default()
        );
        assert_eq!(
            read_folder_meeting_facts(None),
            FolderMeetingFacts::default()
        );

        let broken = folder_with("{ this is not json");
        assert_eq!(
            read_folder_meeting_facts(Some(&broken.path().to_string_lossy())),
            FolderMeetingFacts::default()
        );

        // A recording that never finished has no length yet; a zero length is
        // "unknown", not a real result.
        let unfinished =
            folder_with(r#"{"created_at": "2026-11-18T14:32:05Z", "duration_seconds": 0}"#);
        let facts = read_folder_meeting_facts(Some(&unfinished.path().to_string_lossy()));
        assert_eq!(facts.recorded_at_ms, Some(1_795_012_325_000));
        assert_eq!(facts.duration_seconds, None);
    }

    #[test]
    fn an_unparsable_moment_does_not_discard_the_length() {
        let dir = folder_with(r#"{"created_at": "sometime", "duration_seconds": 42}"#);
        let facts = read_folder_meeting_facts(Some(&dir.path().to_string_lossy()));
        assert_eq!(facts.recorded_at_ms, None);
        assert_eq!(facts.duration_seconds, Some(42.0));
    }
}
