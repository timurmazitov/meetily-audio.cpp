import type { Transcript } from '@/types';
import {
  formatSpeakerDisplayLabel,
  formatTranscriptTime,
  hasSpeakerLabels,
} from '@/lib/transcript-format';

export interface SummaryTranscriptPayload {
  /** Segments joined by newline, fed to the summarization model. */
  transcriptText: string;
  /** Raw segment texts, used for language detection only. */
  transcriptTexts: string[];
}

/**
 * Build the transcript payload for summary generation.
 *
 * When any segment carries a diarization speaker label, each line reads
 * `[MM:SS] <name|Speaker N>: text`; segments without a label keep the
 * timestamp and no prefix. When no segment has such a label the legacy
 * `[MM:SS] text` format is kept byte-for-byte.
 *
 * A segment without `audio_start_time` uses its wall-clock `timestamp`
 * verbatim (no brackets), which is what legacy rows rely on.
 */
export function buildSummaryTranscriptPayload(transcripts: Transcript[]): SummaryTranscriptPayload {
  const withSpeakers = hasSpeakerLabels(transcripts);

  return {
    transcriptText: transcripts
      .map(transcript => {
        const time = transcript.audio_start_time === undefined
          ? transcript.timestamp
          : `[${formatTranscriptTime(transcript.audio_start_time)}]`;
        const label = withSpeakers
          ? formatSpeakerDisplayLabel(transcript.speaker, transcript.speaker_name)
          : null;
        const prefix = label ? `${label}: ` : '';
        return `${time} ${prefix}${transcript.text}`;
      })
      .join('\n'),
    transcriptTexts: transcripts.map(transcript => transcript.text),
  };
}
