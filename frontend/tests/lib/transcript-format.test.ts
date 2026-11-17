import { describe, expect, test } from 'bun:test';

import {
  formatSpeakerDisplayLabel,
  formatTranscriptText,
  formatTranscriptTime,
  hasSpeakerLabels,
} from '../../src/lib/transcript-format';
import type { Transcript } from '../../src/types';

function segment(overrides: Partial<Transcript> = {}): Transcript {
  return {
    id: '1',
    text: 'Всем привет.',
    timestamp: '14:30:05',
    audio_start_time: 0,
    audio_end_time: 24,
    ...overrides,
  };
}

describe('formatTranscriptTime', () => {
  test('formats seconds as MM:SS', () => {
    expect(formatTranscriptTime(0)).toBe('00:00');
    expect(formatTranscriptTime(24)).toBe('00:24');
    expect(formatTranscriptTime(75)).toBe('01:15');
  });

  test('undefined yields --:--', () => {
    expect(formatTranscriptTime(undefined)).toBe('--:--');
  });
});

describe('formatTranscriptText', () => {
  test('without speakers keeps the legacy [MM:SS] format', () => {
    const text = formatTranscriptText([
      segment({ text: 'Всем привет.', audio_start_time: 0, audio_end_time: 24 }),
      segment({ id: '2', text: 'Добрый день.', audio_start_time: 24, audio_end_time: 40 }),
    ]);
    expect(text).toBe('[00:00] Всем привет.\n[00:24] Добрый день.');
  });

  test('with speakers produces ranges and the assigned names', () => {
    const text = formatTranscriptText([
      segment({ text: 'Всем привет.', speaker: 'speaker_0', speaker_name: 'Alex' }),
      segment({
        id: '2',
        text: 'Добрый день.',
        audio_start_time: 24,
        audio_end_time: 40,
        speaker: 'speaker_1',
        speaker_name: 'Maria',
      }),
    ]);
    expect(text).toBe('[00:00–00:24] Alex: Всем привет.\n[00:24–00:40] Maria: Добрый день.');
  });

  test('without a name falls back to a 1-based Speaker N taken from the label suffix', () => {
    const text = formatTranscriptText([
      segment({ text: 'Добрый день.', speaker: 'speaker_1', audio_start_time: 24, audio_end_time: 40 }),
    ]);
    expect(text).toBe('[00:24–00:40] Speaker 2: Добрый день.');
  });

  test('segments without a label keep the range but omit the label', () => {
    const text = formatTranscriptText([
      segment({ text: 'Всем привет.', speaker: 'speaker_0', speaker_name: 'Alex' }),
      segment({ id: '2', text: 'Неразборчиво.', audio_start_time: 24, audio_end_time: 40 }),
    ]);
    expect(text).toBe('[00:00–00:24] Alex: Всем привет.\n[00:24–00:40] Неразборчиво.');
  });

  test('legacy mic/system/free-form speaker values keep the legacy format and get no prefix', () => {
    const text = formatTranscriptText([
      segment({ text: 'Всем привет.', speaker: 'mic' }),
      segment({ id: '2', text: 'Добрый день.', audio_start_time: 24, audio_end_time: 40, speaker: 'system' }),
      segment({ id: '3', text: 'Третий.', audio_start_time: 40, audio_end_time: 50, speaker: 'Guest A' }),
    ]);
    expect(text).toBe('[00:00] Всем привет.\n[00:24] Добрый день.\n[00:40] Третий.');
  });

  test('a name without a diarization label does not trigger the labelled format', () => {
    const text = formatTranscriptText([
      segment({ text: 'Всем привет.', speaker_name: 'Alex' }),
      segment({ id: '2', text: 'Добрый день.', audio_start_time: 24, audio_end_time: 40 }),
    ]);
    expect(text).toBe('[00:00] Всем привет.\n[00:24] Добрый день.');
  });

  test('a segment without audio_end_time falls back to a single timestamp', () => {
    const text = formatTranscriptText([
      segment({ text: 'Всем привет.', speaker: 'speaker_0', speaker_name: 'Alex', audio_end_time: undefined }),
    ]);
    expect(text).toBe('[00:00] Alex: Всем привет.');
  });

  test('a segment without audio_start_time uses the wall-clock fallback verbatim', () => {
    const text = formatTranscriptText(
      [segment({ audio_start_time: undefined, audio_end_time: undefined })],
      { fallbackTimestamp: t => t.timestamp },
    );
    expect(text).toBe('14:30:05 Всем привет.');
  });

  test('header is prepended and markdownLineBreaks adds two trailing spaces', () => {
    const text = formatTranscriptText(
      [segment({ text: 'Всем привет.', speaker: 'speaker_0', speaker_name: 'Alex' })],
      { header: '# Title\n\n', markdownLineBreaks: true },
    );
    expect(text).toBe('# Title\n\n[00:00–00:24] Alex: Всем привет.  ');
  });
});

describe('formatSpeakerDisplayLabel', () => {
  test('uses the assigned name as-is', () => {
    expect(formatSpeakerDisplayLabel('speaker_0', 'Alex')).toBe('Alex');
    expect(formatSpeakerDisplayLabel('speaker_0', 'Guest  A')).toBe('Guest  A');
  });

  test('an empty or whitespace-only name counts as absent', () => {
    expect(formatSpeakerDisplayLabel('speaker_0', '')).toBe('Speaker 1');
    expect(formatSpeakerDisplayLabel('speaker_0', '   ')).toBe('Speaker 1');
  });

  test('falls back to the 1-based number from the label, not from ordering', () => {
    expect(formatSpeakerDisplayLabel('speaker_0', undefined)).toBe('Speaker 1');
    expect(formatSpeakerDisplayLabel('speaker_7', undefined)).toBe('Speaker 8');
  });

  test('non-diarization values produce no label', () => {
    expect(formatSpeakerDisplayLabel('mic', 'Alex')).toBeNull();
    expect(formatSpeakerDisplayLabel('system', undefined)).toBeNull();
    expect(formatSpeakerDisplayLabel('Guest A', 'Guest A')).toBeNull();
    expect(formatSpeakerDisplayLabel(undefined, 'Alex')).toBeNull();
    expect(formatSpeakerDisplayLabel(undefined, undefined)).toBeNull();
  });
});

describe('hasSpeakerLabels', () => {
  test('is true when at least one segment carries a diarization label', () => {
    expect(hasSpeakerLabels([segment(), segment({ id: '2', speaker: 'speaker_3' })])).toBe(true);
  });

  test('is false without diarization labels, including legacy values and names alone', () => {
    expect(hasSpeakerLabels([segment()])).toBe(false);
    expect(hasSpeakerLabels([segment({ speaker: 'mic' }), segment({ id: '2', speaker: 'system' })])).toBe(false);
    expect(hasSpeakerLabels([segment({ speaker: 'Guest A' })])).toBe(false);
    expect(hasSpeakerLabels([segment({ speaker_name: 'Alex' })])).toBe(false);
  });
});
