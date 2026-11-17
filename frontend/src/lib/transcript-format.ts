import { Transcript } from '@/types';

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
 * `[MM:SS–MM:SS]` time range and a raw `speaker_N:` prefix; lines without a
 * speaker keep the range but omit the prefix. When no segment has a speaker,
 * lines keep the legacy `[MM:SS]` format. A segment without audio_end_time
 * falls back to a single `[MM:SS]` label.
 */
export function formatTranscriptText(
  transcripts: Transcript[],
  options: FormatTranscriptOptions = {},
): string {
  const hasSpeakers = transcripts.some(t => t.speaker);
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
    const speakerLabel = hasSpeakers && t.speaker ? `${t.speaker}: ` : '';
    return `${prefix} ${speakerLabel}${t.text}${suffix}`;
  });

  return (options.header ?? '') + lines.join('\n');
}
