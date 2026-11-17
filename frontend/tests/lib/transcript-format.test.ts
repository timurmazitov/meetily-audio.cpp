import { describe, expect, test } from 'bun:test';

import { formatTranscriptText, formatTranscriptTime } from '../../src/lib/transcript-format';
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

  test('with speakers produces ranges and raw speaker labels', () => {
    const text = formatTranscriptText([
      segment({ text: 'Всем привет.', speaker: 'speaker_0' }),
      segment({ id: '2', text: 'Добрый день.', audio_start_time: 24, audio_end_time: 40, speaker: 'speaker_1' }),
    ]);
    expect(text).toBe('[00:00–00:24] speaker_0: Всем привет.\n[00:24–00:40] speaker_1: Добрый день.');
  });

  test('segments without a speaker keep the range but omit the label', () => {
    const text = formatTranscriptText([
      segment({ text: 'Всем привет.', speaker: 'speaker_0' }),
      segment({ id: '2', text: 'Неразборчиво.', audio_start_time: 24, audio_end_time: 40 }),
    ]);
    expect(text).toBe('[00:00–00:24] speaker_0: Всем привет.\n[00:24–00:40] Неразборчиво.');
  });

  test('a segment without audio_end_time falls back to a single timestamp', () => {
    const text = formatTranscriptText([
      segment({ text: 'Всем привет.', speaker: 'speaker_0', audio_end_time: undefined }),
    ]);
    expect(text).toBe('[00:00] speaker_0: Всем привет.');
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
      [segment({ text: 'Всем привет.', speaker: 'speaker_0' })],
      { header: '# Title\n\n', markdownLineBreaks: true },
    );
    expect(text).toBe('# Title\n\n[00:00–00:24] speaker_0: Всем привет.  ');
  });
});
