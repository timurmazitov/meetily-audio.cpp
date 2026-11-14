// audio/transcription/diarization.rs
//
// Speaker attribution: overlay of diarization speaker turns onto transcript
// segments (GLOSSARY: «диаризация», «спикер», «поворот спикера»).
//
// The attribution rule is a pure function — segment timestamps and speaker
// turns in, an optional speaker label per segment out — so the boundary
// cases (segment straddling two turns, segment with no coverage, empty turn
// list, speaker change inside a segment) are fixed by the unit tests in
// this module and stay independent of any engine.

/// One speaker turn interval in seconds from the start of the recording.
///
/// `speaker_id` is the raw identifier the diarization engine produced
/// (e.g. `SPEAKER_00`); callers renumber speakers by order of appearance.
#[derive(Debug, Clone, PartialEq)]
pub struct SpeakerTurnInterval {
    pub start_sec: f64,
    pub end_sec: f64,
    pub speaker_id: String,
}

impl SpeakerTurnInterval {
    pub fn new(start_sec: f64, end_sec: f64, speaker_id: impl Into<String>) -> Self {
        Self {
            start_sec,
            end_sec,
            speaker_id: speaker_id.into(),
        }
    }
}

/// Overlap between one transcript segment `[start, end]` and one turn, in
/// seconds. Zero-length segments (degenerate data) are attributed to the
/// turn containing the start instant, when any.
fn overlap_seconds(start: f64, end: f64, turn: &SpeakerTurnInterval) -> f64 {
    if end > start {
        (end.min(turn.end_sec) - start.max(turn.start_sec)).max(0.0)
    } else if turn.start_sec <= start && start < turn.end_sec {
        1.0
    } else {
        0.0
    }
}

/// The speaker id of the turn covering the largest share of the segment.
///
/// Ties are broken by the turn that starts earliest; a segment with no
/// overlap with any turn gets no speaker.
fn dominant_speaker(start: f64, end: f64, turns: &[SpeakerTurnInterval]) -> Option<&str> {
    let mut best: Option<(&SpeakerTurnInterval, f64)> = None;
    for turn in turns {
        let overlap = overlap_seconds(start, end, turn);
        if overlap <= 0.0 {
            continue;
        }
        let wins = match best {
            None => true,
            Some((current, current_overlap)) => {
                overlap > current_overlap
                    || (overlap == current_overlap && turn.start_sec < current.start_sec)
            }
        };
        if wins {
            best = Some((turn, overlap));
        }
    }
    best.map(|(turn, _)| turn.speaker_id.as_str())
}

/// Attribute each transcript segment to a speaker, renumbered by order of
/// appearance (GLOSSARY: «спикер» is identified as `speaker_0`,
/// `speaker_1`, … by arrival order, without identity).
///
/// `segments` are `(audio_start_time, audio_end_time)` pairs in seconds from
/// the start of the recording. Segments without coverage keep `None`, so
/// transcripts diarized only in part still render.
pub fn attribute_segments_to_speakers(
    segments: &[(f64, f64)],
    turns: &[SpeakerTurnInterval],
) -> Vec<Option<String>> {
    let mut labels: Vec<Option<String>> = Vec::with_capacity(segments.len());
    let mut speaker_numbers: Vec<&str> = Vec::new();

    for &(start, end) in segments {
        let speaker = dominant_speaker(start, end, turns).map(|speaker_id| {
            let index = match speaker_numbers.iter().position(|seen| *seen == speaker_id) {
                Some(index) => index,
                None => {
                    speaker_numbers.push(speaker_id);
                    speaker_numbers.len() - 1
                }
            };
            format!("speaker_{index}")
        });
        labels.push(speaker);
    }

    labels
}

#[cfg(test)]
mod tests {
    use super::*;

    fn turns(pairs: &[(f64, f64, &str)]) -> Vec<SpeakerTurnInterval> {
        pairs
            .iter()
            .map(|&(start, end, speaker)| SpeakerTurnInterval::new(start, end, speaker))
            .collect()
    }

    #[test]
    fn a_segment_fully_inside_one_turn_gets_that_speaker() {
        let turns = turns(&[(0.0, 10.0, "SPEAKER_00")]);
        let labels = attribute_segments_to_speakers(&[(1.0, 5.0)], &turns);
        assert_eq!(labels, vec![Some("speaker_0".to_string())]);
    }

