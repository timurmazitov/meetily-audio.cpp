import { Transcript } from '@/types';

/** Matches the only values the diarization pass writes (`speaker_0`, `speaker_1`, …). */
const DIARIZATION_LABEL = /^speaker_(\d+)$/;

/**
 * Format recording-relative seconds as MM:SS.
 */
export function formatTranscriptTime(seconds: number | undefined): string {
  if (seconds === undefined) return '--:--';
  const totalSecs = Math.floor(seconds);
  const mins = Math.floor(totalSecs / 60);
  const secs = totalSecs % 60;
  return `${mins.toString().padStart(2, '0')}:${secs.toString().padStart(2, '0')}`;
}

/**
 * Whether a speaker value is a diarization label (`speaker_N`).
 *
 * The legacy `speaker` column also documented `mic`/`system` audio-source
 * values, and free-form strings are not diarization labels either; neither
 * counts as a speaker (same rule as the transcript chip).
 */
export function isDiarizationSpeakerLabel(speaker: string | undefined): boolean {
  return speaker !== undefined && DIARIZATION_LABEL.test(speaker);
}

/**
 * Display label for a single segment's speaker: the assigned name, else
 * `Speaker N` (1-based, N taken from the `speaker_N` label suffix), else
 * `null` when the segment has no diarization label (no prefix at all).
 * The name is used verbatim; an empty or whitespace-only name counts as absent.
 */
export function formatSpeakerDisplayLabel(
  speaker: string | undefined,
  speakerName: string | undefined,
): string | null {
  if (!speaker) return null;
  const match = DIARIZATION_LABEL.exec(speaker);
  if (!match) return null;
  if (speakerName && speakerName.trim()) return speakerName;
  return `Speaker ${Number(match[1]) + 1}`;
}

/**
 * Whether any segment carries a diarization label. Only then do lines get the
 * `[MM:SS–MM:SS]` range and a speaker prefix; otherwise the legacy format is
 * kept byte-for-byte.
 *
 * Deliberate legacy decision: a transcript without diarization labels keeps
 * the old `[MM:SS] text` format unchanged. The legacy `speaker` column also
 * documented `mic`/`system` and stored free-form strings; those are audio
 * sources, not people, so they never produce a prefix or a time range. The
 * transcript chip, the Speakers panel and the per-row dropdown follow the same
 * rule (see `isDiarizationSpeakerLabel`).
 */
export function hasSpeakerLabels(transcripts: Transcript[]): boolean {
  return transcripts.some(t => isDiarizationSpeakerLabel(t.speaker));
}

export interface FormatTranscriptOptions {
  /** Prepended verbatim to the formatted text (e.g. a markdown header). */
  header?: string;
  /**
   * Fallback for old transcripts without audio_start_time. The returned label
   * is used verbatim, without brackets (legacy meeting-details copy behavior).
   */
  fallbackTimestamp?: (transcript: Transcript) => string;
  /** Append two trailing spaces to each line (markdown line break, legacy). */
  markdownLineBreaks?: boolean;
}

/**
 * Format transcript segments as plain text, one line per segment.
 *
 * When any segment carries a diarization speaker label, every line includes a
 * `[MM:SS–MM:SS]` time range and a speaker prefix — the assigned name, else
 * `Speaker N`. Lines without a label keep the range but omit the prefix. When
 * no segment has a diarization label, lines keep the legacy `[MM:SS]` format.
 * A segment without audio_end_time falls back to a single `[MM:SS]` label.
 */
export function formatTranscriptText(
  transcripts: Transcript[],
  options: FormatTranscriptOptions = {},
): string {
  const hasSpeakers = hasSpeakerLabels(transcripts);
  const suffix = options.markdownLineBreaks ? '  ' : '';

  const lines = transcripts.map(t => {
    let prefix: string;
    if (t.audio_start_time === undefined) {
      prefix = options.fallbackTimestamp ? options.fallbackTimestamp(t) : '[--:--]';
    } else {
      const start = formatTranscriptTime(t.audio_start_time);
      const timeLabel = hasSpeakers && t.audio_end_time !== undefined
        ? `${start}–${formatTranscriptTime(t.audio_end_time)}`
        : start;
      prefix = `[${timeLabel}]`;
    }
    const displayLabel = hasSpeakers ? formatSpeakerDisplayLabel(t.speaker, t.speaker_name) : null;
    const speakerLabel = displayLabel ? `${displayLabel}: ` : '';
    return `${prefix} ${speakerLabel}${t.text}${suffix}`;
  });

  return (options.header ?? '') + lines.join('\n');
}
