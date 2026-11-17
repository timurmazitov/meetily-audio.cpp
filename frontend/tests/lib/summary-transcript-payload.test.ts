import { describe, expect, test } from 'bun:test';

import { buildSummaryTranscriptPayload } from '../../src/lib/summary-transcript-payload';
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

describe('buildSummaryTranscriptPayload', () => {
  test('uses the assigned names as the speaker prefix', () => {
    const payload = buildSummaryTranscriptPayload([
      segment({ text: 'Всем привет.', speaker: 'speaker_0', speaker_name: 'Alex' }),
      segment({
        id: '2',
        text: 'Добрый день.',
        timestamp: '14:30:29',
        audio_start_time: 24,
        audio_end_time: 40,
        speaker: 'speaker_1',
        speaker_name: 'Maria',
      }),
    ]);

    expect(payload.transcriptText).toBe('[00:00] Alex: Всем привет.\n[00:24] Maria: Добрый день.');
    expect(payload.transcriptTexts).toEqual(['Всем привет.', 'Добрый день.']);
  });

  test('falls back to the 1-based Speaker N taken from the label suffix', () => {
    const payload = buildSummaryTranscriptPayload([
      segment({ text: 'Добрый день.', speaker: 'speaker_1', audio_start_time: 24, audio_end_time: 40 }),
    ]);

    expect(payload.transcriptText).toBe('[00:24] Speaker 2: Добрый день.');
  });

  test('prefixes only segments that carry a speaker label', () => {
    const payload = buildSummaryTranscriptPayload([
      segment({ text: 'Всем привет.', speaker: 'speaker_0', speaker_name: 'Alex' }),
      segment({
        id: '2',
        text: 'Добрый день.',
        timestamp: '14:30:29',
        audio_start_time: 24,
        audio_end_time: 40,
        speaker: 'speaker_1',
      }),
      segment({ id: '3', text: 'Неразборчиво.', timestamp: '14:30:41', audio_start_time: 40, audio_end_time: 50 }),
    ]);

    expect(payload.transcriptText).toBe(
      '[00:00] Alex: Всем привет.\n[00:24] Speaker 2: Добрый день.\n[00:40] Неразборчиво.',
    );
    expect(payload.transcriptTexts).toEqual(['Всем привет.', 'Добрый день.', 'Неразборчиво.']);
  });

  test('keeps the legacy format byte-for-byte when no segment has a speaker label', () => {
    const payload = buildSummaryTranscriptPayload([
      segment({ text: 'Всем привет.', timestamp: '14:30:05', audio_start_time: 0 }),
      segment({
        id: '2',
        text: 'Добрый день.',
        timestamp: '14:30:29',
        audio_start_time: 24,
        speaker: 'mic',
      }),
      segment({ id: '3', text: 'Третий.', timestamp: '14:30:40', audio_start_time: 40, speaker: 'Guest A' }),
    ]);

    expect(payload.transcriptText).toBe('[00:00] Всем привет.\n[00:24] Добрый день.\n[00:40] Третий.');
    expect(payload.transcriptTexts).toEqual(['Всем привет.', 'Добрый день.', 'Третий.']);
  });

  test('a name without a diarization label does not add a prefix', () => {
    const payload = buildSummaryTranscriptPayload([
      segment({ text: 'Всем привет.', speaker_name: 'Alex' }),
      segment({ id: '2', text: 'Добрый день.', timestamp: '14:30:29', audio_start_time: 24 }),
    ]);

    expect(payload.transcriptText).toBe('[00:00] Всем привет.\n[00:24] Добрый день.');
  });

  test('segments without audio_start_time keep the wall-clock fallback', () => {
    const payload = buildSummaryTranscriptPayload([
      segment({ text: 'Всем привет.', timestamp: '14:30:05', audio_start_time: undefined }),
      segment({
        id: '2',
        text: 'Добрый день.',
        timestamp: '14:31:00',
        audio_start_time: 24,
        speaker: 'speaker_0',
        speaker_name: 'Alex',
      }),
    ]);

    expect(payload.transcriptText).toBe('14:30:05 Всем привет.\n[00:24] Alex: Добрый день.');
  });

  test('transcriptTexts stays the raw texts array, untrimmed', () => {
    const payload = buildSummaryTranscriptPayload([
      segment({ text: '  Всем привет.  ', speaker: 'speaker_0', speaker_name: 'Alex' }),
    ]);

    expect(payload.transcriptText).toBe('[00:00] Alex:   Всем привет.  ');
    expect(payload.transcriptTexts).toEqual(['  Всем привет.  ']);
  });

  test('an empty transcript list yields empty text and no texts', () => {
    expect(buildSummaryTranscriptPayload([])).toEqual({ transcriptText: '', transcriptTexts: [] });
  });
});