    #[test]
    fn a_segment_straddling_two_turns_follows_the_larger_overlap() {
        // Turn A covers [0, 5], turn B covers [5, 10]; the second segment is
        // split exactly at the turn boundary but speaks mostly in turn B.
        let turns = turns(&[(0.0, 5.0, "SPEAKER_00"), (5.0, 10.0, "SPEAKER_01")]);
        let labels = attribute_segments_to_speakers(&[(0.5, 2.0), (4.0, 9.0)], &turns);
        assert_eq!(
            labels,
            vec![Some("speaker_0".to_string()), Some("speaker_1".to_string())]
        );
    }

    #[test]
    fn an_exact_boundary_tie_goes_to_the_turn_that_starts_earliest() {
        // Half of the segment lies in each turn with equal overlap; the
        // earlier turn wins, so a boundary placement is deterministic.
        let turns = turns(&[(0.0, 5.0, "SPEAKER_00"), (5.0, 10.0, "SPEAKER_01")]);
        let labels = attribute_segments_to_speakers(&[(3.0, 7.0)], &turns);
        assert_eq!(labels, vec![Some("speaker_0".to_string())]);
    }

    #[test]
    fn a_segment_with_no_turn_coverage_gets_no_speaker() {
        let turns = turns(&[(0.0, 5.0, "SPEAKER_00")]);
        let labels = attribute_segments_to_speakers(&[(10.0, 12.0)], &turns);
        assert_eq!(labels, vec![None]);
    }

    #[test]
    fn empty_turn_list_leaves_every_segment_without_a_speaker() {
        let labels = attribute_segments_to_speakers(&[(0.0, 2.0), (3.0, 4.0)], &[]);
        assert_eq!(labels, vec![None, None]);
    }

    #[test]
    fn empty_segments_yield_no_labels() {
        let turns = turns(&[(0.0, 10.0, "SPEAKER_00")]);
        let labels = attribute_segments_to_speakers(&[], &turns);
        assert!(labels.is_empty());
    }

    #[test]
    fn a_speaker_change_inside_a_segment_follows_who_speaks_most() {
        // Segment [0, 10]: speaker A for [0, 2], speaker B for [2, 10] —
        // the segment is attributed to B without being split.
        let turns = turns(&[(0.0, 2.0, "SPEAKER_00"), (2.0, 10.0, "SPEAKER_01")]);
        let labels = attribute_segments_to_speakers(&[(0.0, 1.0), (0.0, 10.0)], &turns);
        assert_eq!(
            labels,
            vec![Some("speaker_0".to_string()), Some("speaker_1".to_string())]
        );
    }

    #[test]
    fn speakers_are_numbered_by_order_of_appearance_not_engine_id() {
        // The engine's first turn belongs to SPEAKER_07; the first speaker
        // the transcript meets must still be speaker_0.
        let turns = turns(&[(10.0, 20.0, "SPEAKER_07"), (0.0, 5.0, "SPEAKER_03")]);
        let labels = attribute_segments_to_speakers(
            &[(0.0, 5.0), (10.0, 20.0), (12.0, 14.0)],
            &turns,
        );
        assert_eq!(
            labels,
            vec![
                Some("speaker_0".to_string()),
                Some("speaker_1".to_string()),
                Some("speaker_1".to_string()),
            ]
        );
    }

    #[test]
    fn uncovered_segments_do_not_consume_speaker_numbers() {
        let turns = turns(&[(0.0, 5.0, "SPEAKER_00"), (10.0, 15.0, "SPEAKER_01")]);
        let labels = attribute_segments_to_speakers(
            &[(0.0, 5.0), (6.0, 8.0), (10.0, 15.0)],
            &turns,
        );
        assert_eq!(
            labels,
            vec![
                Some("speaker_0".to_string()),
                None,
                Some("speaker_1".to_string()),
            ]
        );
    }

    #[test]
    fn zero_length_segment_is_attributed_to_the_turn_containing_the_instant() {
        let turns = turns(&[(0.0, 5.0, "SPEAKER_00"), (5.0, 10.0, "SPEAKER_01")]);
        let labels = attribute_segments_to_speakers(&[(5.0, 5.0), (12.0, 12.0)], &turns);
        // 5.0 is covered by the second turn (half-open intervals).
        assert_eq!(labels[0], Some("speaker_0".to_string()));
        // ...and 12.0 lies outside every turn.
        assert_eq!(labels[1], None);
    }

    #[test]
    fn unordered_turns_do_not_change_the_attribution() {
        let turns = turns(&[(5.0, 10.0, "SPEAKER_01"), (0.0, 5.0, "SPEAKER_00")]);
        let labels = attribute_segments_to_speakers(&[(0.5, 2.0), (4.0, 9.0)], &turns);
        assert_eq!(
            labels,
            vec![Some("speaker_0".to_string()), Some("speaker_1".to_string())]
        );
    }
}
